use crate::ollama;
use crate::router::SearchResult;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct VectorDbState(pub Mutex<Connection>);

fn vector_db_path() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("burrow");
    std::fs::create_dir_all(&dir).ok();
    dir.join("vectors.db")
}

fn create_vector_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vectors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path TEXT NOT NULL UNIQUE,
            content_preview TEXT NOT NULL,
            embedding BLOB NOT NULL,
            dimension INTEGER NOT NULL,
            model TEXT NOT NULL,
            indexed_at REAL NOT NULL,
            file_mtime REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_vectors_path ON vectors(file_path);
        CREATE INDEX IF NOT EXISTS idx_vectors_mtime ON vectors(file_mtime);",
    )
}

pub fn init_vector_db(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open(vector_db_path())?;
    create_vector_table(&conn)?;
    app.manage(VectorDbState(Mutex::new(conn)));
    Ok(())
}

fn search_vectors(
    conn: &Connection,
    query_embedding: &[f32],
    top_k: usize,
    min_score: f32,
) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT file_path, content_preview, embedding FROM vectors")?;

    let mut scored: Vec<(f32, String, String)> = stmt
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let preview: String = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            Ok((path, preview, blob))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(path, preview, blob)| {
            let embedding = ollama::deserialize_embedding(&blob);
            let score = ollama::cosine_similarity(query_embedding, &embedding);
            if score >= min_score {
                Some((score, path, preview))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    Ok(scored
        .into_iter()
        .map(|(score, path, preview)| {
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let open_cmd = format!("xdg-open {}", path);
            SearchResult {
                id: path.clone(),
                name,
                description: format!("{:.0}% — {}", score * 100.0, preview),
                icon: "".into(),
                category: "vector".into(),
                exec: open_cmd,
            }
        })
        .collect())
}

pub async fn search_by_content(query: &str, app: &AppHandle) -> Result<Vec<SearchResult>, String> {
    let cfg = crate::config::get_config();
    if !cfg.vector_search.enabled {
        return Ok(vec![SearchResult {
            id: "vector-disabled".into(),
            name: "Vector search is disabled".into(),
            description: "Enable in ~/.config/burrow/config.toml".into(),
            icon: "".into(),
            category: "info".into(),
            exec: "".into(),
        }]);
    }

    let query_embedding = ollama::generate_embedding(query).await?;

    let state = app.state::<VectorDbState>();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    search_vectors(
        &conn,
        &query_embedding,
        cfg.vector_search.top_k,
        cfg.vector_search.min_score,
    )
    .map_err(|e| e.to_string())
}

pub fn insert_vector(
    conn: &Connection,
    file_path: &str,
    content_preview: &str,
    embedding: &[f32],
    model: &str,
    file_mtime: f64,
) -> Result<(), rusqlite::Error> {
    let blob = ollama::serialize_embedding(embedding);
    conn.execute(
        "INSERT INTO vectors (file_path, content_preview, embedding, dimension, model, indexed_at, file_mtime)
         VALUES (?1, ?2, ?3, ?4, ?5, julianday('now'), ?6)
         ON CONFLICT(file_path) DO UPDATE SET
           content_preview = ?2, embedding = ?3, dimension = ?4,
           model = ?5, indexed_at = julianday('now'), file_mtime = ?6",
        rusqlite::params![
            file_path,
            content_preview,
            blob,
            embedding.len() as i32,
            model,
            file_mtime,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_vector_table(&conn).unwrap();
        conn
    }

    #[test]
    fn create_table_succeeds() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(create_vector_table(&conn).is_ok());
    }

    #[test]
    fn create_table_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_vector_table(&conn).unwrap();
        assert!(create_vector_table(&conn).is_ok());
    }

    #[test]
    fn insert_and_search() {
        let conn = test_db();
        let emb = vec![1.0, 0.0, 0.0];
        insert_vector(
            &conn,
            "/home/user/doc.txt",
            "hello world",
            &emb,
            "test-model",
            0.0,
        )
        .unwrap();

        let query_emb = vec![1.0, 0.0, 0.0]; // identical
        let results = search_vectors(&conn, &query_emb, 10, 0.0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "/home/user/doc.txt");
        assert_eq!(results[0].name, "doc.txt");
        assert_eq!(results[0].category, "vector");
    }

    #[test]
    fn search_respects_min_score() {
        let conn = test_db();
        let emb = vec![1.0, 0.0, 0.0];
        insert_vector(&conn, "/path/a.txt", "a", &emb, "m", 0.0).unwrap();

        let query = vec![0.0, 1.0, 0.0]; // orthogonal → score ≈ 0
        let results = search_vectors(&conn, &query, 10, 0.5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_top_k() {
        let conn = test_db();
        for i in 0..20 {
            let emb = vec![1.0, i as f32 * 0.01, 0.0];
            insert_vector(
                &conn,
                &format!("/path/f{i}.txt"),
                &format!("file {i}"),
                &emb,
                "m",
                0.0,
            )
            .unwrap();
        }
        let query = vec![1.0, 0.0, 0.0];
        let results = search_vectors(&conn, &query, 5, 0.0).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn search_sorted_by_score_desc() {
        let conn = test_db();
        // Close match
        insert_vector(&conn, "/close.txt", "close", &[1.0, 0.1, 0.0], "m", 0.0).unwrap();
        // Far match
        insert_vector(&conn, "/far.txt", "far", &[0.5, 0.8, 0.3], "m", 0.0).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = search_vectors(&conn, &query, 10, 0.0).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "/close.txt");
    }

    #[test]
    fn upsert_updates_existing() {
        let conn = test_db();
        insert_vector(&conn, "/path.txt", "old", &[1.0, 0.0], "m", 0.0).unwrap();
        insert_vector(&conn, "/path.txt", "new", &[0.0, 1.0], "m", 1.0).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // Verify updated content
        let preview: String = conn
            .query_row(
                "SELECT content_preview FROM vectors WHERE file_path = '/path.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(preview, "new");
    }

    #[test]
    fn empty_db_returns_empty() {
        let conn = test_db();
        let results = search_vectors(&conn, &[1.0, 0.0], 10, 0.0).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn result_description_contains_score_and_preview() {
        let conn = test_db();
        insert_vector(
            &conn,
            "/doc.txt",
            "important document",
            &[1.0, 0.0],
            "m",
            0.0,
        )
        .unwrap();
        let results = search_vectors(&conn, &[1.0, 0.0], 10, 0.0).unwrap();
        assert!(results[0].description.contains("important document"));
        assert!(results[0].description.contains("%"));
    }

    #[test]
    fn stores_dimension() {
        let conn = test_db();
        insert_vector(&conn, "/doc.txt", "x", &[1.0; 384], "m", 0.0).unwrap();
        let dim: i32 = conn
            .query_row(
                "SELECT dimension FROM vectors WHERE file_path = '/doc.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dim, 384);
    }

    #[test]
    fn stores_model_name() {
        let conn = test_db();
        insert_vector(&conn, "/doc.txt", "x", &[1.0], "qwen3-embedding:8b", 0.0).unwrap();
        let model: String = conn
            .query_row(
                "SELECT model FROM vectors WHERE file_path = '/doc.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(model, "qwen3-embedding:8b");
    }

    #[test]
    fn embedding_blob_roundtrip() {
        let conn = test_db();
        let original = vec![1.5f32, -2.3, 0.0, 42.0];
        insert_vector(&conn, "/doc.txt", "x", &original, "m", 0.0).unwrap();

        let blob: Vec<u8> = conn
            .query_row(
                "SELECT embedding FROM vectors WHERE file_path = '/doc.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let recovered = ollama::deserialize_embedding(&blob);
        assert_eq!(original, recovered);
    }
}
