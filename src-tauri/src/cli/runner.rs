use super::output::{
    print_error, print_heading, print_info, print_kv, print_status, print_success, print_warning,
};
use super::progress::IndexProgress;
use super::{Commands, DaemonAction};
use crate::commands::{health, history, vectors};
use crate::config;
use crate::daemon;
use crate::indexer;
use crate::ollama;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

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
        Commands::Daemon { action } => cmd_daemon(action),
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
    let cfg = config::get_config();

    if !cfg.vector_search.enabled {
        print_error("Vector search is disabled in config");
        return 1;
    }

    // Try daemon first for real-time indexer progress
    if daemon::is_daemon_running().is_some() {
        let rt = match create_runtime() {
            Ok(rt) => rt,
            Err(code) => return code,
        };

        let result = rt.block_on(async {
            let client = daemon::DaemonClient::new();
            client.progress().await
        });

        if let Ok(progress) = result {
            if progress.running {
                print_heading("Indexer Progress (Live)");
                print_kv("Phase", &progress.phase);
                print_kv("Current file", &progress.current_file);
                let pct = if progress.total > 0 {
                    (progress.processed as f64 / progress.total as f64 * 100.0).round() as u32
                } else {
                    0
                };
                print_kv(
                    "Progress",
                    &format!("{}/{} ({}%)", progress.processed, progress.total, pct),
                );
                print_kv("Errors", &progress.errors.to_string());
                return 0;
            } else if !progress.last_result.is_empty() {
                // Show last result but fall through to show static stats too
                print_info(&format!("Last run: {}", progress.last_result));
                println!();
            }
        }
    }

    // Fall back to static index stats
    let indexable_paths = indexer::collect_indexable_paths(cfg);
    let total_files = indexable_paths.len();

    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    let indexed_count: i64 = match conn.query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
    {
        Ok(count) => count,
        Err(e) => {
            print_error(&format!("Failed to query indexed count: {e}"));
            return 1;
        }
    };

    let last_indexed: Option<f64> = conn
        .query_row("SELECT MAX(indexed_at) FROM vectors", [], |r| r.get(0))
        .ok();

    print_heading("Index Status");
    print_kv("Indexed", &format!("{indexed_count} files"));
    print_kv(
        "Indexable",
        &format!("{total_files} files in configured dirs"),
    );

    let coverage = if total_files > 0 {
        (indexed_count as f64 / total_files as f64 * 100.0).round() as u32
    } else {
        0
    };
    print_kv("Coverage", &format!("{coverage}%"));

    if last_indexed.is_some() {
        print_kv("Last indexed", "available");
    } else {
        print_kv("Last indexed", "never");
    }

    if indexed_count == 0 {
        println!();
        print_info("Run 'burrow reindex' to index all files");
    } else if (indexed_count as usize) < total_files {
        println!();
        print_info("Run 'burrow update' to index new/modified files");
    }

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

    // Try to delegate to daemon if running
    if daemon::is_daemon_running().is_some() {
        return delegate_to_daemon(true, quiet);
    }

    // Standalone mode - open vector DB and clear it
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

    // Try to delegate to daemon if running
    if daemon::is_daemon_running().is_some() {
        return delegate_to_daemon(false, quiet);
    }

    // Standalone mode
    let conn = match vectors::open_vector_db() {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Failed to open vector DB: {e}"));
            return 1;
        }
    };

    run_indexer(&conn, cfg, quiet, true)
}

/// Delegate indexing to the daemon and optionally show progress.
fn delegate_to_daemon(full: bool, quiet: bool) -> i32 {
    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };

    // Start the indexer on the daemon
    let result = rt.block_on(async {
        let client = daemon::DaemonClient::new();
        client.start_indexer(full).await
    });

    match result {
        Ok(resp) => {
            if !resp.started {
                print_info(&resp.message);
                return 0;
            }

            if !quiet {
                print_success(&resp.message);
            }

            // If not quiet, poll for progress until done
            if !quiet {
                show_daemon_progress(&rt);
            }

            0
        }
        Err(e) => {
            print_error(&format!("Failed to start indexer via daemon: {e}"));
            1
        }
    }
}

