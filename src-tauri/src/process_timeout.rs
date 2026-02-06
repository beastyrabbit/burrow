use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Wait for a child process to exit until `timeout`.
/// Returns `Ok(Some(status))` if it exited, `Ok(None)` on timeout.
pub fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> Result<Option<ExitStatus>, std::io::Error> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Best-effort terminate + reap.
pub fn kill_and_reap(child: &mut Child) {
    if let Err(e) = child.kill() {
        if e.kind() != std::io::ErrorKind::InvalidInput {
            tracing::warn!(error = %e, "failed to kill child process");
        }
    }
    if let Err(e) = child.wait() {
        tracing::warn!(error = %e, "failed to wait for child process");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn wait_with_timeout_reports_exit() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .expect("spawn should succeed");
        let status = wait_with_timeout(&mut child, Duration::from_secs(1))
            .expect("wait should succeed")
            .expect("process should exit before timeout");
        assert!(status.success());
    }

    #[test]
    fn wait_with_timeout_reports_timeout() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 1"])
            .spawn()
            .expect("spawn should succeed");
        let status =
            wait_with_timeout(&mut child, Duration::from_millis(10)).expect("wait should succeed");
        assert!(status.is_none(), "expected timeout for sleeping process");
        kill_and_reap(&mut child);
    }
}
