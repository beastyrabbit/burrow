use axum::{extract::State, http::StatusCode, routing::get, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

use crate::commands::{health, vectors};
use crate::config;
use crate::indexer::{self, IndexStats, IndexerProgress, IndexerState};

/// Shared daemon state for request handlers.
pub struct DaemonState {
    /// Indexer progress tracking
    pub indexer: IndexerState,
    /// When the daemon started
    pub started_at: Instant,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            indexer: IndexerState::new(),
            started_at: Instant::now(),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

/// Response for daemon status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub version: String,
    pub pid: u32,
    pub uptime_secs: u64,
}

/// Response for stats endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub indexed_files: i64,
    pub launch_count: i64,
    pub last_indexed: Option<String>,
}

/// Request body for starting indexer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerStartRequest {
    /// If true, performs full reindex (clears existing). If false, incremental update.
    #[serde(default)]
    pub full: bool,
}

/// Response for indexer start endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerStartResponse {
    pub started: bool,
    pub message: String,
}

// === Handlers ===

async fn daemon_status(
    State(state): State<Arc<DaemonState>>,
) -> Result<Json<DaemonStatus>, (StatusCode, String)> {
    let uptime = state.started_at.elapsed().as_secs();
    Ok(Json(DaemonStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        uptime_secs: uptime,
    }))
}

async fn daemon_shutdown() -> Result<Json<()>, (StatusCode, String)> {
    tracing::info!("shutdown requested via API");
    // Schedule shutdown after response is sent
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Ok(Json(()))
}

async fn indexer_progress(
    State(state): State<Arc<DaemonState>>,
) -> Result<Json<IndexerProgress>, (StatusCode, String)> {
    Ok(Json(state.indexer.get()))
}

async fn indexer_start(
    State(state): State<Arc<DaemonState>>,
    Json(body): Json<IndexerStartRequest>,
) -> Result<Json<IndexerStartResponse>, (StatusCode, String)> {
    let progress = state.indexer.get();
    if progress.running {
        return Ok(Json(IndexerStartResponse {
            started: false,
            message: "Indexer is already running".to_string(),
        }));
    }

    let state_clone = state.clone();
    let is_full = body.full;

    tokio::spawn(async move {
        let stats = if is_full {
            run_index_all(&state_clone.indexer).await
        } else {
            run_index_incremental(&state_clone.indexer).await
        };
        tracing::info!(
            action = if is_full { "reindex" } else { "update" },
            indexed = stats.indexed,
            skipped = stats.skipped,
            removed = stats.removed,
            errors = stats.errors,
            "daemon indexer run complete"
        );
    });

    Ok(Json(IndexerStartResponse {
        started: true,
        message: if is_full {
            "Reindex started".to_string()
        } else {
            "Incremental update started".to_string()
        },
    }))
}

async fn health_check_handler() -> Result<Json<health::HealthStatus>, (StatusCode, String)> {
    health::health_check_standalone()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn stats_handler() -> Result<Json<StatsResponse>, (StatusCode, String)> {
    let vconn = vectors::open_vector_db().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to open vector DB: {e}"),
        )
    })?;

    let file_count: i64 = vconn
        .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query file count: {e}"),
            )
        })?;

    let last_indexed: Option<f64> = vconn
        .query_row("SELECT MAX(indexed_at) FROM vectors", [], |r| r.get(0))
        .ok();

    let hconn = crate::commands::history::open_history_db().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to open history DB: {e}"),
        )
    })?;

    let launch_count: i64 = hconn
        .query_row("SELECT COUNT(*) FROM launches", [], |r| r.get(0))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query launch count: {e}"),
            )
        })?;

    let last_str = last_indexed.map(|_| "available".to_string());

    Ok(Json(StatsResponse {
        indexed_files: file_count,
        launch_count,
        last_indexed: last_str,
    }))
}

// === Standalone indexer functions (no Tauri state) ===

async fn run_index_all(progress: &IndexerState) -> IndexStats {
    let cfg = config::get_config();
    let mut stats = IndexStats::default();

    progress.start_standalone();

    // Clear existing vectors
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to open vector DB");
            progress.finish_standalone(format!("Failed to open DB: {e}"));
            return stats;
        }
    };

    if let Err(e) = conn.execute("DELETE FROM vectors", []) {
        tracing::error!(error = %e, "failed to clear vectors table");
    }
    drop(conn);

    let paths = indexer::collect_indexable_paths(cfg);
    let total = paths.len() as u32;
    progress.set_total(total);
    progress.set_phase("embedding");

    for path in &paths {
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.set_current_file(&name);

        match index_single_file_standalone(path, cfg.indexer.max_content_chars).await {
            Ok(()) => stats.indexed += 1,
            Err(e) => {
                tracing::debug!(path = %path.display(), error = %e, "failed to index file");
                stats.errors += 1;
            }
        }
        stats.skipped = total - stats.indexed - stats.errors;

        progress.inc_processed(stats.errors);
    }

    progress.finish_standalone(format!(
        "Indexed {} files, {} errors",
        stats.indexed, stats.errors
    ));
    stats
}

