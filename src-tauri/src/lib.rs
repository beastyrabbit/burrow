mod commands;
mod config;
mod ollama;
mod router;

use commands::{apps, history, vectors};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config::init_config();

    tauri::Builder::default()
        .setup(|app| {
            history::init_db(app.handle())?;
            vectors::init_vector_db(app.handle())?;
            apps::init_app_cache();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            router::search,
            history::record_launch,
            apps::launch_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
