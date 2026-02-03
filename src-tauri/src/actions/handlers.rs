use crate::actions::modifier::Modifier;
use crate::actions::utils;
use crate::commands::onepass;
use crate::router::{Category, SearchResult};
use serde::Serialize;
use tauri::Emitter;

/// Check whether a category has a handler in the action dispatcher.
/// Note: Chat category is handled separately by the frontend and is intentionally excluded.
pub fn is_valid_category(category: Category) -> bool {
    matches!(
        category,
        Category::Onepass
            | Category::File
            | Category::Vector
            | Category::App
            | Category::History
            | Category::Ssh
            | Category::Math
            | Category::Action
            | Category::Info
            | Category::Special
    )
}

pub fn handle_action(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    match result.category {
        Category::Onepass => handle_onepass(result, modifier, app),
        Category::File | Category::Vector => handle_file(result, modifier, app),
        Category::App | Category::History | Category::Special => handle_launch(result, app),
        Category::Ssh => handle_ssh(result, modifier),
        Category::Math => handle_math(result, modifier),
        Category::Action => Ok(()), // No-op: action results are dispatched by frontend via run_setting() command
        Category::Info => Ok(()),
        Category::Chat => Ok(()), // Handled by frontend
    }
}

/// Payload for vault-load-result events sent to the frontend.
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct VaultLoadResult {
    pub ok: bool,
    pub message: String,
}

impl VaultLoadResult {
    /// Create a success result with the given message.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    /// Create a failure result with the given error message.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

fn handle_onepass(
    result: &SearchResult,
    modifier: Modifier,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    if result.exec == "op-load-vault" {
        // Spawn in a thread because load_vault does blocking I/O + stdin prompts
        let app_handle = app.clone();
        std::thread::spawn(move || {
            let payload = match onepass::load_vault() {
                Ok(msg) => {
                    eprintln!("[1pass] {msg}");
                    VaultLoadResult::success(msg)
                }
                Err(e) => {
                    eprintln!("[1pass] vault load failed: {e}");
                    VaultLoadResult::failure(e)
                }
            };
            if let Err(e) = app_handle.emit("vault-load-result", payload) {
                eprintln!("[1pass] failed to emit vault-load-result event: {e}");
            }
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
    description
        .split_once('@')
        .map(|(user, _)| user.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_user_from_description_with_user() {
        let desc = "admin@192.168.1.10";
        assert_eq!(
            extract_user_from_description(desc),
            Some("admin".to_string())
        );
    }

    #[test]
    fn extract_user_from_description_no_user() {
        let desc = "192.168.1.10";
        assert_eq!(extract_user_from_description(desc), None);
    }

    #[test]
    fn extract_user_from_description_hostname() {
        let desc = "deploy@example.com";
        assert_eq!(
            extract_user_from_description(desc),
            Some("deploy".to_string())
        );
    }

    #[test]
    fn handle_math_none_is_noop() {
        let result = SearchResult {
            id: "math-result".into(),
            name: "= 42".into(),
            description: "6*7 = 42".into(),
            icon: "".into(),
            category: Category::Math,
            exec: "".into(),
        };
        assert!(handle_math(&result, Modifier::None).is_ok());
    }

    #[test]
    fn chat_category_is_not_handled_by_dispatcher() {
        // Chat is handled by frontend, not by handle_action dispatcher
        assert!(!is_valid_category(Category::Chat));
    }

    #[test]
    fn all_dispatchable_categories_are_valid() {
        let categories = [
            Category::Onepass,
            Category::File,
            Category::Vector,
            Category::App,
            Category::History,
            Category::Ssh,
            Category::Math,
            Category::Action,
            Category::Info,
            Category::Special,
        ];
        for cat in categories {
            assert!(is_valid_category(cat), "{cat:?} should be valid");
        }
    }

    #[test]
    fn vault_load_result_success_constructs_correctly() {
        let result = VaultLoadResult::success("Loaded 42 items from vault");
        assert!(result.ok, "success result should have ok=true");
        assert_eq!(result.message, "Loaded 42 items from vault");
    }

    #[test]
    fn vault_load_result_failure_constructs_correctly() {
        let result = VaultLoadResult::failure("Authentication failed");
        assert!(!result.ok, "failure result should have ok=false");
        assert_eq!(result.message, "Authentication failed");
    }

    #[test]
    fn vault_load_result_serializes_to_json() {
        let success = VaultLoadResult::success("Loaded 5 items");
        let json = serde_json::to_string(&success).expect("should serialize");
        assert!(json.contains(r#""ok":true"#), "JSON should contain ok:true");
        assert!(
            json.contains(r#""message":"Loaded 5 items""#),
            "JSON should contain message"
        );

        let failure = VaultLoadResult::failure("Network error");
        let json = serde_json::to_string(&failure).expect("should serialize");
        assert!(
            json.contains(r#""ok":false"#),
            "JSON should contain ok:false"
        );
        assert!(
            json.contains(r#""message":"Network error""#),
            "JSON should contain error message"
        );
    }

    #[test]
    fn vault_load_result_from_result_type() {
        // Simulate the pattern used in handle_onepass
        let ok_result: Result<String, String> = Ok("Loaded 10 items".to_string());
        let payload = match ok_result {
            Ok(msg) => VaultLoadResult::success(msg),
            Err(e) => VaultLoadResult::failure(e),
        };
        assert_eq!(
            payload,
            VaultLoadResult {
                ok: true,
                message: "Loaded 10 items".to_string()
            }
        );

        let err_result: Result<String, String> = Err("op CLI not found".to_string());
        let payload = match err_result {
            Ok(msg) => VaultLoadResult::success(msg),
            Err(e) => VaultLoadResult::failure(e),
        };
        assert_eq!(
            payload,
            VaultLoadResult {
                ok: false,
                message: "op CLI not found".to_string()
            }
        );
    }
}
