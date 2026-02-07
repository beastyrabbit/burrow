pub mod actions;
pub(crate) mod chat;
pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod daemon;
pub mod dev_server;
pub mod icons;
pub mod indexer;
pub mod logging;
pub mod ollama;
pub mod output_buffers;
pub mod process_timeout;
pub mod router;
pub(crate) mod text_extract;
pub mod window_manager;

use commands::{apps, history, vectors};
use context::AppContext;
use std::sync::Arc;
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
            apps::init_app_cache();

            // Create shared state instances once — used by both Tauri managed state and AppContext.
            let db = Arc::new(history::DbState::new(history::open_history_db()?));
            let vector_db = Arc::new(vectors::VectorDbState::new(vectors::open_vector_db()?));
            let indexer_state = Arc::new(indexer::IndexerState::new());
            let output_buffers = Arc::new(output_buffers::OutputBufferState::new());

            // Manage individual states for Tauri commands and background indexer
            app.manage(db.clone());
            app.manage(vector_db.clone());
            app.manage(indexer_state.clone());
            app.manage(output_buffers.clone());
            indexer::start_background_indexer(app.handle().clone());

            // Build AppContext sharing the same Arc references — no duplicate connections
            let ctx = AppContext::from_arcs(db, vector_db, indexer_state, output_buffers)
                .with_app_handle(app.handle().clone());
            app.manage(ctx);

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
            router::search_cmd,
            history::record_launch_cmd,
            apps::launch_app,
            commands::chat::chat_ask_cmd,
            commands::health::health_check_cmd,
            actions::execute_action_cmd,
            output_buffers::get_output_cmd,
            hide_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