async fn run_index_incremental(progress: &IndexerState) -> IndexStats {
    let cfg = config::get_config();
    let mut stats = IndexStats::default();

    progress.start_standalone();

    // Get existing mtimes
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to open vector DB");
            progress.finish_standalone(format!("Failed to open DB: {e}"));
            return stats;
        }
    };

    let existing: std::collections::HashMap<String, f64> = {
        let mut stmt = match conn.prepare("SELECT file_path, file_mtime FROM vectors") {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to prepare mtime query");
                progress.finish_standalone(format!("DB query failed: {e}"));
                return stats;
            }
        };
        let result = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "failed to query mtimes");
                std::collections::HashMap::new()
            }
        };
        result
    };
    drop(conn);

    let all_paths = indexer::collect_indexable_paths(cfg);

    // Filter to only changed files
    let to_index: Vec<_> = all_paths
        .iter()
        .filter(|path| {
            let path_str = path.to_string_lossy().to_string();
            let mtime = indexer::file_mtime(path);
            match existing.get(&path_str) {
                Some(&db_mtime) => (mtime - db_mtime).abs() >= 1.0,
                None => true,
            }
        })
        .collect();

    let total = to_index.len() as u32;
    stats.skipped = all_paths.len() as u32 - total;

    progress.set_total(total);
    progress.set_phase("embedding");

    for path in &to_index {
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.set_current_file(&name);

        match index_single_file_standalone(path, cfg.indexer.max_content_chars).await {
            Ok(()) => stats.indexed += 1,
            Err(e) => {
                tracing::debug!(path = %path.display(), error = %e, "failed to index file");
                stats.errors += 1;
            }
        }

        progress.inc_processed(stats.errors);
    }

    // Cleanup stale entries
    progress.set_phase("cleanup");
    let valid_paths: std::collections::HashSet<String> = all_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    stats.removed = cleanup_stale_standalone(&valid_paths);

    progress.finish_standalone(format!(
        "Indexed {}, skipped {}, removed {}, {} errors",
        stats.indexed, stats.skipped, stats.removed, stats.errors
    ));
    stats
}

async fn index_single_file_standalone(
    path: &std::path::Path,
    max_content_chars: usize,
) -> Result<(), String> {
    let cfg = config::get_config();
    let content = crate::text_extract::extract_text(path, max_content_chars)?;
    let embedding = crate::ollama::generate_embedding(&content).await?;
    let preview: String = content.chars().take(200).collect();
    let mtime = indexer::file_mtime(path);
    let path_str = path.to_string_lossy().to_string();

    let conn = vectors::open_vector_db().map_err(|e| e.to_string())?;
    vectors::insert_vector(
        &conn,
        &path_str,
        &preview,
        &embedding,
        &cfg.ollama.embedding_model,
        mtime,
    )
    .map_err(|e| e.to_string())
}

fn cleanup_stale_standalone(valid_paths: &std::collections::HashSet<String>) -> u32 {
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to open vector DB for cleanup");
            return 0;
        }
    };

    let paths: Vec<String> = {
        let mut stmt = match conn.prepare("SELECT file_path FROM vectors") {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to prepare cleanup query");
                return 0;
            }
        };
        let result = match stmt.query_map([], |row| row.get(0)) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::error!(error = %e, "failed to query paths for cleanup");
                return 0;
            }
        };
        result
    };

    let mut removed = 0u32;
    for path in paths {
        if !std::path::Path::new(&path).exists() || !valid_paths.contains(&path) {
            if let Err(e) = conn.execute("DELETE FROM vectors WHERE file_path = ?1", [&path]) {
                tracing::warn!(path, error = %e, "failed to delete stale vector");
            } else {
                removed += 1;
            }
        }
    }
    removed
}

/// Create the daemon router with all endpoints.
pub fn create_router(state: Arc<DaemonState>) -> Router {
    Router::new()
        // Daemon lifecycle
        .route("/daemon/status", get(daemon_status))
        .route("/daemon/shutdown", post(daemon_shutdown))
        // Indexer operations
        .route("/indexer/progress", get(indexer_progress))
        .route("/indexer/start", post(indexer_start))
        // Health and stats
        .route("/health", get(health_check_handler))
        .route("/stats", get(stats_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_state_default_not_running() {
        let state = DaemonState::new();
        let progress = state.indexer.get();
        assert!(!progress.running);
    }

    #[test]
    fn daemon_status_has_version() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(!version.is_empty());
    }

    #[test]
    fn indexer_start_request_default_not_full() {
        let req: IndexerStartRequest = serde_json::from_str("{}").unwrap();
        assert!(!req.full);
    }

    #[test]
    fn indexer_start_request_full() {
        let req: IndexerStartRequest = serde_json::from_str(r#"{"full": true}"#).unwrap();
        assert!(req.full);
    }

    #[test]
    fn stats_response_serializes() {
        let resp = StatsResponse {
            indexed_files: 100,
            launch_count: 50,
            last_indexed: Some("available".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("100"));
        assert!(json.contains("50"));
    }
}
