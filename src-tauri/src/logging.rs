//! Logging infrastructure for Burrow.
//!
//! Logs are written to `/tmp/burrow/` and cleared on app restart.
//! Set `BURROW_LOG=trace` for extremely verbose output (debugging).
//! Set `BURROW_LOG=debug` for detailed output.
//! Default level is `info`.

use std::path::PathBuf;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const LOG_DIR: &str = "/tmp/burrow";
const MAX_LOG_FILES: usize = 5;

/// Initialize the logging system.
///
/// - Clears old logs on startup
/// - Writes to `/tmp/burrow/burrow.YYYY-MM-DD.log`
/// - Uses `BURROW_LOG` env var for log level (default: info)
/// - Outputs to both file and stderr
pub fn init_logging() {
    let log_dir = PathBuf::from(LOG_DIR);

    // Clear old logs on startup
    if log_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&log_dir) {
            eprintln!("[logging] failed to clear old logs: {e}");
        }
    }
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("[logging] failed to create log directory: {e}");
        // Continue anyway - we'll still have stderr output
    }

    // Rolling file appender - rotates daily, keeps last 5 files (~10MB total)
    let file_appender = match RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("burrow")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(&log_dir)
    {
        Ok(appender) => Some(appender),
        Err(e) => {
            eprintln!("[logging] failed to create file appender: {e}");
            None
        }
    };

    // Environment filter: BURROW_LOG=trace for verbose, default=info
    let filter = EnvFilter::try_from_env("BURROW_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,reqwest=warn"));

    // Build the subscriber with both file and stderr output
    let subscriber = tracing_subscriber::registry().with(filter);

    if let Some(appender) = file_appender {
        let (non_blocking, _guard) = tracing_appender::non_blocking(appender);
        // Leak the guard to keep the writer alive for the program's lifetime
        std::mem::forget(_guard);

        subscriber
            .with(
                fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(true)
                    .with_line_number(true),
            )
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(true)
                    .with_target(true),
            )
            .init();
    } else {
        // File appender failed, just use stderr
        subscriber
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(true)
                    .with_target(true),
            )
            .init();
    }

    tracing::info!("Burrow logging initialized (log dir: {LOG_DIR})");
}

/// Get the path to the log directory.
pub fn log_dir() -> PathBuf {
    PathBuf::from(LOG_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_is_tmp() {
        assert_eq!(log_dir(), PathBuf::from("/tmp/burrow"));
    }
}
