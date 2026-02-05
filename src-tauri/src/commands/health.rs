use crate::config;
use crate::context::AppContext;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub ollama: bool,
    pub vector_db: bool,
    pub api_key: bool,
    pub indexing: bool,
    pub issues: Vec<String>,
}

/// Shared health check logic - checks Ollama and constructs status.
/// `vector_db_check` is a closure that performs the DB check (different for Tauri vs CLI).
async fn health_check_core<F>(vector_db_check: F, indexing: bool) -> Result<HealthStatus, String>
where
    F: FnOnce() -> Result<(), String>,
{
    let cfg = config::get_config();
    let mut issues = Vec::new();

    // Check Ollama connectivity
    let ollama = match check_ollama(&cfg.ollama.url).await {
        Ok(()) => true,
        Err(e) => {
            issues.push(format!("Ollama: {e}"));
            false
        }
    };

    // Check vector DB accessibility
    let vector_db = match vector_db_check() {
        Ok(()) => true,
        Err(e) => {
            issues.push(format!("Vector DB: {e}"));
            false
        }
    };

    // API key is optional - don't report as health issue per coding guidelines
    let api_key = check_api_key(&cfg.openrouter.api_key);

    Ok(HealthStatus {
        ollama,
        vector_db,
        api_key,
        indexing,
        issues,
    })
}

/// Primary health check — Tauri-free.
pub async fn health_check(ctx: &AppContext) -> Result<HealthStatus, String> {
    let indexing = ctx.indexer.get().running;
    health_check_core(
        || {
            let conn = ctx.vector_db.lock()?;
            check_vector_db_conn(&conn)
        },
        indexing,
    )
    .await
}

/// Tauri command wrapper for health_check.
#[tauri::command]
pub async fn health_check_cmd(app: tauri::AppHandle) -> Result<HealthStatus, String> {
    let ctx = app.state::<AppContext>();
    health_check(ctx.inner()).await
}

/// Standalone health check for CLI (no Tauri state)
pub async fn health_check_standalone() -> Result<HealthStatus, String> {
    health_check_core(check_vector_db_standalone, false).await
}

fn check_vector_db_standalone() -> Result<(), String> {
    let conn = super::vectors::open_vector_db().map_err(|e| format!("open failed ({e})"))?;
    check_vector_db_conn(&conn)
}

/// Pure helper to check DB connectivity - testable with in-memory connections.
fn check_vector_db_conn(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch("SELECT 1")
        .map_err(|e| format!("query failed ({e})"))?;
    Ok(())
}

async fn check_ollama(url: &str) -> Result<(), String> {
    if crate::actions::dry_run::is_enabled() {
        tracing::debug!(url, "[dry-run] check_ollama");
        // Assume healthy in dry-run to avoid false alarms in tests
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!("{url}/api/tags"))
        .send()
        .await
        .map_err(|e| format!("unreachable ({e})"))?;

    resp.error_for_status()
        .map_err(|e| format!("unhealthy ({e})"))?;

    Ok(())
}

fn check_api_key(key: &str) -> bool {
    !key.trim().is_empty()
}

pub fn format_health(status: &HealthStatus) -> String {
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut lines = vec![
        format!("Ollama: {}", ok(status.ollama)),
        format!("Vector DB: {}", ok(status.vector_db)),
        format!("API Key: {}", ok(status.api_key)),
        format!(
            "Indexer: {}",
            if status.indexing { "running" } else { "idle" }
        ),
    ];
    if !status.issues.is_empty() {
        lines.push(format!("Issues: {}", status.issues.join("; ")));
    }
    lines.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn check_vector_db_conn_ok() {
        let conn = Connection::open_in_memory().unwrap();
        check_vector_db_conn(&conn).unwrap();
    }

    #[test]
    fn api_key_empty_is_unhealthy() {
        assert!(!check_api_key(""));
    }

    #[test]
    fn api_key_present_is_healthy() {
        assert!(check_api_key("sk-test-123"));
    }

    #[test]
    fn api_key_whitespace_only_is_unhealthy() {
        assert!(!check_api_key("   "));
    }

    #[test]
    fn health_status_all_ok_has_no_issues() {
        let status = HealthStatus {
            ollama: true,
            vector_db: true,
            api_key: true,
            indexing: false,
            issues: vec![],
        };
        assert!(status.ollama && status.vector_db && status.api_key);
        assert!(status.issues.is_empty());
    }

    #[test]
    fn health_status_failure_has_issues() {
        let status = HealthStatus {
            ollama: false,
            vector_db: true,
            api_key: true,
            indexing: false,
            issues: vec!["Ollama: unreachable".into()],
        };
        assert!(!status.ollama);
        assert_eq!(status.issues.len(), 1);
    }

    #[test]
    fn format_health_all_ok() {
        let status = HealthStatus {
            ollama: true,
            vector_db: true,
            api_key: true,
            indexing: false,
            issues: vec![],
        };
        let s = format_health(&status);
        assert!(s.contains("Ollama: OK"));
        assert!(s.contains("Vector DB: OK"));
        assert!(s.contains("API Key: OK"));
        assert!(s.contains("Indexer: idle"));
        assert!(!s.contains("Issues"));
    }

    #[test]
    fn format_health_with_issues() {
        let status = HealthStatus {
            ollama: false,
            vector_db: true,
            api_key: false,
            indexing: false,
            issues: vec!["Ollama: down".into(), "No API key".into()],
        };
        let s = format_health(&status);
        assert!(s.contains("Ollama: FAIL"));
        assert!(s.contains("API Key: FAIL"));
        assert!(s.contains("Issues:"));
    }

    #[test]
    fn format_health_indexing() {
        let status = HealthStatus {
            ollama: true,
            vector_db: true,
            api_key: true,
            indexing: true,
            issues: vec![],
        };
        let s = format_health(&status);
        assert!(s.contains("Indexer: running"));
    }
}
