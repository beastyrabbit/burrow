use super::vectors::VectorDbState;
use crate::{
    chat::{self, ContextSnippet},
    config, ollama,
};
use tauri::Manager;

#[tauri::command]
pub async fn chat_ask(query: String, app: tauri::AppHandle) -> Result<String, String> {
    let trimmed = query.trim_start_matches('?').trim();
    if trimmed.is_empty() {
        return Err("Empty question".into());
    }

    let cfg = config::get_config();
    let context_snippets = if cfg.vector_search.enabled {
        fetch_context(
            trimmed,
            &app,
            cfg.vector_search.top_k,
            cfg.vector_search.min_score,
        )
        .await
    } else {
        vec![]
    };

    chat::generate_answer(trimmed, &context_snippets).await
}

/// Retrieves the most relevant indexed file snippets for the query via vector similarity.
/// Returns an empty list on any failure to keep chat functional without vector search.
async fn fetch_context(
    query: &str,
    app: &tauri::AppHandle,
    top_k: usize,
    min_score: f32,
) -> Vec<ContextSnippet> {
    let embedding = match ollama::generate_embedding(query).await {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[chat] Failed to generate embedding for context: {e}");
            return vec![];
        }
    };

    let state = app.state::<VectorDbState>();
    let conn = match state.lock() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[chat] Vector DB lock failed: {e}");
            return vec![];
        }
    };

    let mut stmt = match conn.prepare("SELECT file_path, content_preview, embedding FROM vectors") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[chat] Failed to prepare vector query: {e}");
            return vec![];
        }
    };

    let rows = match stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let preview: String = row.get(1)?;
        let blob: Vec<u8> = row.get(2)?;
        Ok((path, preview, blob))
    }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[chat] Failed to query vectors: {e}");
            return vec![];
        }
    };

    let mut scored: Vec<(f32, String, String)> = rows
        .filter_map(|r| match r {
            Ok(row) => Some(row),
            Err(e) => {
                eprintln!("[chat] Skipping corrupt vector row: {e}");
                None
            }
        })
        .filter_map(|(path, preview, blob)| {
            let emb = ollama::deserialize_embedding(&blob);
            let score = ollama::cosine_similarity(&embedding, &emb);
            if score >= min_score {
                Some((score, path, preview))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    scored
        .into_iter()
        .map(|(_, path, preview)| ContextSnippet { path, preview })
        .collect()
}
