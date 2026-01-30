use burrow_lib::commands::vectors::insert_vector;
use burrow_lib::ollama::{cosine_similarity, deserialize_embedding};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
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
        CREATE INDEX IF NOT EXISTS idx_vectors_path ON vectors(file_path);",
    )
    .unwrap();
    conn
}

fn search_vectors(
    conn: &Connection,
    query_embedding: &[f32],
    top_k: usize,
    min_score: f32,
    model: &str,
) -> Vec<(f32, String, String)> {
    let dim = query_embedding.len() as i32;
    let mut stmt = conn
        .prepare("SELECT file_path, content_preview, embedding FROM vectors WHERE model = ?1 AND dimension = ?2")
        .unwrap();

    let mut scored: Vec<(f32, String, String)> = stmt
        .query_map(rusqlite::params![model, dim], |row| {
            let path: String = row.get(0)?;
            let preview: String = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            Ok((path, preview, blob))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .filter_map(|(path, preview, blob)| {
            let embedding = deserialize_embedding(&blob);
            let score = cosine_similarity(query_embedding, &embedding);
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
}

#[test]
fn full_insert_search_cycle() {
    let conn = setup_db();

    // Insert 10 vectors with varying similarity to [1,0,0]
    for i in 0..10 {
        let emb = vec![1.0 - (i as f32 * 0.1), i as f32 * 0.1, 0.0];
        insert_vector(
            &conn,
            &format!("/path/file_{i}.txt"),
            &format!("File {i} content"),
            &emb,
            "test-model",
            i as f64,
        )
        .unwrap();
    }

    let query = vec![1.0, 0.0, 0.0];
    let results = search_vectors(&conn, &query, 5, 0.0, "test-model");
    assert_eq!(results.len(), 5);
    // First result should be most similar (file_0)
    assert_eq!(results[0].1, "/path/file_0.txt");
}

#[test]
fn upsert_preserves_single_row() {
    let conn = setup_db();
    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![0.0, 1.0, 0.0];

    insert_vector(&conn, "/same/path.txt", "old", &emb1, "m", 0.0).unwrap();
    insert_vector(&conn, "/same/path.txt", "new", &emb2, "m", 1.0).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);

    let preview: String = conn
        .query_row(
            "SELECT content_preview FROM vectors WHERE file_path = '/same/path.txt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(preview, "new");
}

#[test]
fn search_filters_by_model() {
    let conn = setup_db();
    let emb = vec![1.0, 0.0, 0.0];
    insert_vector(&conn, "/a.txt", "a", &emb, "model_a", 0.0).unwrap();

    // Search with model_b should find nothing
    let results = search_vectors(&conn, &emb, 10, 0.0, "model_b");
    assert!(results.is_empty());

    // Search with model_a should find it
    let results = search_vectors(&conn, &emb, 10, 0.0, "model_a");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_filters_by_dimension() {
    let conn = setup_db();
    let emb_3d = vec![1.0, 0.0, 0.0];
    insert_vector(&conn, "/a.txt", "a", &emb_3d, "m", 0.0).unwrap();

    // Query with 5D vector — dimension mismatch
    let query_5d = vec![1.0, 0.0, 0.0, 0.0, 0.0];
    let results = search_vectors(&conn, &query_5d, 10, 0.0, "m");
    assert!(results.is_empty());
}

#[test]
fn embedding_serialization_roundtrip_through_db() {
    let conn = setup_db();
    let original = vec![1.5f32, -2.3, 0.0, 42.0, std::f32::consts::PI];
    insert_vector(&conn, "/doc.txt", "x", &original, "m", 0.0).unwrap();

    let blob: Vec<u8> = conn
        .query_row(
            "SELECT embedding FROM vectors WHERE file_path = '/doc.txt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let recovered = deserialize_embedding(&blob);
    assert_eq!(original, recovered);
}

#[test]
fn min_score_filtering_works() {
    let conn = setup_db();
    // Insert orthogonal vector
    insert_vector(&conn, "/orth.txt", "x", &[0.0, 1.0, 0.0], "m", 0.0).unwrap();
    // Insert parallel vector
    insert_vector(&conn, "/para.txt", "y", &[1.0, 0.0, 0.0], "m", 0.0).unwrap();

    let query = vec![1.0, 0.0, 0.0];
    let results = search_vectors(&conn, &query, 10, 0.5, "m");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, "/para.txt");
}
