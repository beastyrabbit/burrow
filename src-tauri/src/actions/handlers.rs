use crate::actions::modifier::Modifier;
use crate::actions::{output_window, utils};
use crate::commands::{apps, onepass, special};
use crate::context::AppContext;
use crate::router::{Category, OutputMode, SearchResult};
use serde::Serialize;

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
            | Category::Info
            | Category::Special
    )
}

/// Resolve user-provided results to trusted, canonical command payloads.
/// This prevents untrusted clients from injecting arbitrary `exec` values.
fn resolve_trusted_result(result: &SearchResult) -> Result<SearchResult, String> {
    match result.category {
        Category::App | Category::History => {
            let exec = apps::resolve_app_exec(&result.id)
                .ok_or_else(|| format!("Unknown app id for action: {}", result.id))?;
            let mut trusted = result.clone();
            trusted.exec = exec;
            trusted.input_spec = None;
            trusted.output_mode = None;
            Ok(trusted)
        }
        Category::Special => special::resolve_special_by_id(&result.id)
            .ok_or_else(|| format!("Unknown special command id: {}", result.id)),
        Category::Onepass => {
            let mut trusted = result.clone();
            trusted.input_spec = None;
            if trusted.id == "op-load-vault" {
                trusted.exec = "op-load-vault".to_string();
                return Ok(trusted);
            }
            if let Some(item_id) = trusted.id.strip_prefix("op-") {
                if item_id.is_empty() {
                    return Err("Invalid 1Password result id".to_string());
                }
                trusted.exec = format!("op-vault-item:{item_id}");
                return Ok(trusted);
            }
            Err(format!("Invalid 1Password result id: {}", trusted.id))
        }
        _ => {
            // For non-shell categories we do not rely on input_spec.
            let mut trusted = result.clone();
            trusted.input_spec = None;
            Ok(trusted)
        }
    }
}

