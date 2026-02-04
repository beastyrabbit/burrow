use super::output::{
    print_error, print_heading, print_info, print_kv, print_status, print_success, print_warning,
};
use super::progress::IndexProgress;
use super::Commands;
use crate::commands::{health, history, vectors};
use crate::config;
use crate::indexer;
use crate::ollama;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Threshold for mtime comparison (1 second) to account for filesystem precision.
const MTIME_EPSILON: f64 = 1.0;

/// Run a CLI command and return the exit code
pub fn run_command(cmd: Commands) -> i32 {
    match cmd {
        Commands::Health { json } => cmd_health(json),
        Commands::Stats { json } => cmd_stats(json),
        Commands::Config { path } => cmd_config(path),
        Commands::Progress => cmd_progress(),
        Commands::Index { file, force } => cmd_index(&file, force),
        Commands::Reindex { quiet } => cmd_reindex(quiet),
        Commands::Update { quiet } => cmd_update(quiet),
    }
}

fn create_runtime() -> Result<tokio::runtime::Runtime, i32> {
    tokio::runtime::Runtime::new().map_err(|e| {
        print_error(&format!("Failed to initialize async runtime: {e}"));
        1
    })
}

fn cmd_health(json: bool) -> i32 {
    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let status = match rt.block_on(health::health_check_standalone()) {
        Ok(s) => s,
        Err(e) => {
            print_error(&format!("Health check failed: {e}"));
            return 1;
        }
    };

    if json {
        if let Err(e) = super::output::print_json_compact(&status) {
            print_error(&format!("JSON serialization failed: {e}"));
            return 1;
        }
    } else {
        print_heading("System Health");
        print_status("Ollama", status.ollama);
        print_status("Vector DB", status.vector_db);
        print_status("API Key", status.api_key);

        if !status.issues.is_empty() {
            println!();
            print_heading("Issues");
            for issue in &status.issues {
                print_info(issue);
            }
        }
    }

    // Exit 0 only if core services (Ollama, Vector DB) are healthy.
    // API key is optional - chat features degrade gracefully without it.
    if status.ollama && status.vector_db {
        0
    } else {
        1
    }
}

#[derive(Serialize)]
struct StatsOutput {
    indexed_files: i64,
    launch_count: i64,
    last_indexed: Option<String>,
}

fn cmd_stats(json: bool) -> i32 {
    let vconn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    let file_count: i64 = match vconn.query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0)) {
        Ok(count) => count,
        Err(e) => {
            tracing::error!(error = %e, "failed to query vector count");
            print_error(&format!("Failed to query indexed file count: {e}"));
            return 1;
        }
    };

    let last_indexed: Option<f64> = vconn
        .query_row("SELECT MAX(indexed_at) FROM vectors", [], |r| r.get(0))
        .ok();

    let hconn = match history::open_history_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open history DB: {e}"));
            return 1;
        }
    };

    let launch_count: i64 = match hconn.query_row("SELECT COUNT(*) FROM launches", [], |r| r.get(0))
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!(error = %e, "failed to query launch count");
            print_error(&format!("Failed to query launch history: {e}"));
            return 1;
        }
    };

    let last_str = if last_indexed.is_some() {
        "available"
    } else {
        "never"
    };

    if json {
        let output = StatsOutput {
            indexed_files: file_count,
            launch_count,
            last_indexed: last_indexed.map(|_| last_str.to_string()),
        };
        if let Err(e) = super::output::print_json_compact(&output) {
            print_error(&format!("JSON serialization failed: {e}"));
            return 1;
        }
    } else {
        print_heading("Statistics");
        print_kv("Indexed files", &file_count.to_string());
        print_kv("Launch history", &format!("{launch_count} entries"));
        print_kv("Last indexed", last_str);
    }

    0
}

fn cmd_config(path_only: bool) -> i32 {
    let path = config::config_path();

    if path_only {
        println!("{}", path.display());
        return 0;
    }

    // Open config in $EDITOR, falling back to xdg-open (system default handler).
    // Note: Complex $EDITOR values with quoted arguments may not parse correctly.
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "xdg-open".into());
    let mut parts = editor.split_whitespace();
    let cmd = parts.next().unwrap_or("xdg-open");
    let args: Vec<&str> = parts.collect();

    match std::process::Command::new(cmd)
        .args(&args)
        .arg(&path)
        .spawn()
    {
        Ok(_) => {
            print_success(&format!("Opened {}", path.display()));
            0
        }
        Err(e) => {
            print_error(&format!("Failed to open config: {e}"));
            1
        }
    }
}

