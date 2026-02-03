pub mod actions;
pub(crate) mod chat;
pub mod commands;
pub mod config;
#[cfg(debug_assertions)]
pub mod dev_server;
pub mod icons;
pub mod indexer;
pub mod logging;
pub mod ollama;
pub mod router;
mod text_extract;

use commands::{apps, history, vectors};
use tauri::Manager;

#[tauri::command]
fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    match app.get_webview_window("main") {
        Some(win) => win.hide().map_err(|e| e.to_string()),
        None => Err("main window not found".into()),
    }
}

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
        "reindex" | "update" => {
            let progress_state = app.state::<indexer::IndexerState>();
            if progress_state.get().running {
                return Ok("Indexer is already running".into());
            }
            let handle = app.clone();
            let is_full = action == "reindex";
            tauri::async_runtime::spawn(async move {
                let stats = if is_full {
                    indexer::index_all(&handle).await
                } else {
                    indexer::index_incremental(&handle).await
                };
                tracing::info!(
                    action = if is_full { "reindex" } else { "update" },
                    indexed = stats.indexed,
                    skipped = stats.skipped,
                    removed = stats.removed,
                    errors = stats.errors,
                    "indexer run complete"
                );
            });
            Ok(if is_full {
                "Reindexing started in background..."
            } else {
                "Incremental update started in background..."
            }
            .into())
        }
        "config" => {
            let path = config::config_path();
            if actions::dry_run::is_enabled() {
                tracing::debug!(path = %path.display(), "[dry-run] open config");
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
            let vconn = vector_state.lock()?;
            let file_count: i64 = vconn
                .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            let last_indexed: Option<f64> = vconn
                .query_row("SELECT MAX(indexed_at) FROM vectors", [], |r| r.get(0))
                .ok();
            drop(vconn);

            let history_state = app.state::<history::DbState>();
            let hconn = history_state.lock()?;
            let launch_count: i64 = hconn
                .query_row("SELECT COUNT(*) FROM launches", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;

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
    logging::init_logging();
    config::init_config();

    tauri::Builder::default()
        // When a second instance is launched (e.g. `burrow toggle` from a keybinding),
        // toggle or focus the existing window instead of opening a duplicate.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let Some(win) = app.get_webview_window("main") else {
                tracing::warn!("main window not found, cannot toggle");
                return;
            };

            let should_hide = args.iter().any(|a| a == "toggle")
                && win.is_visible().unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to check window visibility");
                    false
                });

            if should_hide {
                if let Err(e) = win.hide() {
                    tracing::warn!(error = %e, "failed to hide window");
                }
            } else {
                if let Err(e) = win.show() {
                    tracing::warn!(error = %e, "failed to show window");
                }
                if let Err(e) = win.set_focus() {
                    tracing::warn!(error = %e, "failed to set focus");
                }
            }
        }))
        .setup(|app| {
            history::init_db(app.handle())?;
            vectors::init_vector_db(app.handle())?;
            apps::init_app_cache();
            // Vault is loaded on-demand via "Load 1Password Data" action
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
            hide_window,
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
