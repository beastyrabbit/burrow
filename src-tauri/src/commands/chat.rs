use crate::{
    chat::{self, ContextSnippet},
    config,
    context::AppContext,
    ollama,
};
use tauri::Manager;

/// Primary chat implementation — Tauri-free.
pub async fn chat_ask(query: String, ctx: &AppContext) -> Result<String, String> {
    let trimmed = query.trim_start_matches('?').trim();
    if trimmed.is_empty() {
        return Err("Empty question".into());
    }

    let cfg = config::get_config();
    let context_snippets = if cfg.vector_search.enabled {
        fetch_context_ctx(
            trimmed,
            ctx,
            cfg.vector_search.top_k,
            cfg.vector_search.min_score,
        )
        .await
    } else {
        vec![]
    };

    chat::generate_answer(trimmed, &context_snippets).await
}

/// Tauri command wrapper for chat_ask.
#[tauri::command]
pub async fn chat_ask_cmd(query: String, app: tauri::AppHandle) -> Result<String, String> {
    let ctx = app.state::<AppContext>();
    chat_ask(query, &ctx).await
}

/// Fetch context snippets using AppContext (Tauri-free).
async fn fetch_context_ctx(
    query: &str,
    ctx: &AppContext,
    top_k: usize,
    min_score: f32,
) -> Vec<ContextSnippet> {
    let embedding = match ollama::generate_embedding(query).await {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(error = %e, "failed to generate embedding for chat context");
            return vec![];
        }
    };

    let conn = match ctx.vector_db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "vector DB lock failed in chat");
            return vec![];
        }
    };

    fetch_context_from_conn(&conn, &embedding, top_k, min_score)
}

/// Retrieves the most relevant indexed file snippets for the query via vector similarity.
/// Returns an empty list on any failure to keep chat functional without vector search.
fn fetch_context_from_conn(
    conn: &rusqlite::Connection,
    embedding: &[f32],
    top_k: usize,
    min_score: f32,
) -> Vec<ContextSnippet> {
    let mut stmt = match conn.prepare("SELECT file_path, content_preview, embedding FROM vectors") {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to prepare chat vector query");
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
            tracing::warn!(error = %e, "failed to query vectors for chat");
            return vec![];
        }
    };

    let mut scored: Vec<(f32, String, String)> = rows
        .filter_map(|r| match r {
            Ok(row) => Some(row),
            Err(e) => {
                tracing::trace!(error = %e, "skipping corrupt vector row in chat");
                None
            }
        })
        .filter_map(|(path, preview, blob)| {
            let emb = ollama::deserialize_embedding(&blob);
            let score = ollama::cosine_similarity(embedding, &emb);
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