fn cmd_progress() -> i32 {
    // In CLI mode, we can't read the Tauri state, so we just report
    // that CLI doesn't have access to the GUI's indexer state.
    // Returns 0 because this is not an error condition - the command succeeds
    // at reporting that progress information is unavailable in CLI mode.
    print_info("Indexer status is only available in GUI mode");
    print_info("Use 'burrow stats' to see indexed file count");
    0
}

fn cmd_index(file: &Path, force: bool) -> i32 {
    let cfg = config::get_config();

    // Validate file exists
    if !file.exists() {
        print_error(&format!("File not found: {}", file.display()));
        return 1;
    }

    if !file.is_file() {
        print_error(&format!("Not a file: {}", file.display()));
        return 1;
    }

    // Check if file is indexable
    if !indexer::is_indexable_file(
        file,
        cfg.vector_search.max_file_size_bytes,
        &cfg.indexer.file_extensions,
    ) {
        print_error(&format!(
            "File type not supported or too large: {}",
            file.display()
        ));
        return 1;
    }

    // Open vector DB
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    let path_str = file.to_string_lossy().to_string();

    // Check if already indexed and unchanged (unless --force)
    if !force {
        let existing_mtime: Option<f64> = conn
            .query_row(
                "SELECT file_mtime FROM vectors WHERE file_path = ?1",
                [&path_str],
                |r| r.get(0),
            )
            .ok();

        if let Some(db_mtime) = existing_mtime {
            let current_mtime = indexer::file_mtime(file);
            if !is_file_modified(current_mtime, db_mtime) {
                print_info(&format!(
                    "File unchanged, use --force to re-index: {}",
                    file.display()
                ));
                return 0;
            }
        }
    }

    // Index the file
    let progress = IndexProgress::spinner(&format!("Indexing {}...", file.display()));

    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let result = rt.block_on(index_single_file_standalone(
        file,
        &conn,
        cfg.indexer.max_content_chars,
    ));

    match result {
        Ok(()) => {
            progress.finish_success(&format!("Indexed {}", file.display()));
            0
        }
        Err(e) => {
            progress.finish_error(&format!("Failed: {e}"));
            1
        }
    }
}

fn cmd_reindex(quiet: bool) -> i32 {
    let cfg = config::get_config();

    if !cfg.vector_search.enabled {
        print_error("Vector search is disabled in config");
        return 1;
    }

    // Open vector DB and clear it
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    if let Err(e) = conn.execute("DELETE FROM vectors", []) {
        print_error(&format!("Failed to clear vectors: {e}"));
        return 1;
    }

    run_indexer(&conn, cfg, quiet, false)
}

fn cmd_update(quiet: bool) -> i32 {
    let cfg = config::get_config();

    if !cfg.vector_search.enabled {
        print_error("Vector search is disabled in config");
        return 1;
    }

    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    run_indexer(&conn, cfg, quiet, true)
}

/// Check if a file has been modified based on mtime comparison.
fn is_file_modified(current_mtime: f64, db_mtime: f64) -> bool {
    (current_mtime - db_mtime).abs() >= MTIME_EPSILON
}

