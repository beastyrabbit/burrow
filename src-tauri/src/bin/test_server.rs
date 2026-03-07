//! Standalone test server — runs the axum HTTP bridge without Tauri runtime.
//! No GUI window, no event loop, no single-instance plugin.
//!
//! Usage:
//!   BURROW_DRY_RUN=1 cargo run --bin test-server
//!
//! This is used by Playwright e2e tests as the backend, replacing `pnpm tauri dev`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn write_fixture_desktop_entry(
    dir: &Path,
    id: &str,
    name: &str,
    exec: &str,
) -> std::io::Result<()> {
    fs::write(
        dir.join(format!("{id}.desktop")),
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nIcon=\nComment=Playwright fixture\n"
        ),
    )
}

fn prepare_e2e_application_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(app_dir) = std::env::var("BURROW_E2E_APP_DIR") else {
        return Ok(());
    };

    let app_dir = PathBuf::from(app_dir);
    fs::create_dir_all(&app_dir)?;

    for entry in fs::read_dir(&app_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("desktop") {
            fs::remove_file(path)?;
        }
    }

    if let Ok(shared_roots) = std::env::var("XDG_DATA_DIRS") {
        if let Some(shared_root) = shared_roots.split(':').next() {
            fs::create_dir_all(PathBuf::from(shared_root).join("applications"))?;
        }
    }

    for i in 1..=12 {
        write_fixture_desktop_entry(
            &app_dir,
            &format!("fixture-{i}"),
            &format!("Fixture App {i}"),
            &format!("fixture-app-{i}"),
        )?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // BURROW_DRY_RUN is set by playwright.config.ts env — no need to set here.

    burrow_lib::logging::init_logging();
    burrow_lib::config::init_config();
    prepare_e2e_application_fixtures()?;

    let ctx = Arc::new(burrow_lib::context::AppContext::from_disk()?);
    if let Err(error) = ctx.start_app_watcher() {
        tracing::warn!(error = %error, "application watcher unavailable in test-server mode; continuing without auto-refresh");
    }

    let router = burrow_lib::dev_server::build_router(ctx);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    eprintln!("test-server listening on http://127.0.0.1:3001");
    axum::serve(listener, router).await?;
    Ok(())
}
