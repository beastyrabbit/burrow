use std::process::Command;

/// Run a shell command string via `sh -c`.
pub fn exec_shell(cmd: &str) -> Result<(), String> {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .spawn()
        .map_err(|e| format!("Failed to exec: {e}"))?;
    Ok(())
}

/// Copy text to clipboard using wl-copy.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .spawn()
        .map_err(|e| format!("Failed to copy to clipboard: {e}"))?;
    Ok(())
}

/// Type text via wtype (Wayland). Hides window first, sleeps 1s, then types.
pub fn type_text_wayland(text: &str, app: &tauri::AppHandle) -> Result<(), String> {
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
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

/// Open a path with xdg-open.
pub fn xdg_open(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Failed to open: {e}"))?;
    Ok(())
}

/// Open a path's parent directory in the terminal.
pub fn open_dir_in_terminal(path: &str) -> Result<(), String> {
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

    #[test]
    fn terminal_fallback() {
        let original = std::env::var("TERMINAL").ok();
        std::env::remove_var("TERMINAL");
        assert_eq!(get_terminal_cmd(), "foot");
        if let Some(val) = original {
            std::env::set_var("TERMINAL", val);
        }
    }

    #[test]
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
