use crate::commands::vectors::VectorDbState;
use crate::config;
use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub ollama: bool,
    pub vector_db: bool,
    pub api_key: bool,
    pub issues: Vec<String>,
}

#[tauri::command]
pub async fn health_check(app: tauri::AppHandle) -> Result<HealthStatus, String> {
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
    let vector_db = match check_vector_db(&app) {
        Ok(()) => true,
        Err(e) => {
            issues.push(format!("Vector DB: {e}"));
            false
        }
    };

    // Check API key presence
    let api_key = check_api_key(&cfg.openrouter.api_key);
    if !api_key {
        issues.push("OpenRouter API key not configured".into());
    }

    Ok(HealthStatus {
        ollama,
        vector_db,
        api_key,
        issues,
    })
}

async fn check_ollama(url: &str) -> Result<(), String> {
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

fn check_vector_db(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<VectorDbState>();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute_batch("SELECT 1")
        .map_err(|e| format!("query failed ({e})"))?;
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
    ];
    if !status.issues.is_empty() {
        lines.push(format!("Issues: {}", status.issues.join("; ")));
    }
    lines.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
            issues: vec![],
        };
        let s = format_health(&status);
        assert!(s.contains("Ollama: OK"));
        assert!(s.contains("Vector DB: OK"));
        assert!(s.contains("API Key: OK"));
        assert!(!s.contains("Issues"));
    }

    #[test]
    fn format_health_with_issues() {
        let status = HealthStatus {
            ollama: false,
            vector_db: true,
            api_key: false,
            issues: vec!["Ollama: down".into(), "No API key".into()],
        };
        let s = format_health(&status);
        assert!(s.contains("Ollama: FAIL"));
        assert!(s.contains("API Key: FAIL"));
        assert!(s.contains("Issues:"));
    }
}
