//! Dry-run mode: when `BURROW_DRY_RUN` is set, all side-effectful actions
//! (launching apps, clipboard, xdg-open, etc.) are replaced with stderr logging.
//! Used by Playwright tests and CI to prevent real app launches.

use std::sync::OnceLock;

static DRY_RUN: OnceLock<bool> = OnceLock::new();

/// Returns true when `BURROW_DRY_RUN` env var is set to a truthy value.
/// The result is cached on first call for the lifetime of the process.
pub fn is_enabled() -> bool {
    *DRY_RUN.get_or_init(|| parse_truthy(&std::env::var("BURROW_DRY_RUN").unwrap_or_default()))
}

/// Parse a string as a truthy boolean. Empty, "0", and "false" (case-insensitive) are falsy.
fn parse_truthy(val: &str) -> bool {
    !val.is_empty() && val != "0" && val.to_lowercase() != "false"
}

pub fn exec_shell(cmd: &str) -> Result<(), String> {
    eprintln!("[dry-run] exec_shell: {cmd}");
    Ok(())
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    eprintln!("[dry-run] copy_to_clipboard: {}", truncate(text, 40));
    Ok(())
}

pub fn type_text_wayland(_text: &str, _app: &tauri::AppHandle) -> Result<(), String> {
    eprintln!("[dry-run] type_text_wayland");
    Ok(())
}

pub fn hide_window(_app: &tauri::AppHandle) {
    eprintln!("[dry-run] hide_window");
}

pub fn xdg_open(path: &str) -> Result<(), String> {
    eprintln!("[dry-run] xdg_open: {path}");
    Ok(())
}

pub fn open_dir_in_terminal(path: &str) -> Result<(), String> {
    eprintln!("[dry-run] open_dir_in_terminal: {path}");
    Ok(())
}

pub fn open_in_vscode(path: &str) -> Result<(), String> {
    eprintln!("[dry-run] open_in_vscode: {path}");
    Ok(())
}

pub fn launch_app(exec: &str) -> Result<(), String> {
    eprintln!("[dry-run] launch_app: {exec}");
    Ok(())
}

pub fn exec_ssh(host: &str, user: Option<&str>) -> Result<(), String> {
    let target = match user {
        Some(u) => format!("{}@{}", u, host),
        None => host.to_string(),
    };
    eprintln!("[dry-run] exec_ssh: {target}");
    Ok(())
}

/// Truncate a string to at most `max_chars` characters (UTF-8 safe).
pub fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_truthy_common_values() {
        assert!(parse_truthy("1"));
        assert!(parse_truthy("true"));
        assert!(parse_truthy("TRUE"));
        assert!(parse_truthy("yes"));
        assert!(!parse_truthy(""));
        assert!(!parse_truthy("0"));
        assert!(!parse_truthy("false"));
        assert!(!parse_truthy("False"));
        assert!(!parse_truthy("FALSE"));
    }

    #[test]
    fn truncate_ascii() {
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn truncate_multibyte_safe() {
        // 3 emoji = 3 chars but 12 bytes; truncating at char 2 must not panic
        let text = "🎉🎉🎉";
        assert_eq!(truncate(text, 2), "🎉🎉");
    }

    #[test]
    fn copy_to_clipboard_handles_multibyte_utf8() {
        // Emoji at boundary — must not panic
        let text = "a]".repeat(20) + "🎉🎉🎉";
        assert!(copy_to_clipboard(&text).is_ok());
    }
}
