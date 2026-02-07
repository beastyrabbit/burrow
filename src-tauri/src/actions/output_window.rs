use crate::output_buffers::OutputBufferState;
use crate::window_manager::Stream;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

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

    register_window_close_handler(app, &label, child.id());

    // Wait for both stream readers to finish, logging panics
    let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);
    if let Err(e) = stdout_result {
        tracing::error!(error = %e, "stdout reader task panicked");
    }
    if let Err(e) = stderr_result {
        tracing::error!(error = %e, "stderr reader task panicked");
    }

    // Wait for the process to exit and get the exit code
    let exit_code = match child.wait().await {
        Ok(status) => status.code(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to wait for child process");
            None
        }
    };

    buffers.set_done(&label, exit_code);
    tracing::info!(label = %label, ?exit_code, "output window command finished");

    Ok(())
}

/// Register a listener that sends SIGTERM to the child process when the window is closed.
#[cfg(unix)]
fn register_window_close_handler(app: &tauri::AppHandle, label: &str, child_pid: Option<u32>) {
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

    // Use an Arc<AtomicBool> to track whether we've already signaled the process
    let killed = Arc::new(std::sync::atomic::AtomicBool::new(false));

    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            if !killed.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::info!(pid, "output window closed, sending SIGTERM to child");
                // SAFETY: pid_i32 is a valid i32 PID obtained from the child process.
                // The process may have already exited, in which case kill() returns -1/ESRCH
                // which we log but treat as non-fatal.
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
fn register_window_close_handler(_app: &tauri::AppHandle, _label: &str, _child_pid: Option<u32>) {
    tracing::warn!("window close handler is only supported on unix");
}