pub fn handle_action(
    result: &SearchResult,
    modifier: Modifier,
    secondary_input: Option<&str>,
    ctx: &AppContext,
) -> Result<(), String> {
    let trusted = resolve_trusted_result(result)?;

    match trusted.category {
        Category::Onepass => handle_onepass(&trusted, modifier, ctx),
        Category::File | Category::Vector => handle_file(&trusted, modifier, ctx),
        Category::App | Category::History | Category::Special => {
            handle_launch(&trusted, ctx, secondary_input)
        }
        Category::Ssh => handle_ssh(&trusted, modifier),
        Category::Math => handle_math(&trusted, modifier),
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
    ctx: &AppContext,
) -> Result<(), String> {
    if result.exec == "op-load-vault" {
        // Spawn in a thread because load_vault does blocking I/O + stdin prompts
        let app_handle = ctx.clone_app_handle();
        std::thread::spawn(move || {
            let payload = match onepass::load_vault() {
                Ok(msg) => {
                    tracing::info!(message = %msg, "1Password vault loaded");
                    VaultLoadResult::success(msg)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "1Password vault load failed");
                    VaultLoadResult::failure(e)
                }
            };
            if let Some(ref app) = app_handle {
                use tauri::Emitter;
                if let Err(e) = app.emit("vault-load-result", payload) {
                    tracing::error!(error = %e, "failed to emit vault-load-result event");
                }
            } else {
                tracing::debug!("[no-window] vault-load-result emit skipped");
            }
        });
        return Ok(());
    }

    let item_id = result
        .exec
        .strip_prefix("op-vault-item:")
        .ok_or_else(|| "Could not extract 1Password item ID".to_string())?;

    ctx.hide_window();

    match modifier {
        Modifier::Shift => {
            let id = item_id.to_string();
            std::thread::spawn(move || match onepass::get_password(&id) {
                Ok(pw) => {
                    if let Err(e) = utils::copy_to_clipboard(&pw) {
                        tracing::warn!(error = %e, "1Password copy password failed");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "1Password get password failed"),
            });
            Ok(())
        }
        Modifier::Ctrl => {
            let id = item_id.to_string();
            std::thread::spawn(move || match onepass::get_username(&id) {
                Ok(user) => {
                    if let Err(e) = utils::copy_to_clipboard(&user) {
                        tracing::warn!(error = %e, "1Password copy username failed");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "1Password get username failed"),
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
                        tracing::warn!(error = %e, "wtype failed (is wtype installed?)");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "1Password get password failed"),
            });
            Ok(())
        }
    }
}

fn handle_file(result: &SearchResult, modifier: Modifier, ctx: &AppContext) -> Result<(), String> {
    let path = &result.id; // file/vector results use id as the path
    ctx.hide_window();
    match modifier {
        Modifier::Shift => utils::open_dir_in_terminal(path),
        Modifier::Ctrl => utils::open_in_vscode(path),
        _ => utils::xdg_open(path),
    }
}

/// Escape user input for safe shell interpolation by wrapping in single quotes.
/// Single quotes prevent all shell interpretation except for single quotes themselves,
/// which are escaped using the `'\''` technique (end quote, escaped literal quote, restart quote).
fn escape_for_single_quotes(input: &str) -> String {
    // Replace ' with '\'' (end single quote, add escaped literal quote, start single quote)
    input.replace('\'', "'\\''")
}

/// Resolve final command, applying secondary input if provided.
/// The input is wrapped in single quotes for shell safety.
fn resolve_exec(result: &SearchResult, secondary_input: Option<&str>) -> String {
    match (&result.input_spec, secondary_input) {
        (Some(spec), Some(input)) if !input.is_empty() => {
            if !spec.template.contains("{}") {
                tracing::warn!(
                    template = %spec.template,
                    "input_spec template missing {{}} placeholder; input will be ignored"
                );
                return result.exec.clone();
            }
            // Wrap in single quotes for consistent shell safety
            let escaped = format!("'{}'", escape_for_single_quotes(input));
            spec.template.replace("{}", &escaped)
        }
        _ => result.exec.clone(),
    }
}

fn handle_launch(
    result: &SearchResult,
    ctx: &AppContext,
    secondary_input: Option<&str>,
) -> Result<(), String> {
    let cmd = resolve_exec(result, secondary_input);
    ctx.hide_window();

    match result.output_mode {
        Some(OutputMode::Window) => {
            if let Some(app) = ctx.clone_app_handle() {
                let title = result.name.clone();
                let buffers = ctx.output_buffers.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) =
                        output_window::run_in_output_window(cmd, title, &app, buffers).await
                    {
                        tracing::error!(error = %e, "output window execution failed");
                    }
                });
                Ok(())
            } else {
                // test-server fallback: no app handle, fire-and-forget
                utils::exec_shell(&cmd)
            }
        }
        _ => utils::exec_shell(&cmd),
    }
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
            input_spec: None,
            output_mode: None,
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
            Category::Info,
            Category::Special,
        ];
        for cat in categories {
            assert!(is_valid_category(cat), "{cat:?} should be valid");
        }
    }

    #[test]
    fn resolve_trusted_result_rejects_unknown_app_id() {
        let forged = SearchResult {
            id: "definitely-not-real".into(),
            name: "Forged".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::App,
            exec: "rm -rf /".into(),
            input_spec: None,
            output_mode: None,
        };
        let err = resolve_trusted_result(&forged).unwrap_err();
        assert!(err.contains("Unknown app id"));
    }

    #[test]
    fn resolve_trusted_result_overrides_special_exec() {
        let forged = SearchResult {
            id: "special-cowork".into(),
            name: "cowork".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "rm -rf /".into(),
            input_spec: Some(InputSpec {
                placeholder: "p".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let trusted = resolve_trusted_result(&forged).expect("special command should resolve");
        assert_ne!(trusted.exec, "rm -rf /");
        assert!(trusted.exec.contains("claude"));
        assert!(trusted.input_spec.is_some());
    }

    #[test]
    fn resolve_trusted_result_derives_onepass_exec_from_id() {
        let forged = SearchResult {
            id: "op-abc123".into(),
            name: "1Password".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Onepass,
            exec: "malicious".into(),
            input_spec: Some(InputSpec {
                placeholder: "p".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let trusted = resolve_trusted_result(&forged).expect("onepass result should resolve");
        assert_eq!(trusted.exec, "op-vault-item:abc123");
        assert!(trusted.input_spec.is_none());
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

    // --- resolve_exec tests ---

    use crate::router::InputSpec;

    #[test]
    fn resolve_exec_without_input_spec_returns_exec() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::App,
            exec: "default-command".into(),
            input_spec: None,
            output_mode: None,
        };
        assert_eq!(
            resolve_exec(&result, None),
            "default-command",
            "should return exec when no input_spec"
        );
        assert_eq!(
            resolve_exec(&result, Some("ignored")),
            "default-command",
            "should return exec when input_spec is None even with secondary input"
        );
    }

    #[test]
    fn resolve_exec_with_input_spec_but_no_input_returns_exec() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base-command".into(),
            input_spec: Some(InputSpec {
                placeholder: "Enter value".into(),
                template: "templated-command {}".into(),
            }),
            output_mode: None,
        };
        assert_eq!(
            resolve_exec(&result, None),
            "base-command",
            "should return exec when secondary_input is None"
        );
        assert_eq!(
            resolve_exec(&result, Some("")),
            "base-command",
            "should return exec when secondary_input is empty"
        );
    }

    #[test]
    fn resolve_exec_with_input_substitutes_template() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base-command".into(),
            input_spec: Some(InputSpec {
                placeholder: "Enter value".into(),
                template: "templated-command {}".into(),
            }),
            output_mode: None,
        };
        // Input is wrapped in single quotes
        assert_eq!(
            resolve_exec(&result, Some("my-value")),
            "templated-command 'my-value'",
            "should substitute input wrapped in single quotes"
        );
    }

    #[test]
    fn resolve_exec_escapes_single_quotes() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base".into(),
            input_spec: Some(InputSpec {
                placeholder: "".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let output = resolve_exec(&result, Some("it's a test"));
        // Single quotes are escaped using '\'' technique inside single-quoted string
        assert_eq!(
            output, "echo 'it'\\''s a test'",
            "should wrap in single quotes and escape internal single quotes"
        );
    }

    #[test]
    fn resolve_exec_preserves_special_chars_in_single_quotes() {
        // Single quotes protect against all shell metacharacters except single quotes
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base".into(),
            input_spec: Some(InputSpec {
                placeholder: "".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        // All these dangerous chars are safe inside single quotes
        let output = resolve_exec(&result, Some("$HOME; rm -rf / | cat && whoami > /tmp/x"));
        assert_eq!(
            output, "echo '$HOME; rm -rf / | cat && whoami > /tmp/x'",
            "shell metacharacters should be preserved literally inside single quotes"
        );
    }

    #[test]
    fn resolve_exec_handles_backticks_in_single_quotes() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base".into(),
            input_spec: Some(InputSpec {
                placeholder: "".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let output = resolve_exec(&result, Some("`whoami`"));
        // Backticks are safe inside single quotes
        assert_eq!(output, "echo '`whoami`'");
    }

    #[test]
    fn resolve_exec_handles_dollar_in_single_quotes() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base".into(),
            input_spec: Some(InputSpec {
                placeholder: "".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let output = resolve_exec(&result, Some("$(cat /etc/passwd)"));
        // $() is safe inside single quotes
        assert_eq!(output, "echo '$(cat /etc/passwd)'");
    }

    #[test]
    fn resolve_exec_handles_double_quotes_in_single_quotes() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base".into(),
            input_spec: Some(InputSpec {
                placeholder: "".into(),
                template: "echo {}".into(),
            }),
            output_mode: None,
        };
        let output = resolve_exec(&result, Some("say \"hello\""));
        // Double quotes are safe inside single quotes
        assert_eq!(output, "echo 'say \"hello\"'");
    }

    #[test]
    fn resolve_exec_template_without_placeholder_returns_base_exec() {
        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base-command".into(),
            input_spec: Some(InputSpec {
                placeholder: "Enter input".into(),
                template: "broken-template-no-placeholder".into(),
            }),
            output_mode: None,
        };
        let output = resolve_exec(&result, Some("ignored-input"));
        assert_eq!(
            output, "base-command",
            "should return base exec when template has no placeholder"
        );
    }

    // --- escape_for_single_quotes tests ---

    #[test]
    fn escape_for_single_quotes_simple_text() {
        assert_eq!(escape_for_single_quotes("hello world"), "hello world");
    }

    #[test]
    fn escape_for_single_quotes_with_single_quote() {
        assert_eq!(escape_for_single_quotes("it's"), "it'\\''s");
    }

    #[test]
    fn escape_for_single_quotes_preserves_other_chars() {
        // Only single quotes need escaping inside single-quoted strings
        let input = "\"$HOME`whoami`;rm -rf /|cat&&echo>file";
        assert_eq!(escape_for_single_quotes(input), input);
    }

    #[test]
    fn escape_for_single_quotes_multiple_quotes() {
        assert_eq!(escape_for_single_quotes("'a'b'"), "'\\''a'\\''b'\\''");
    }
}
