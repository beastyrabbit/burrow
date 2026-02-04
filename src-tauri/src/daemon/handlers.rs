use axum::{extract::State, http::StatusCode, routing::get, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

use crate::chat::{self, ContextSnippet};
use crate::commands::{health, vectors};
use crate::config;
use crate::indexer::{self, IndexStats, IndexerProgress, IndexerState};
use crate::ollama;

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

/// Request body for chat endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub query: String,
    /// Use small model instead of large
    #[serde(default)]
    pub small: bool,
}

/// Response for chat endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub answer: String,
    pub model: String,
    pub provider: String,
}

/// Information about a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
}

/// Response for models list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponse {
    pub embedding: ModelInfo,
    pub chat: ModelInfo,
    pub chat_large: ModelInfo,
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
    // Schedule graceful shutdown after response is sent
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        crate::daemon::socket::trigger_shutdown();
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
        progress.finish_standalone(format!("Failed to clear vectors: {e}"));
        return stats;
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
            Ok(rows) => rows
                .filter_map(|r| match r {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping corrupted mtime row");
                        None
                    }
                })
                .collect(),
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
                Some(&db_mtime) => indexer::is_file_modified(mtime, db_mtime),
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
        &cfg.models.embedding.name,
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
            Ok(rows) => rows
                .filter_map(|r| match r {
                    Ok(p) => Some(p),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping corrupted path row during cleanup");
                        None
                    }
                })
                .collect(),
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

// === Chat handlers ===

/// Select chat model based on small flag
fn select_chat_model(cfg: &config::AppConfig, small: bool) -> &config::ModelSpec {
    if small {
        &cfg.models.chat
    } else {
        &cfg.models.chat_large
    }
}

async fn chat_handler(
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    if body.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Query cannot be empty".to_string()));
    }

    tracing::info!(query = %body.query.chars().take(50).collect::<String>(), small = body.small, "chat request");

    let cfg = config::get_config();
    let model = select_chat_model(cfg, body.small);

    let answer = chat::generate_chat(&body.query, &[], model)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(ChatResponse {
        answer,
        model: model.name.clone(),
        provider: model.provider.clone(),
    }))
}

async fn chat_docs_handler(
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    if body.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Query cannot be empty".to_string()));
    }

    tracing::info!(query = %body.query.chars().take(50).collect::<String>(), small = body.small, "chat-docs request");

    let cfg = config::get_config();
    let model = select_chat_model(cfg, body.small);

    // Fetch context from vector DB
    let context = fetch_context_for_query(&body.query, cfg)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let answer = chat::generate_chat(&body.query, &context, model)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(ChatResponse {
        answer,
        model: model.name.clone(),
        provider: model.provider.clone(),
    }))
}

/// Fetch context snippets from vector DB for RAG
async fn fetch_context_for_query(
    query: &str,
    cfg: &config::AppConfig,
) -> Result<Vec<ContextSnippet>, String> {
    if !cfg.chat.rag_enabled {
        return Ok(vec![]);
    }

    let query_embedding = ollama::generate_embedding(query).await?;

    let conn = vectors::open_vector_db().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT file_path, content_preview, embedding FROM vectors")
        .map_err(|e| format!("DB query failed: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| format!("Failed to query vectors: {e}"))?;

    let mut scored: Vec<(f32, String, String)> = vec![];

    for row in rows.filter_map(|r| match r {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "skipping corrupted vector row in context query");
            None
        }
    }) {
        let (path, preview, embedding_bytes) = row;
        let embedding = ollama::deserialize_embedding(&embedding_bytes);
        let score = ollama::cosine_similarity(&query_embedding, &embedding);

        if score >= cfg.vector_search.min_score {
            scored.push((score, path, preview));
        }
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Take top_k results
    let context: Vec<ContextSnippet> = scored
        .into_iter()
        .take(cfg.chat.max_context_snippets)
        .map(|(_, path, preview)| ContextSnippet { path, preview })
        .collect();

    Ok(context)
}

// === Models handler ===

async fn models_list_handler() -> Result<Json<ModelsListResponse>, (StatusCode, String)> {
    let cfg = config::get_config();

    Ok(Json(ModelsListResponse {
        embedding: ModelInfo {
            name: cfg.models.embedding.name.clone(),
            provider: cfg.models.embedding.provider.clone(),
        },
        chat: ModelInfo {
            name: cfg.models.chat.name.clone(),
            provider: cfg.models.chat.provider.clone(),
        },
        chat_large: ModelInfo {
            name: cfg.models.chat_large.name.clone(),
            provider: cfg.models.chat_large.provider.clone(),
        },
    }))
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
        // Chat operations
        .route("/chat", post(chat_handler))
        .route("/chat/docs", post(chat_docs_handler))
        // Models
        .route("/models", get(models_list_handler))
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

    #[test]
    fn chat_request_deserializes_minimal() {
        let req: ChatRequest = serde_json::from_str(r#"{"query": "hello"}"#).unwrap();
        assert_eq!(req.query, "hello");
        assert!(!req.small);
    }

    #[test]
    fn chat_request_deserializes_with_small() {
        let req: ChatRequest = serde_json::from_str(r#"{"query": "hi", "small": true}"#).unwrap();
        assert_eq!(req.query, "hi");
        assert!(req.small);
    }

    #[test]
    fn chat_response_serializes() {
        let resp = ChatResponse {
            answer: "Hello world".to_string(),
            model: "llama3:8b".to_string(),
            provider: "ollama".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Hello world"));
        assert!(json.contains("llama3:8b"));
        assert!(json.contains("ollama"));
    }

    #[test]
    fn models_list_response_serializes() {
        let resp = ModelsListResponse {
            embedding: ModelInfo {
                name: "nomic-embed-text".to_string(),
                provider: "ollama".to_string(),
            },
            chat: ModelInfo {
                name: "llama3:8b".to_string(),
                provider: "ollama".to_string(),
            },
            chat_large: ModelInfo {
                name: "claude-sonnet-4-20250514".to_string(),
                provider: "openrouter".to_string(),
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("nomic-embed-text"));
        assert!(json.contains("llama3:8b"));
        assert!(json.contains("claude-sonnet-4-20250514"));
        assert!(json.contains("openrouter"));
    }
}
