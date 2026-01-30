use crate::commands::vectors::{self, VectorDbState};
use crate::config;
use crate::ollama;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::Manager;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Default)]
pub struct IndexStats {
    pub indexed: u32,
    pub skipped: u32,
    pub removed: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexerProgress {
    pub running: bool,
    pub phase: String, // "idle", "scanning", "embedding", "cleanup", "done"
    pub current_file: String,
    pub processed: u32,
    pub total: u32,
    pub errors: u32,
    pub last_result: String,
}

impl Default for IndexerProgress {
    fn default() -> Self {
        Self {
            running: false,
            phase: "idle".into(),
            current_file: String::new(),
            processed: 0,
            total: 0,
            errors: 0,
            last_result: String::new(),
        }
    }
}

pub struct IndexerState(pub Mutex<IndexerProgress>);

impl IndexerState {
    pub fn new() -> Self {
        Self(Mutex::new(IndexerProgress::default()))
    }

    fn update(&self, f: impl FnOnce(&mut IndexerProgress)) {
        if let Ok(mut p) = self.0.lock() {
            f(&mut p);
        }
    }

    pub fn get(&self) -> IndexerProgress {
        self.0.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

/// Default extensions used in tests when no config is available.
#[cfg(test)]
const DEFAULT_EXTENSIONS: &[&str] = &[
    "txt", "md", "rs", "ts", "tsx", "js", "py", "toml", "yaml", "yml", "json", "sh", "css", "html",
    "pdf", "docx", "xlsx", "xls", "pptx", "odt", "ods", "odp", "csv", "rtf",
];

pub fn is_indexable_file(path: &Path, max_size: u64, extensions: &[String]) -> bool {
    // Skip hidden files (dotfiles) — only check the filename itself
    if let Some(name) = path.file_name() {
        if name.to_string_lossy().starts_with('.') {
            return false;
        }
    }

    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    if !extensions.iter().any(|e| e == &ext) {
        return false;
    }

    match path.metadata() {
        Ok(m) => m.len() <= max_size && m.is_file(),
        Err(_) => false,
    }
}

fn is_hidden_entry(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn file_mtime(path: &Path) -> f64 {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

fn collect_indexable_paths(cfg: &config::AppConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in &cfg.vector_search.index_dirs {
        let dir_path = expand_tilde(dir);
        if !dir_path.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !is_hidden_entry(e))
            .filter_map(|e| e.ok())
        {
            if is_indexable_file(
                entry.path(),
                cfg.vector_search.max_file_size_bytes,
                &cfg.indexer.file_extensions,
            ) {
                paths.push(entry.into_path());
            }
        }
    }
    paths
}

pub async fn index_all(app: &tauri::AppHandle) -> IndexStats {
    let cfg = config::get_config();
    let db = app.state::<VectorDbState>();
    let progress = app.state::<IndexerState>();
    let mut stats = IndexStats::default();

    progress.update(|p| {
        p.running = true;
        p.phase = "scanning".into();
        p.processed = 0;
        p.total = 0;
        p.errors = 0;
        p.current_file.clear();
    });

    // Drop all existing vectors first
    {
        let conn = db.0.lock().unwrap();
        conn.execute("DELETE FROM vectors", []).ok();
    }

    let paths = collect_indexable_paths(cfg);
    let total = paths.len() as u32;
    progress.update(|p| {
        p.phase = "embedding".into();
        p.total = total;
    });

    for path in &paths {
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.update(|p| p.current_file = name);

        match index_single_file(
            path,
            &db,
            &cfg.ollama.embedding_model,
            cfg.indexer.max_content_chars,
        )
        .await
        {
            Ok(()) => stats.indexed += 1,
            Err(_) => stats.errors += 1,
        }
        stats.skipped = total - stats.indexed - stats.errors;

        progress.update(|p| {
            p.processed = stats.indexed + stats.errors;
            p.errors = stats.errors;
        });
    }

    let result = format!("Indexed {} files, {} errors", stats.indexed, stats.errors);
    progress.update(|p| {
        p.running = false;
        p.phase = "idle".into();
        p.current_file.clear();
        p.last_result = result;
    });

    stats
}

pub async fn index_incremental(app: &tauri::AppHandle) -> IndexStats {
    let cfg = config::get_config();
    let db = app.state::<VectorDbState>();
    let progress = app.state::<IndexerState>();
    let mut stats = IndexStats::default();

    progress.update(|p| {
        p.running = true;
        p.phase = "scanning".into();
        p.processed = 0;
        p.total = 0;
        p.errors = 0;
        p.current_file.clear();
    });

    // Collect existing mtimes from DB
    let existing: std::collections::HashMap<String, f64> = {
        let conn = db.0.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT file_path, file_mtime FROM vectors")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    };

    let all_paths = collect_indexable_paths(cfg);

    // Filter to only changed files
    let to_index: Vec<&PathBuf> = all_paths
        .iter()
        .filter(|path| {
            let path_str = path.to_string_lossy().to_string();
            let mtime = file_mtime(path);
            match existing.get(&path_str) {
                Some(&db_mtime) => (mtime - db_mtime).abs() >= 1.0,
                None => true,
            }
        })
        .collect();

    let total = to_index.len() as u32;
    stats.skipped = all_paths.len() as u32 - total;

    progress.update(|p| {
        p.phase = "embedding".into();
        p.total = total;
    });

    for path in &to_index {
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.update(|p| p.current_file = name);

        match index_single_file(
            path,
            &db,
            &cfg.ollama.embedding_model,
            cfg.indexer.max_content_chars,
        )
        .await
        {
            Ok(()) => stats.indexed += 1,
            Err(_) => stats.errors += 1,
        }

        progress.update(|p| {
            p.processed = stats.indexed + stats.errors;
            p.errors = stats.errors;
        });
    }

    // Cleanup stale entries
    progress.update(|p| p.phase = "cleanup".into());
    stats.removed = cleanup_stale(&db);

    let result = format!(
        "Indexed {}, skipped {}, removed {}, {} errors",
        stats.indexed, stats.skipped, stats.removed, stats.errors
    );
    progress.update(|p| {
        p.running = false;
        p.phase = "idle".into();
        p.current_file.clear();
        p.last_result = result;
    });

    stats
}

async fn index_single_file(
    path: &Path,
    state: &VectorDbState,
    model: &str,
    max_content_chars: usize,
) -> Result<(), String> {
    let content = crate::text_extract::extract_text(path, max_content_chars)?;

    let embedding = ollama::generate_embedding(&content).await?;

    let preview: String = content.chars().take(200).collect();
    let mtime = file_mtime(path);
    let path_str = path.to_string_lossy().to_string();

    let conn = state.0.lock().map_err(|e| e.to_string())?;
    vectors::insert_vector(&conn, &path_str, &preview, &embedding, model, mtime)
        .map_err(|e| e.to_string())
}

fn cleanup_stale(state: &VectorDbState) -> u32 {
    let conn = state.0.lock().unwrap();
    let paths: Vec<String> = {
        let mut stmt = conn.prepare("SELECT file_path FROM vectors").unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };

    let mut removed = 0u32;
    for path in paths {
        if !Path::new(&path).exists() {
            conn.execute("DELETE FROM vectors WHERE file_path = ?1", [&path])
                .ok();
            removed += 1;
        }
    }
    removed
}

pub fn start_background_indexer(app: tauri::AppHandle) {
    let cfg = config::get_config();
    if !cfg.vector_search.enabled {
        return;
    }

    tauri::async_runtime::spawn(async move {
        loop {
            let stats = index_incremental(&app).await;
            eprintln!(
                "[indexer] indexed={}, skipped={}, removed={}, errors={}",
                stats.indexed, stats.skipped, stats.removed, stats.errors
            );
            let interval = config::get_config().indexer.interval_hours;
            tokio::time::sleep(std::time::Duration::from_secs(interval * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn default_exts() -> Vec<String> {
        DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn indexable_txt_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.md");
        fs::write(&file, "hello").unwrap();
        assert!(is_indexable_file(&file, 1_000_000, &default_exts()));
    }

    #[test]
    fn not_indexable_binary() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("image.png");
        fs::write(&file, "fake").unwrap();
        assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
    }

    #[test]
    fn not_indexable_no_extension() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("Makefile");
        fs::write(&file, "all:").unwrap();
        assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
    }

    #[test]
    fn not_indexable_too_large() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("big.txt");
        fs::write(&file, "x".repeat(100)).unwrap();
        assert!(!is_indexable_file(&file, 10, &default_exts())); // max 10 bytes
    }

    #[test]
    fn hidden_file_not_indexable() {
        let tmp = TempDir::new().unwrap();
        let hidden = tmp.path().join(".hidden.rs");
        fs::write(&hidden, "fn main(){}").unwrap();
        assert!(!is_indexable_file(&hidden, 1_000_000, &default_exts()));
    }

    #[test]
    fn hidden_dir_file_with_normal_name_is_indexable() {
        // is_indexable_file only checks the filename, not parent dirs
        // WalkDir filtering handles hidden directories separately
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".git");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        fs::write(&file, "x").unwrap();
        assert!(is_indexable_file(&file, 1_000_000, &default_exts()));
    }

    #[test]
    fn all_supported_extensions() {
        let tmp = TempDir::new().unwrap();
        let exts = default_exts();
        for ext in DEFAULT_EXTENSIONS {
            let file = tmp.path().join(format!("test.{ext}"));
            fs::write(&file, "content").unwrap();
            assert!(
                is_indexable_file(&file, 1_000_000, &exts),
                "Extension .{ext} should be indexable"
            );
        }
    }

    #[test]
    fn custom_extensions_filter() {
        let tmp = TempDir::new().unwrap();
        let custom = vec!["xyz".to_string()];
        let file_yes = tmp.path().join("test.xyz");
        let file_no = tmp.path().join("test.txt");
        fs::write(&file_yes, "y").unwrap();
        fs::write(&file_no, "n").unwrap();
        assert!(is_indexable_file(&file_yes, 1_000_000, &custom));
        assert!(!is_indexable_file(&file_no, 1_000_000, &custom));
    }

    #[test]
    fn expand_tilde_works() {
        let expanded = expand_tilde("~/Documents");
        assert!(!expanded.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn expand_tilde_no_tilde() {
        let expanded = expand_tilde("/tmp/foo");
        assert_eq!(expanded, PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn cleanup_stale_removes_missing_files() {
        use rusqlite::Connection;
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE vectors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                content_preview TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dimension INTEGER NOT NULL,
                model TEXT NOT NULL,
                indexed_at REAL NOT NULL,
                file_mtime REAL NOT NULL
            )",
        )
        .unwrap();

        // Insert a path that doesn't exist
        conn.execute(
            "INSERT INTO vectors (file_path, content_preview, embedding, dimension, model, indexed_at, file_mtime)
             VALUES ('/nonexistent/file.txt', 'x', X'00', 1, 'm', 0.0, 0.0)",
            [],
        )
        .unwrap();

        let state = VectorDbState(std::sync::Mutex::new(conn));
        let removed = cleanup_stale(&state);
        assert_eq!(removed, 1);

        let conn = state.0.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn cleanup_stale_keeps_existing_files() {
        use rusqlite::Connection;
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("exists.txt");
        fs::write(&file, "hello").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE vectors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                content_preview TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dimension INTEGER NOT NULL,
                model TEXT NOT NULL,
                indexed_at REAL NOT NULL,
                file_mtime REAL NOT NULL
            )",
        )
        .unwrap();

        let path_str = file.to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO vectors (file_path, content_preview, embedding, dimension, model, indexed_at, file_mtime)
             VALUES (?1, 'x', X'00', 1, 'm', 0.0, 0.0)",
            [&path_str],
        )
        .unwrap();

        let state = VectorDbState(std::sync::Mutex::new(conn));
        let removed = cleanup_stale(&state);
        assert_eq!(removed, 0);
    }
}
