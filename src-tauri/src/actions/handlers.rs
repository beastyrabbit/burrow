use crate::actions::modifier::Modifier;
use crate::actions::utils;
use crate::commands::onepass;
use crate::router::SearchResult;

/// Check whether a category string has a handler in the action dispatcher.
/// Note: "chat" category is handled separately by the frontend and is intentionally excluded.
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
        "action" => Ok(()), // No-op: "action" results are dispatched by frontend via run_setting() command
        "info" => Ok(()),
        _ => Err(format!("Unknown category: {}", result.category)),
    }
}

fn handle_onepass(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    if result.exec == "op-load-vault" {
        // Spawn in a thread because load_vault does blocking I/O + stdin prompts
        std::thread::spawn(|| match onepass::load_vault() {
            Ok(msg) => eprintln!("[1pass] {msg}"),
            Err(e) => eprintln!("[1pass] vault load failed: {e}"),
        });
        return Ok(());
    }

    let item_id = result
        .exec
        .strip_prefix("op-vault-item:")
        .ok_or_else(|| "Could not extract 1Password item ID".to_string())?;

    // Hide window immediately
    utils::hide_window(app);

    match modifier {
        Modifier::Shift => {
            let id = item_id.to_string();
            std::thread::spawn(move || match onepass::get_password(&id) {
                Ok(pw) => {
                    if let Err(e) = utils::copy_to_clipboard(&pw) {
                        eprintln!("[1pass] copy password failed: {e}");
                    }
                }
                Err(e) => eprintln!("[1pass] get password failed: {e}"),
            });
            Ok(())
        }
        Modifier::Ctrl => {
            let id = item_id.to_string();
            std::thread::spawn(move || match onepass::get_username(&id) {
                Ok(user) => {
                    if let Err(e) = utils::copy_to_clipboard(&user) {
                        eprintln!("[1pass] copy username failed: {e}");
                    }
                }
                Err(e) => eprintln!("[1pass] get username failed: {e}"),
            });
            Ok(())
        }
        _ => {
            let id = item_id.to_string();
            std::thread::spawn(move || match onepass::get_password(&id) {
                Ok(pw) => {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if let Err(e) = std::process::Command::new("wtype")
                        .arg("--")
                        .arg(&pw)
                        .status()
                    {
                        eprintln!("[1pass] wtype failed (is wtype installed?): {e}");
                    }
                }
                Err(e) => eprintln!("[1pass] get password failed: {e}"),
            });
            Ok(())
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
    // Data contract: exec = Host alias only, description = "user@hostname" or "hostname"
    // This avoids shell interpolation by passing the alias directly to Command::arg()
    let host = &result.exec;
    let user = extract_user_from_description(&result.description);

    match modifier {
        Modifier::Ctrl => {
            // Copy "ssh user@host" to clipboard
            let target = match &user {
                Some(u) => format!("{}@{}", u, host),
                None => host.clone(),
            };
            utils::copy_to_clipboard(&format!("ssh {target}"))
        }
        _ => {
            // Default + Shift: launch SSH connection safely (no shell interpolation)
            utils::exec_ssh(host, user.as_deref())
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

/// Extract the user from SSH description (format: "user@hostname" or "hostname").
/// Returns Some(user) if present, None otherwise.
fn extract_user_from_description(description: &str) -> Option<String> {
    description.split_once('@').map(|(user, _)| user.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_user_from_description_with_user() {
        let desc = "admin@192.168.1.10";
        assert_eq!(extract_user_from_description(desc), Some("admin".to_string()));
    }

    #[test]
    fn extract_user_from_description_no_user() {
        let desc = "192.168.1.10";
        assert_eq!(extract_user_from_description(desc), None);
    }

    #[test]
    fn extract_user_from_description_hostname() {
        let desc = "deploy@example.com";
        assert_eq!(extract_user_from_description(desc), Some("deploy".to_string()));
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
