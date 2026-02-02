pub mod apps;
pub mod chat;
pub mod files;
pub mod health;
pub mod history;
pub mod math;
pub mod onepass;
pub mod onepass_vault;
pub mod settings;
pub mod special;
pub mod ssh;
pub mod vectors;

use std::path::PathBuf;

/// Resolve the application data directory, respecting the `BURROW_DATA_DIR` environment variable override.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BURROW_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            eprintln!("[warning] $HOME/$XDG_DATA_HOME not set, using current directory for data");
            PathBuf::from(".")
        })
        .join("burrow")
}
