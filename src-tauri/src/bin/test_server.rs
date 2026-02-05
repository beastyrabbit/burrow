//! Standalone test server — runs the axum HTTP bridge without Tauri runtime.
//! No GUI window, no event loop, no single-instance plugin.
//!
//! Usage:
//!   BURROW_DRY_RUN=1 cargo run --bin test-server
//!
//! This is used by Playwright e2e tests as the backend, replacing `pnpm tauri dev`.

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // BURROW_DRY_RUN is set by playwright.config.ts env — no need to set here.

    burrow_lib::logging::init_logging();
    burrow_lib::config::init_config();
    burrow_lib::commands::apps::init_app_cache();

    let ctx = Arc::new(burrow_lib::context::AppContext::from_disk()?);

    let router = burrow_lib::dev_server::build_router(ctx);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    eprintln!("test-server listening on http://127.0.0.1:3001");
    axum::serve(listener, router).await?;
    Ok(())
}