/// Poll the daemon for indexer progress and display it.
fn show_daemon_progress(rt: &tokio::runtime::Runtime) {
    use std::io::Write;

    let client = daemon::DaemonClient::new();
    let mut last_processed = 0u32;

    while let Ok(progress) = rt.block_on(client.progress()) {
        if !progress.running {
            if !progress.last_result.is_empty() {
                println!();
                print_success(&progress.last_result);
            }
            break;
        }

        // Only print on change to avoid spamming
        if progress.processed != last_processed {
            let pct = if progress.total > 0 {
                (progress.processed as f64 / progress.total as f64 * 100.0).round() as u32
            } else {
                0
            };
            print!(
                "\r{}: {}/{} ({}%) {}",
                progress.phase,
                progress.processed,
                progress.total,
                pct,
                if progress.errors > 0 {
                    format!("[{} errors]", progress.errors)
                } else {
                    String::new()
                }
            );
            let _ = std::io::stdout().flush();
            last_processed = progress.processed;
        }

        std::thread::sleep(std::time::Duration::from_millis(200));
    }
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

fn cmd_daemon(action: Option<DaemonAction>) -> i32 {
    let action = action.unwrap_or(DaemonAction::Start { background: false });

    match action {
        DaemonAction::Start { background } => cmd_daemon_start(background),
        DaemonAction::Stop => cmd_daemon_stop(),
        DaemonAction::Status => cmd_daemon_status(),
    }
}

fn cmd_daemon_start(background: bool) -> i32 {
    // Check if already running
    if let Some(pid) = daemon::is_daemon_running() {
        print_error(&format!("Daemon already running (PID {pid})"));
        return 1;
    }

    if background {
        // Fork into background
        return daemonize();
    }

    // Foreground mode - run the daemon directly
    run_daemon_foreground()
}

fn daemonize() -> i32 {
    use std::process::Command;

    // Get the current executable path
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            print_error(&format!("Failed to get executable path: {e}"));
            return 1;
        }
    };

    // Spawn a child process with the daemon command (without --background)
    // Using nohup-style approach: redirect stdin/stdout/stderr to /dev/null
    let child = Command::new(&exe)
        .args(["daemon", "start"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match child {
        Ok(_) => {
            print_success("Daemon started in background");
            // Give it a moment to start and write PID file
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Verify it started
            if let Some(pid) = daemon::is_daemon_running() {
                print_kv("PID", &pid.to_string());
                print_kv("Socket", &daemon::socket_path().display().to_string());
                0
            } else {
                print_warning("Daemon may not have started successfully");
                1
            }
        }
        Err(e) => {
            print_error(&format!("Failed to start daemon: {e}"));
            1
        }
    }
}

fn run_daemon_foreground() -> i32 {
    // Write PID file
    if let Err(e) = daemon::write_pid_file() {
        print_error(&format!("Failed to write PID file: {e}"));
        return 1;
    }

    print_heading("Daemon Starting");
    print_kv("PID", &std::process::id().to_string());
    print_kv("Socket", &daemon::socket_path().display().to_string());

    // Set up signal handlers for graceful shutdown
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();

    ctrlc::set_handler(move || {
        tracing::info!("received shutdown signal");
        running_clone.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .ok();

    // Create runtime and run the daemon
    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => {
            let _ = daemon::remove_pid_file();
            return code;
        }
    };

    let result = rt.block_on(async {
        let state = Arc::new(daemon::handlers::DaemonState::new());
        let router = daemon::handlers::create_router(state);

        print_success("Daemon ready");

        daemon::start_server(router).await
    });

    // Cleanup
    let _ = daemon::remove_pid_file();
    if let Err(e) = std::fs::remove_file(daemon::socket_path()) {
        tracing::debug!(error = %e, "failed to remove socket file");
    }

    match result {
        Ok(()) => {
            print_info("Daemon stopped");
            0
        }
        Err(e) => {
            print_error(&format!("Daemon error: {e}"));
            1
        }
    }
}

fn cmd_daemon_stop() -> i32 {
    let rt = match create_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };

    // Try to connect to daemon and send shutdown
    let result = rt.block_on(async {
        let client = daemon::DaemonClient::new();
        client.shutdown().await
    });

    match result {
        Ok(()) => {
            print_success("Shutdown signal sent");
            // Wait a bit for the daemon to actually stop
            std::thread::sleep(std::time::Duration::from_millis(500));
            if daemon::is_daemon_running().is_none() {
                print_info("Daemon stopped");
            }
            0
        }
        Err(e) => {
            // Maybe daemon isn't running?
            if daemon::is_daemon_running().is_none() {
                print_info("Daemon is not running");
                0
            } else {
                print_error(&format!("Failed to stop daemon: {e}"));
                1
            }
        }
    }
}

fn cmd_daemon_status() -> i32 {
    match daemon::is_daemon_running() {
        Some(pid) => {
            let rt = match create_runtime() {
                Ok(rt) => rt,
                Err(code) => return code,
            };

            // Try to get detailed status from daemon
            let result = rt.block_on(async {
                let client = daemon::DaemonClient::new();
                client.status().await
            });

            print_heading("Daemon Status");
            print_status("Running", true);
            print_kv("PID", &pid.to_string());
            print_kv("Socket", &daemon::socket_path().display().to_string());

            if let Ok(status) = result {
                print_kv("Version", &status.version);
                print_kv("Uptime", &format_uptime(status.uptime_secs));
            }

            0
        }
        None => {
            print_heading("Daemon Status");
            print_status("Running", false);
            print_info("Start with: burrow daemon");
            1
        }
    }
}

fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
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

    #[test]
    fn format_uptime_seconds_only() {
        assert_eq!(format_uptime(45), "45s");
    }

    #[test]
    fn format_uptime_minutes() {
        assert_eq!(format_uptime(125), "2m 5s");
    }

    #[test]
    fn format_uptime_hours() {
        assert_eq!(format_uptime(3725), "1h 2m 5s");
    }

    #[test]
    fn format_uptime_zero() {
        assert_eq!(format_uptime(0), "0s");
    }
}
