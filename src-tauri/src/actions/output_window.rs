use crate::output_buffers::OutputBufferState;
use crate::window_manager::Stream;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

/// Delay before removing buffer after command finishes, giving the frontend
/// time to poll the final snapshot.
const BUFFER_CLEANUP_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

/// Spawn a task that reads lines from an async reader and pushes them into the output buffer.
fn spawn_line_reader<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    stream: Stream,
    buffers: Arc<OutputBufferState>,
    label: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => buffers.push_line(&label, stream, line),
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(error = %e, ?stream, label = %label, "error reading output stream");
                    break;
                }
            }
        }
    })
}

/// Run a shell command in a new output window, streaming stdout/stderr into a shared buffer
/// that the frontend polls via `get_output`.
///
/// 1. Spawns a new Tauri window via `window_manager::spawn_output_window`.
/// 2. Creates a buffer in `OutputBufferState` for this window label.
/// 3. Spawns the command as a child process with piped stdout/stderr.
/// 4. Reads stdout/stderr line-by-line, pushing into the buffer.
/// 5. On process exit, marks the buffer as done with the exit code.
/// 6. If the window is closed before the process finishes, the child is killed via its handle.
pub async fn run_in_output_window(
    command: String,
    name: String,
    app: &tauri::AppHandle,
    buffers: Arc<OutputBufferState>,
) -> Result<(), String> {
    let label = crate::window_manager::spawn_output_window(app, &name)?;

    // Create the buffer before spawning the command so the frontend can start polling immediately.
    buffers.create(label.clone());

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn command: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture stderr".to_string())?;

    // Use a oneshot channel so the window close handler can request termination
    // without raw PID signaling (avoids PID reuse issues).
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

    // Register close handler immediately after spawn so there's no window where
    // the user could close the window and leave the child running orphaned.
    register_window_close_handler(app, &label, kill_tx);

    let stdout_task = spawn_line_reader(stdout, Stream::Stdout, buffers.clone(), label.clone());
    let stderr_task = spawn_line_reader(stderr, Stream::Stderr, buffers.clone(), label.clone());

    // Drive readers, process exit, and kill signal concurrently.
    // If kill_rx fires first, we kill the child and still await exit.
    let wait_task = async {
        tokio::select! {
            status = child.wait() => status,
            _ = kill_rx => {
                tracing::info!("output window closed, killing child process");
                if let Err(e) = child.kill().await {
                    tracing::warn!(error = %e, "failed to kill child (may have already exited)");
                }
                child.wait().await
            }
        }
    };

    let (stdout_result, stderr_result, wait_result) =
        tokio::join!(stdout_task, stderr_task, wait_task);

    if let Err(e) = stdout_result {
        tracing::error!(error = %e, "stdout reader task panicked");
    }
    if let Err(e) = stderr_result {
        tracing::error!(error = %e, "stderr reader task panicked");
    }

    let exit_code = match wait_result {
        Ok(status) => status.code(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to wait for child process");
            None
        }
    };

    buffers.set_done(&label, exit_code);
    tracing::info!(label = %label, ?exit_code, "output window command finished");

    // Schedule buffer removal after a delay so the frontend can poll the final snapshot.
    let cleanup_buffers = buffers.clone();
    let cleanup_label = label.clone();
    tokio::spawn(async move {
        tokio::time::sleep(BUFFER_CLEANUP_DELAY).await;
        cleanup_buffers.remove(&cleanup_label);
        tracing::debug!(label = %cleanup_label, "output buffer removed after cleanup delay");
    });

    Ok(())
}

/// Register a listener that signals the main task to kill the child process
/// when the window is closed. Uses a oneshot channel to avoid raw PID signaling.
fn register_window_close_handler(
    app: &tauri::AppHandle,
    label: &str,
    kill_tx: tokio::sync::oneshot::Sender<()>,
) {
    use tauri::Manager;

    let Some(window) = app.get_webview_window(label) else {
        tracing::warn!(label, "could not find window for close handler");
        return;
    };

    // Wrap in Option+Mutex so the sync callback can take it once.
    let kill_tx = std::sync::Mutex::new(Some(kill_tx));

    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            if let Some(tx) = kill_tx.lock().expect("kill_tx lock poisoned").take() {
                let _ = tx.send(());
            }
        }
    });
}
