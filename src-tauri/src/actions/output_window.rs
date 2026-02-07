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
/// 6. If the window is closed before the process finishes, the child receives SIGTERM.
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

    let stdout_task = spawn_line_reader(stdout, Stream::Stdout, buffers.clone(), label.clone());
    let stderr_task = spawn_line_reader(stderr, Stream::Stderr, buffers.clone(), label.clone());

    // Shared flag so the close handler won't signal a PID after the process has exited
    // (preventing accidental signal to a reused PID).
    let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    register_window_close_handler(app, &label, child.id(), exited.clone());

    // Drive readers and process exit concurrently so a hung pipe doesn't block set_done.
    let (stdout_result, stderr_result, wait_result) =
        tokio::join!(stdout_task, stderr_task, child.wait());

    // Mark process as exited before any cleanup so the close handler won't signal.
    exited.store(true, std::sync::atomic::Ordering::Release);

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

/// Register a listener that sends SIGTERM to the child process when the window is closed.
/// The `exited` flag is set by the caller after `child.wait()` returns, preventing
/// SIGTERM delivery to a potentially reused PID.
#[cfg(unix)]
fn register_window_close_handler(
    app: &tauri::AppHandle,
    label: &str,
    child_pid: Option<u32>,
    exited: Arc<std::sync::atomic::AtomicBool>,
) {
    use tauri::Manager;

    let Some(pid) = child_pid else {
        tracing::warn!("no child PID available for close handler");
        return;
    };

    let Ok(pid_i32) = i32::try_from(pid) else {
        tracing::error!(
            pid,
            "child PID exceeds i32::MAX, cannot register close handler"
        );
        return;
    };

    let Some(window) = app.get_webview_window(label) else {
        tracing::warn!(label, "could not find window for close handler");
        return;
    };

    // Track whether we've already sent SIGTERM to avoid double-signaling.
    let signaled = Arc::new(std::sync::atomic::AtomicBool::new(false));

    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            // Don't signal if the process has already exited (PID may have been reused).
            if exited.load(std::sync::atomic::Ordering::Acquire) {
                tracing::debug!(pid, "process already exited, skipping SIGTERM");
                return;
            }
            if !signaled.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::info!(pid, "output window closed, sending SIGTERM to child");
                // SAFETY: pid_i32 is a valid i32 PID obtained from the child process.
                // We've verified via the `exited` flag that the process hasn't finished yet.
                let ret = unsafe { libc::kill(pid_i32, libc::SIGTERM) };
                if ret != 0 {
                    let errno = std::io::Error::last_os_error();
                    tracing::warn!(pid, error = %errno, "SIGTERM delivery failed (process may have already exited)");
                }
            }
        }
    });
}

#[cfg(not(unix))]
fn register_window_close_handler(
    _app: &tauri::AppHandle,
    _label: &str,
    _child_pid: Option<u32>,
    _exited: Arc<std::sync::atomic::AtomicBool>,
) {
    tracing::warn!("window close handler is only supported on unix");
}
