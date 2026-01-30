mod commands;
mod router;

use commands::{apps, history};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            history::init_db(app.handle())?;
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
