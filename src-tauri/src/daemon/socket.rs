use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::Notify;

/// Shutdown signal for graceful daemon termination.
pub static SHUTDOWN_SIGNAL: std::sync::OnceLock<Arc<Notify>> = std::sync::OnceLock::new();

/// Get or create the shutdown signal.
pub fn shutdown_signal() -> Arc<Notify> {
    SHUTDOWN_SIGNAL
        .get_or_init(|| Arc::new(Notify::new()))
        .clone()
}

/// Trigger a graceful shutdown of the daemon.
pub fn trigger_shutdown() {
    if let Some(signal) = SHUTDOWN_SIGNAL.get() {
        signal.notify_one();
    }
}

/// Get the XDG runtime directory for the daemon socket.
/// Falls back to ~/.local/share/burrow if XDG_RUNTIME_DIR is not set.
pub fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("burrow")
        })
        .join("burrow")
}

/// Get the path to the daemon socket file.
pub fn socket_path() -> PathBuf {
    runtime_dir().join("burrow.sock")
}

/// Get the path to the daemon PID file.
pub fn pid_path() -> PathBuf {
    runtime_dir().join("burrow.pid")
}

/// Start the Unix socket server.
///
/// Creates the socket file and listens for HTTP requests.
/// Returns an error if the socket is already in use.
pub async fn start_server(router: Router) -> Result<(), String> {
    let sock_path = socket_path();

    // Create the runtime directory if it doesn't exist
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create runtime dir: {e}"))?;
    }

    // Remove stale socket file if it exists
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)
            .map_err(|e| format!("failed to remove stale socket: {e}"))?;
    }

    let listener = UnixListener::bind(&sock_path)
        .map_err(|e| format!("failed to bind Unix socket at {}: {e}", sock_path.display()))?;

    tracing::info!(socket = %sock_path.display(), "daemon listening on Unix socket");

    // Initialize shutdown signal before serving
    let signal = shutdown_signal();

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            signal.notified().await;
            tracing::info!("shutdown signal received, stopping server");
        })
        .await
        .map_err(|e| format!("daemon server error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_dir_ends_with_burrow() {
        let dir = runtime_dir();
        assert!(
            dir.ends_with("burrow"),
            "expected dir to end with 'burrow', got: {}",
            dir.display()
        );
    }

    #[test]
    fn socket_path_ends_with_sock() {
        let path = socket_path();
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("sock"),
            "expected .sock extension, got: {}",
            path.display()
        );
    }

    #[test]
    fn pid_path_ends_with_pid() {
        let path = pid_path();
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("pid"),
            "expected .pid extension, got: {}",
            path.display()
        );
    }

    #[test]
    fn socket_path_is_under_runtime_dir() {
        let sock = socket_path();
        let runtime = runtime_dir();
        assert!(
            sock.starts_with(&runtime),
            "expected socket {} to be under runtime dir {}",
            sock.display(),
            runtime.display()
        );
    }

    #[test]
    fn respects_xdg_runtime_dir() {
        // Save original value
        let original = std::env::var("XDG_RUNTIME_DIR").ok();

        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let dir = runtime_dir();
        assert_eq!(dir, PathBuf::from("/run/user/1000/burrow"));

        // Restore original value
        match original {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }
}
