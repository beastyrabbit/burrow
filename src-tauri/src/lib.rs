pub mod actions;
pub(crate) mod chat;
pub mod cli;
pub mod commands;
pub mod config;
pub mod daemon;
#[cfg(debug_assertions)]
pub mod dev_server;
pub mod icons;
pub mod indexer;
pub mod logging;
pub mod ollama;
pub mod router;
pub(crate) mod text_extract;

use commands::{apps, history, vectors};
use tauri::Manager;

#[tauri::command]
fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    match app.get_webview_window("main") {
        Some(win) => win.hide().map_err(|e| e.to_string()),
        None => Err("main window not found".into()),
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
            // In headless mode, ignore single-instance window toggles
            if std::env::var("BURROW_HEADLESS").is_ok() {
                tracing::info!("headless mode: ignoring single-instance window toggle");
                return;
            }

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

            // Show window unless in headless mode (BURROW_HEADLESS env var, for testing)
            if std::env::var("BURROW_HEADLESS").is_err() {
                if let Some(win) = app.get_webview_window("main") {
                    if let Err(e) = win.show() {
                        tracing::warn!(error = %e, "failed to show main window on startup");
                    }
                    if let Err(e) = win.set_focus() {
                        tracing::warn!(error = %e, "failed to focus main window on startup");
                    }
                } else {
                    tracing::error!("main window not found during setup");
                }
            } else {
                tracing::info!("headless mode: window stays hidden");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            router::search,
            history::record_launch,
            apps::launch_app,
            commands::chat::chat_ask,
            commands::health::health_check,
            actions::execute_action,
            hide_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
