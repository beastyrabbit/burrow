use super::dry_run;
use std::process::Command;

/// Run a shell command string via `sh -c`.
pub fn exec_shell(cmd: &str) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::exec_shell(cmd);
    }
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")))
        .spawn()
        .map_err(|e| format!("Failed to exec: {e}"))?;
    Ok(())
}

/// Copy text to clipboard using wl-copy.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::copy_to_clipboard(text);
    }
    Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .spawn()
        .map_err(|e| format!("Failed to copy to clipboard: {e}"))?;
    Ok(())
}

/// Type text via wtype (Wayland). Hides window first, sleeps 1s, then types.
pub fn type_text_wayland(text: &str, app: &tauri::AppHandle) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::type_text_wayland(text, app);
    }
    hide_window(app);

    // Sleep 1s then type — run as a background process
    let text = text.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let _ = Command::new("wtype").arg("--").arg(&text).status();
    });

    Ok(())
}

/// Get the user's terminal command (from $TERMINAL or fallback to foot).
pub fn get_terminal_cmd() -> String {
    std::env::var("TERMINAL").unwrap_or_else(|_| "foot".into())
}

/// Hide the Tauri window.
pub fn hide_window(app: &tauri::AppHandle) {
    if dry_run::is_enabled() {
        dry_run::hide_window(app);
        return;
    }
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

/// Launch SSH connection safely without shell interpolation.
/// Uses Command::arg() to prevent shell injection.
pub fn exec_ssh(host: &str, user: Option<&str>) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::exec_ssh(host, user);
    }
    let terminal = get_terminal_cmd();
    let mut cmd = Command::new(&terminal);
    cmd.current_dir(dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")));
    cmd.arg("ssh");
    cmd.arg("--"); // Prevent option injection (e.g., host starting with "-")
    if let Some(u) = user {
        cmd.arg(format!("{}@{}", u, host));
    } else {
        cmd.arg(host);
    }
    cmd.spawn()
        .map_err(|e| format!("Failed to launch SSH: {e}"))?;
    Ok(())
}

/// Open a path with xdg-open.
pub fn xdg_open(path: &str) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::xdg_open(path);
    }
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open: {e}"))?;
    Ok(())
}

/// Open a path's parent directory in the terminal.
pub fn open_dir_in_terminal(path: &str) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::open_dir_in_terminal(path);
    }
    let dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    let terminal = get_terminal_cmd();
    Command::new(&terminal)
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Failed to open terminal: {e}"))?;
    Ok(())
}

/// Open a path in VS Code.
pub fn open_in_vscode(path: &str) -> Result<(), String> {
    if dry_run::is_enabled() {
        return dry_run::open_in_vscode(path);
    }
    Command::new("code")
        .arg("--")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open in VS Code: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Environment variable tests are inherently flaky when run in parallel
    // because env vars are process-global. Run with --test-threads=1 for reliability.
    #[test]
    #[ignore = "flaky due to parallel env var access; run with --test-threads=1"]
    fn terminal_fallback() {
        let original = std::env::var("TERMINAL").ok();
        std::env::remove_var("TERMINAL");
        assert_eq!(get_terminal_cmd(), "foot");
        if let Some(val) = original {
            std::env::set_var("TERMINAL", val);
        }
    }

    #[test]
    #[ignore = "flaky due to parallel env var access; run with --test-threads=1"]
    fn terminal_from_env() {
        let original = std::env::var("TERMINAL").ok();
        std::env::set_var("TERMINAL", "kitty");
        assert_eq!(get_terminal_cmd(), "kitty");
        if let Some(val) = original {
            std::env::set_var("TERMINAL", val);
        } else {
            std::env::remove_var("TERMINAL");
        }
    }
}
