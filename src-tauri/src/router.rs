use crate::commands::{apps, files, history, math, onepass, ssh};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub exec: String,
}

#[tauri::command]
pub async fn search(query: String, app: tauri::AppHandle) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return history::get_frecent(&app).map_err(|e| e.to_string());
    }

    if query.starts_with(' ') {
        let q = query.trim_start();
        if q.starts_with('*') {
            return Ok(vec![SearchResult {
                id: "vector-placeholder".into(),
                name: "Content search not yet available".into(),
                description: "Ollama integration pending".into(),
                icon: "".into(),
                category: "info".into(),
                exec: "".into(),
            }]);
        }
        return files::search_files(q);
    }

    if query.starts_with('!') {
        let q = query.trim_start_matches('!').trim();
        return onepass::search_onepass(q).await;
    }

    if query.starts_with("ssh ") || query == "ssh" {
        let q = query.strip_prefix("ssh").unwrap_or("").trim();
        return ssh::search_ssh(q);
    }

    if let Some(result) = math::try_calculate(&query) {
        return Ok(vec![result]);
    }

    apps::search_apps(&query)
}
