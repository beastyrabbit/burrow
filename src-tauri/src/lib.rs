pub mod actions;
pub(crate) mod chat;
pub mod commands;
pub mod config;
#[cfg(debug_assertions)]
pub mod dev_server;
pub mod icons;
pub mod indexer;
pub mod ollama;
pub mod router;
mod text_extract;

use commands::{apps, history, vectors};
use tauri::Manager;

fn format_config_action(path: &std::path::Path, dry_run: bool) -> String {
    if dry_run {
        format!("[dry-run] Would open {}", path.display())
    } else {
        format!("Opened {}", path.display())
    }
}

#[tauri::command]
async fn run_setting(action: String, app: tauri::AppHandle) -> Result<String, String> {
    match action.as_str() {
        "reindex" => {
            let progress_state = app.state::<indexer::IndexerState>();
            if progress_state.get().running {
                return Ok("Indexer is already running".into());
            }
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let stats = indexer::index_all(&handle).await;
                eprintln!(
                    "[reindex] done: indexed={}, errors={}",
                    stats.indexed, stats.errors
                );
            });
            Ok("Reindexing started in background...".into())
        }
        "update" => {
            let progress_state = app.state::<indexer::IndexerState>();
            if progress_state.get().running {
                return Ok("Indexer is already running".into());
            }
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let stats = indexer::index_incremental(&handle).await;
                eprintln!(
                    "[update] done: indexed={}, skipped={}, removed={}, errors={}",
                    stats.indexed, stats.skipped, stats.removed, stats.errors
                );
            });
            Ok("Incremental update started in background...".into())
        }
        "config" => {
            let path = config::config_path();
            if actions::dry_run::is_enabled() {
                eprintln!("[dry-run] open config: {}", path.display());
                return Ok(format_config_action(&path, true));
            }
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "xdg-open".into());
            let mut parts = editor.split_whitespace();
            let cmd = parts.next().unwrap_or("xdg-open");
            let args: Vec<&str> = parts.collect();
            std::process::Command::new(cmd)
                .args(&args)
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to open config: {e}"))?;
            Ok(format_config_action(&path, false))
        }
        "stats" => {
            let vector_state = app.state::<vectors::VectorDbState>();
            let vconn = vector_state.0.lock().map_err(|e: _| e.to_string())?;
            let file_count: i64 = vconn
                .query_row("SELECT COUNT(*) FROM vectors", [], |r: &rusqlite::Row| {
                    r.get(0)
                })
                .map_err(|e: rusqlite::Error| e.to_string())?;
            let last_indexed: Option<f64> = vconn
                .query_row(
                    "SELECT MAX(indexed_at) FROM vectors",
                    [],
                    |r: &rusqlite::Row| r.get(0),
                )
                .ok();
            drop(vconn);

            let history_state = app.state::<history::DbState>();
            let hconn = history_state.0.lock().map_err(|e: _| e.to_string())?;
            let launch_count: i64 = hconn
                .query_row("SELECT COUNT(*) FROM launches", [], |r: &rusqlite::Row| {
                    r.get(0)
                })
                .map_err(|e: rusqlite::Error| e.to_string())?;

            let last_str = last_indexed
                .map(|_| "available".to_string())
                .unwrap_or_else(|| "never".into());

            let progress_state = app.state::<indexer::IndexerState>();
            let p = progress_state.get();
            let status = if p.running {
                format!(" | Indexer: {} {}/{}", p.phase, p.processed, p.total)
            } else if !p.last_result.is_empty() {
                format!(" | Last run: {}", p.last_result)
            } else {
                String::new()
            };

            Ok(format!(
                "Content indexed: {} files | Apps tracked: {} launches | Last indexed: {}{}",
                file_count, launch_count, last_str, status
            ))
        }
        "progress" => {
            let progress_state = app.state::<indexer::IndexerState>();
            let p = progress_state.get();
            if p.running {
                let pct = if p.total > 0 {
                    (p.processed as f64 / p.total as f64 * 100.0) as u32
                } else {
                    0
                };
                Ok(format!(
                    "Indexing: {} ({}/{} — {}%) | Errors: {}",
                    p.current_file, p.processed, p.total, pct, p.errors
                ))
            } else if !p.last_result.is_empty() {
                Ok(format!("Idle | Last run: {}", p.last_result))
            } else {
                Ok("Idle | No indexing has run yet".into())
            }
        }
        "health" => {
            let status = commands::health::health_check(app)
                .await
                .map_err(|e| e.to_string())?;
            Ok(commands::health::format_health(&status))
        }
        _ => Err(format!("Unknown setting action: {action}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config::init_config();

    tauri::Builder::default()
        // When a second instance is launched (e.g. `burrow toggle` from a keybinding),
        // toggle or focus the existing window instead of opening a duplicate.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            match app.get_webview_window("main") {
                Some(win) => {
                    if args.iter().any(|a| a == "toggle") {
                        let visible = win.is_visible().unwrap_or_else(|e| {
                            eprintln!("[single-instance] failed to check visibility: {e}");
                            false
                        });
                        if visible {
                            if let Err(e) = win.hide() {
                                eprintln!("[single-instance] failed to hide window: {e}");
                            }
                        } else {
                            if let Err(e) = win.show() {
                                eprintln!("[single-instance] failed to show window: {e}");
                            }
                            if let Err(e) = win.set_focus() {
                                eprintln!("[single-instance] failed to set focus: {e}");
                            }
                        }
                    } else {
                        // Plain `burrow` invocation — bring existing instance to front
                        if let Err(e) = win.show() {
                            eprintln!("[single-instance] failed to show window: {e}");
                        }
                        if let Err(e) = win.set_focus() {
                            eprintln!("[single-instance] failed to set focus: {e}");
                        }
                    }
                }
                None => {
                    eprintln!("[single-instance] main window not found, cannot toggle");
                }
            }
        }))
        .setup(|app| {
            history::init_db(app.handle())?;
            vectors::init_vector_db(app.handle())?;
            apps::init_app_cache();
            app.manage(indexer::IndexerState::new());
            indexer::start_background_indexer(app.handle().clone());
            #[cfg(debug_assertions)]
            dev_server::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            router::search,
            history::record_launch,
            apps::launch_app,
            commands::chat::chat_ask,
            commands::health::health_check,
            run_setting,
            actions::execute_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_config_action_dry_run() {
        let path = std::path::Path::new("/tmp/config.toml");
        let msg = format_config_action(path, true);
        assert!(
            msg.contains("[dry-run]"),
            "expected dry-run marker, got: {msg}"
        );
        assert!(
            msg.contains("/tmp/config.toml"),
            "expected path in message, got: {msg}"
        );
    }

    #[test]
    fn format_config_action_normal() {
        let path = std::path::Path::new("/home/user/.config/burrow/config.toml");
        let msg = format_config_action(path, false);
        assert_eq!(msg, "Opened /home/user/.config/burrow/config.toml");
    }
}