fn run_indexer(
    conn: &rusqlite::Connection,
    cfg: &config::AppConfig,
    quiet: bool,
    incremental: bool,
) -> i32 {
    // Collect existing mtimes if incremental
    let existing: HashMap<String, f64> = if incremental {
        let mut stmt = match conn.prepare("SELECT file_path, file_mtime FROM vectors") {
            Ok(s) => s,
            Err(e) => {
                print_error(&format!("Failed to query mtimes: {e}"));
                return 1;
            }
        };
        let result: HashMap<String, f64> = match stmt.query_map([], |row| {
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
                tracing::error!(error = %e, "failed to query existing mtimes, falling back to full reindex");
                print_warning(&format!(
                    "Could not read existing index state, performing full reindex: {e}"
                ));
                HashMap::new()
            }
        };
        result
    } else {
        HashMap::new()
    };

    // Collect files to index using shared function from indexer module
    let paths = indexer::collect_indexable_paths(cfg);
    let paths_to_index: Vec<_> = if incremental {
        paths
            .iter()
            .filter(|path| {
                let path_str = path.to_string_lossy().to_string();
                let mtime = indexer::file_mtime(path);
                match existing.get(&path_str) {
                    Some(&db_mtime) => is_file_modified(mtime, db_mtime),
                    None => true,
                }
            })
            .collect()
    } else {
        paths.iter().collect()
    };

    let total = paths_to_index.len();
    let skipped = paths.len() - total;

    if total == 0 {
        if !quiet {
            print_success(if incremental {
                "All files up to date"
            } else {
                "No files to index"
            });
        }
        return 0;
    }

    let progress = if quiet {
        None
    } else {
        Some(IndexProgress::new(total as u64))
    };

    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let mut indexed = 0u32;
    let mut errors = 0u32;
    let mut error_messages: Vec<String> = Vec::new();

    for path in paths_to_index {
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(ref p) = progress {
            p.set_current(&filename);
        }

        let result = rt.block_on(index_single_file_standalone(
            path,
            conn,
            cfg.indexer.max_content_chars,
        ));

        match result {
            Ok(()) => indexed += 1,
            Err(e) => {
                errors += 1;
                error_messages.push(format!("{}: {e}", path.display()));
            }
        }

        if let Some(ref p) = progress {
            p.inc();
        }
    }

    // Remove DB entries for files that no longer exist or are outside indexed directories
    // (incremental mode only)
    let removed = if incremental {
        let valid_paths: HashSet<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        cleanup_stale(conn, &valid_paths)
    } else {
        0
    };

    if let Some(p) = progress {
        p.finish_clear();

        // Print errors
        for msg in &error_messages {
            print_error(msg);
        }

        // Print summary
        let summary = if incremental {
            format!("Indexed {indexed}, skipped {skipped}, removed {removed}, {errors} errors")
        } else {
            format!("Indexed {indexed} files, {errors} errors")
        };

        if errors > 0 {
            print_info(&summary);
        } else {
            print_success(&summary);
        }
    }

    i32::from(errors > 0)
}

/// Index a single file and store its embedding in the database.
/// Uses the embedding model from global config for generation.
async fn index_single_file_standalone(
    path: &Path,
    conn: &rusqlite::Connection,
    max_content_chars: usize,
) -> Result<(), String> {
    let cfg = config::get_config();
    let content = crate::text_extract::extract_text(path, max_content_chars)?;
    let embedding = ollama::generate_embedding(&content).await?;
    let preview: String = content.chars().take(200).collect();
    let mtime = indexer::file_mtime(path);
    let path_str = path.to_string_lossy().to_string();

    vectors::insert_vector(
        conn,
        &path_str,
        &preview,
        &embedding,
        &cfg.ollama.embedding_model,
        mtime,
    )
    .map_err(|e| e.to_string())
}

fn cleanup_stale(conn: &rusqlite::Connection, valid_paths: &HashSet<String>) -> u32 {
    let mut stmt = match conn.prepare("SELECT file_path FROM vectors") {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to prepare stale cleanup query");
            return 0;
        }
    };
    let paths: Vec<String> = match stmt.query_map([], |row| row.get(0)) {
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
            tracing::warn!(error = %e, "failed to query stale paths");
            return 0;
        }
    };

    let mut removed = 0u32;
    for path in paths {
        if !Path::new(&path).exists() || !valid_paths.contains(&path) {
            if let Err(e) = conn.execute("DELETE FROM vectors WHERE file_path = ?1", [&path]) {
                tracing::warn!(path, error = %e, "failed to delete stale vector entry");
            } else {
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_file_modified_detects_change() {
        assert!(is_file_modified(100.0, 99.0));
        assert!(is_file_modified(99.0, 100.0));
    }

    #[test]
    fn is_file_modified_same_time() {
        assert!(!is_file_modified(100.0, 100.0));
        assert!(!is_file_modified(100.0, 100.5)); // Within epsilon
    }

    #[test]
    fn cmd_config_path_only_returns_zero() {
        // This just tests the exit code, not the actual output
        let exit_code = cmd_config(true);
        assert_eq!(exit_code, 0);
    }
}
