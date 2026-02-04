use super::socket::pid_path;
use std::fs;
use std::process;

/// Write the current process ID to the PID file.
pub fn write_pid_file() -> Result<(), String> {
    let path = pid_path();

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create PID dir: {e}"))?;
    }

    let pid = process::id();
    fs::write(&path, pid.to_string()).map_err(|e| format!("failed to write PID file: {e}"))?;

    tracing::debug!(pid, path = %path.display(), "wrote PID file");
    Ok(())
}

/// Remove the PID file.
pub fn remove_pid_file() -> Result<(), String> {
    let path = pid_path();

    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("failed to remove PID file: {e}"))?;
        tracing::debug!(path = %path.display(), "removed PID file");
    }

    Ok(())
}

/// Read the PID from the PID file.
pub fn read_pid() -> Option<u32> {
    let path = pid_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Check if a process with the given PID is running.
fn is_process_alive(pid: u32) -> bool {
    // On Unix, sending signal 0 checks if process exists without affecting it
    #[cfg(unix)]
    {
        // Use kill -0 to check if process exists
        // SAFETY: kill with signal 0 only checks if the process exists, it doesn't affect it
        let result = unsafe { libc::kill(pid as i32, 0) };
        result == 0
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, assume process is alive if we can't check
        let _ = pid;
        true
    }
}

/// Check if a daemon process is currently running.
///
/// Returns `Some(pid)` if a daemon is running, `None` otherwise.
/// Cleans up stale PID files automatically.
pub fn is_daemon_running() -> Option<u32> {
    let pid = read_pid()?;

    if is_process_alive(pid) {
        Some(pid)
    } else {
        // Stale PID file - clean it up
        tracing::debug!(pid, "found stale PID file, removing");
        let _ = remove_pid_file();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_pid() {
        // Use a temp directory
        let tmp = tempfile::TempDir::new().unwrap();
        let pid_file = tmp.path().join("burrow/burrow.pid");

        // Create parent directory
        fs::create_dir_all(pid_file.parent().unwrap()).unwrap();
        fs::write(&pid_file, "12345").unwrap();

        let content = fs::read_to_string(&pid_file).unwrap();
        assert_eq!(content, "12345");
    }

    #[test]
    fn is_process_alive_current_process() {
        let current_pid = process::id();
        assert!(
            is_process_alive(current_pid),
            "current process should be alive"
        );
    }

    #[test]
    fn is_process_alive_nonexistent() {
        // PID 0 is never a valid user process
        // A very high PID is unlikely to exist
        let unlikely_pid = 999_999_999;
        assert!(
            !is_process_alive(unlikely_pid),
            "nonexistent process should not be alive"
        );
    }

    #[test]
    fn read_pid_parses_number() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Test with whitespace
        let pid_file = tmp.path().join("test.pid");
        fs::write(&pid_file, "  12345\n").unwrap();
        let content = fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        assert_eq!(content, Some(12345));
    }

    #[test]
    fn read_pid_invalid_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pid_file = tmp.path().join("test.pid");
        fs::write(&pid_file, "not-a-number").unwrap();
        let content = fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        assert_eq!(content, None);
    }
}
