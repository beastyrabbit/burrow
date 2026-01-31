use crate::actions::modifier::Modifier;
use crate::actions::utils;
use crate::commands::onepass;
use crate::router::SearchResult;

/// Check whether a category string is recognized by the action dispatcher.
pub fn is_valid_category(category: &str) -> bool {
    matches!(
        category,
        "onepass"
            | "file"
            | "vector"
            | "app"
            | "history"
            | "ssh"
            | "math"
            | "action"
            | "info"
            | "special"
    )
}

pub fn handle_action(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    match result.category.as_str() {
        "onepass" => handle_onepass(result, modifier, app),
        "file" | "vector" => handle_file(result, modifier, app),
        "app" | "history" | "special" => handle_launch(result, app),
        "ssh" => handle_ssh(result, modifier),
        "math" => handle_math(result, modifier),
        "action" => Ok(()), // Defensive no-op: frontend dispatches via run_setting
        "info" => Ok(()),
        _ => Err(format!("Unknown category: {}", result.category)),
    }
}

fn handle_onepass(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let item_id = onepass::extract_item_id(&result.exec)
        .ok_or_else(|| "Could not extract 1Password item ID".to_string())?;

    match modifier {
        Modifier::Shift => {
            let password = onepass::get_password(&item_id)?;
            utils::copy_to_clipboard(&password)
        }
        Modifier::Ctrl => {
            let username = onepass::get_username(&item_id)?;
            utils::copy_to_clipboard(&username)
        }
        _ => {
            // Default: type password via wtype
            let password = onepass::get_password(&item_id)?;
            utils::type_text_wayland(&password, app)
        }
    }
}

fn handle_file(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let path = &result.id; // file/vector results use id as the path
    utils::hide_window(app);
    match modifier {
        Modifier::Shift => utils::open_dir_in_terminal(path),
        Modifier::Ctrl => utils::open_in_vscode(path),
        _ => utils::xdg_open(path),
    }
}

fn handle_launch(result: &SearchResult, app: &tauri::AppHandle) -> Result<(), String> {
    utils::hide_window(app);
    utils::exec_shell(&result.exec)
}

fn handle_ssh(result: &SearchResult, modifier: Modifier) -> Result<(), String> {
    match modifier {
        Modifier::Ctrl => {
            // Copy "ssh user@host" to clipboard
            // Parse from exec which is like "kitty ssh 'user@host'"
            let ssh_target = extract_ssh_target(&result.exec);
            utils::copy_to_clipboard(&format!("ssh {ssh_target}"))
        }
        _ => {
            // Default + Shift: launch SSH connection
            utils::exec_shell(&result.exec)
        }
    }
}

fn handle_math(result: &SearchResult, modifier: Modifier) -> Result<(), String> {
    match modifier {
        Modifier::Shift | Modifier::Ctrl => {
            // Copy the result value (strip the "= " prefix from name)
            let value = result.name.strip_prefix("= ").unwrap_or(&result.name);
            utils::copy_to_clipboard(value)
        }
        _ => Ok(()), // No-op for plain Enter
    }
}

/// Extract the SSH target from an exec string like "kitty ssh 'user@host'"
fn extract_ssh_target(exec: &str) -> String {
    if let Some(idx) = exec.find("ssh ") {
        let rest = &exec[idx + 4..];
        rest.split_whitespace()
            .last()
            .unwrap_or(rest)
            .trim_matches(&['\'', '"'][..])
            .to_string()
    } else {
        exec.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ssh_target_with_user() {
        let exec = "kitty ssh 'admin@server1'";
        assert_eq!(extract_ssh_target(exec), "admin@server1");
    }

    #[test]
    fn extract_ssh_target_no_user() {
        let exec = "kitty ssh 'myhost'";
        assert_eq!(extract_ssh_target(exec), "myhost");
    }

    #[test]
    fn extract_ssh_target_no_quotes() {
        let exec = "kitty ssh admin@server1";
        assert_eq!(extract_ssh_target(exec), "admin@server1");
    }

    #[test]
    fn extract_ssh_target_double_quotes() {
        let exec = r#"kitty ssh "admin@server1""#;
        assert_eq!(extract_ssh_target(exec), "admin@server1");
    }

    #[test]
    fn extract_ssh_target_empty_after_ssh() {
        let exec = "kitty ssh ";
        assert_eq!(extract_ssh_target(exec), "");
    }

    #[test]
    fn handle_math_none_is_noop() {
        let result = SearchResult {
            id: "math-result".into(),
            name: "= 42".into(),
            description: "6*7 = 42".into(),
            icon: "".into(),
            category: "math".into(),
            exec: "".into(),
        };
        assert!(handle_math(&result, Modifier::None).is_ok());
    }

    #[test]
    fn unknown_category_is_invalid() {
        assert!(!is_valid_category("unknown"));
        assert!(!is_valid_category(""));
        assert!(!is_valid_category("ONEPASS"));
    }

    #[test]
    fn all_known_categories_are_valid() {
        for cat in &[
            "onepass", "file", "vector", "app", "history", "ssh", "math", "action", "info",
            "special",
        ] {
            assert!(is_valid_category(cat), "{cat} should be valid");
        }
    }
}
