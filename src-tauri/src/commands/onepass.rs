use crate::commands::onepass_vault;
use crate::router::SearchResult;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use zeroize::Zeroizing;

/// Per-account session tokens (account_id → session token).
static OP_SESSIONS: Mutex<Option<HashMap<String, Zeroizing<String>>>> = Mutex::new(None);

fn get_session(account_id: &str) -> Option<String> {
    OP_SESSIONS
        .lock()
        .ok()?
        .as_ref()?
        .get(account_id)
        .map(|z| z.to_string())
}

fn set_session(account_id: &str, token: String) {
    if let Ok(mut guard) = OP_SESSIONS.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(account_id.to_string(), Zeroizing::new(token));
    }
}

fn clear_session(account_id: &str) {
    if let Ok(mut guard) = OP_SESSIONS.lock() {
        if let Some(map) = guard.as_mut() {
            map.remove(account_id);
        }
    }
}

#[cfg(test)]
fn clear_all_sessions() {
    if let Ok(mut guard) = OP_SESSIONS.lock() {
        *guard = None;
    }
}

/// Sign in to a specific 1Password account.
fn signin(account_id: &str) -> Result<String, String> {
    eprintln!("[1pass] signing in to account {account_id}...");
    let output = Command::new("op")
        .args(["signin", "--account", account_id, "--raw"])
        .stdin(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("Failed to run op signin: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "op signin failed for {account_id}: {}",
            stderr.trim()
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err(format!("op signin returned empty token for {account_id}"));
    }
    set_session(account_id, token.clone());
    eprintln!("[1pass] session token acquired for {account_id}");
    Ok(token)
}

/// Run `op` with the given args and a `--session` flag.
fn run_op_once(args: &[&str], token: &str) -> Result<std::process::Output, String> {
    let session_flag = format!("--session={token}");
    let mut cmd_args: Vec<&str> = args.to_vec();
    cmd_args.push(&session_flag);

    Command::new("op")
        .args(&cmd_args)
        .output()
        .map_err(|e| format!("Failed to run op: {e}"))
}

/// Run an `op` command with the cached session for a specific account.
/// Retries with a fresh signin if the cached session is missing or expired.
fn run_op_with_session(account_id: &str, args: &[&str]) -> Result<std::process::Output, String> {
    if let Some(token) = get_session(account_id) {
        let output = run_op_once(args, &token)?;
        if output.status.success() {
            return Ok(output);
        }
        eprintln!("[1pass] session expired for {account_id}, re-authenticating...");
        clear_session(account_id);
    }

    let token = signin(account_id)?;
    let output = run_op_once(args, &token)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op command failed: {}", stderr.trim()));
    }
    Ok(output)
}

/// Fetch all account IDs via `op account list`.
fn fetch_account_ids() -> Result<Vec<String>, String> {
    eprintln!("[1pass] fetching account list...");
    // account list doesn't need a session
    let output = Command::new("op")
        .args(["account", "list", "--format=json"])
        .output()
        .map_err(|e| format!("Failed to run op account list: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op account list failed: {}", stderr.trim()));
    }

    let accounts: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;

    let ids: Vec<String> = accounts
        .iter()
        .filter_map(|a| {
            a.get("account_uuid")
                .or_else(|| a.get("user_uuid"))
                .and_then(|id| id.as_str())
                .map(String::from)
        })
        .collect();
    eprintln!("[1pass] found {} account(s)", ids.len());
    Ok(ids)
}

/// Fetch icon for a domain via DuckDuckGo icon proxy, returning base64 data URI.
fn fetch_icon_for_domain(domain: &str) -> Option<String> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("burrow/1password-icons");

    // Check cache first
    let cache_file = cache_dir.join(format!("{}.png", domain.replace('/', "_")));
    if let Ok(bytes) = std::fs::read(&cache_file) {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        return Some(format!("data:image/x-icon;base64,{b64}"));
    }

    // Fetch from DuckDuckGo
    let url = format!("https://icons.duckduckgo.com/ip3/{domain}.ico");
    let resp = reqwest::blocking::get(&url).ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().ok()?;
    if bytes.is_empty() || bytes.len() < 100 {
        return None;
    }

    // Cache to disk
    std::fs::create_dir_all(&cache_dir).ok();
    std::fs::write(&cache_file, &bytes).ok();

    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    Some(format!("data:image/x-icon;base64,{b64}"))
}

/// Extract primary domain from a 1Password item's URLs array.
fn extract_domain(item: &serde_json::Value) -> Option<String> {
    item.get("urls")?
        .as_array()?
        .first()?
        .get("href")
        .and_then(|h| h.as_str())
        .and_then(|href| url::Url::parse(href).ok())
        .and_then(|u| u.host_str().map(String::from))
}

/// Extract a field value from 1Password item JSON by field label or id.
fn extract_field(item: &serde_json::Value, field_name: &str) -> Option<String> {
    // Check fields array
    if let Some(fields) = item.get("fields").and_then(|f| f.as_array()) {
        for f in fields {
            let label = f.get("label").and_then(|l| l.as_str()).unwrap_or("");
            let id = f.get("id").and_then(|l| l.as_str()).unwrap_or("");
            let purpose = f.get("purpose").and_then(|p| p.as_str()).unwrap_or("");
            if label.eq_ignore_ascii_case(field_name)
                || id.eq_ignore_ascii_case(field_name)
                || purpose.eq_ignore_ascii_case(field_name)
            {
                return f.get("value").and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    None
}

/// Load all 1Password items into the in-memory vault.
/// This signs into each account, fetches item details with secrets, and caches icons.
pub fn load_vault() -> Result<String, String> {
    if crate::actions::dry_run::is_enabled() {
        eprintln!("[dry-run] load_vault");
        return Ok("dry-run: vault load skipped".into());
    }

    let timeout_minutes = crate::config::get_config().onepass.idle_timeout_minutes;
    let timeout = if timeout_minutes == 0 {
        Duration::from_secs(u64::MAX / 2) // effectively never
    } else {
        Duration::from_secs(timeout_minutes as u64 * 60)
    };

    let account_ids = fetch_account_ids()?;
    if account_ids.is_empty() {
        return Err("No 1Password accounts found. Is `op` CLI configured?".into());
    }

    // Sign into each account
    for account_id in &account_ids {
        if get_session(account_id).is_none() {
            signin(account_id)?;
        }
    }

    // Fetch item list for each account
    let mut all_list_items: Vec<(String, serde_json::Value)> = Vec::new();
    let mut failed_accounts: Vec<String> = Vec::new();
    for account_id in &account_ids {
        eprintln!("[1pass] fetching items for account {account_id}...");
        let output = match run_op_with_session(
            account_id,
            &["item", "list", "--account", account_id, "--format=json"],
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[1pass] failed to list items for {account_id}: {e}");
                failed_accounts.push(account_id.clone());
                continue;
            }
        };

        match serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout) {
            Ok(items) => {
                eprintln!("[1pass] account {account_id}: {} items", items.len());
                for item in items {
                    all_list_items.push((account_id.clone(), item));
                }
            }
            Err(e) => {
                eprintln!("[1pass] failed to parse items for {account_id}: {e}");
                failed_accounts.push(account_id.clone());
            }
        }
    }

    eprintln!(
        "[1pass] fetching details for {} items...",
        all_list_items.len()
    );

    // Fetch full item details (with --reveal for secrets)
    let mut vault_items: Vec<onepass_vault::VaultItemInput> = Vec::new();

    for (account_id, list_item) in &all_list_items {
        let item_id = match list_item.get("id").and_then(|i| i.as_str()) {
            Some(id) => id,
            None => {
                eprintln!("[1pass] skipping item with no id field");
                continue;
            }
        };

        let detail_output = match run_op_with_session(
            account_id,
            &[
                "item",
                "get",
                item_id,
                "--account",
                account_id,
                "--format=json",
                "--reveal",
            ],
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[1pass] failed to get item {item_id}: {e}");
                continue;
            }
        };

        let detail: serde_json::Value = match serde_json::from_slice(&detail_output.stdout) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[1pass] failed to parse item {item_id}: {e}");
                continue;
            }
        };

        let title = detail
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let category = detail
            .get("category")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let username = extract_field(&detail, "username").unwrap_or_default();
        let password = extract_field(&detail, "password").unwrap_or_default();

        // Fetch icon
        let icon_b64 = extract_domain(&detail)
            .and_then(|domain| fetch_icon_for_domain(&domain))
            .unwrap_or_default();

        vault_items.push(onepass_vault::VaultItemInput {
            id: item_id.to_string(),
            title,
            category,
            icon_b64,
            account_id: account_id.clone(),
            username,
            password,
        });
    }

    let count = vault_items.len();
    onepass_vault::store_items(vault_items, timeout);
    eprintln!("[1pass] vault loaded with {count} items");
    if failed_accounts.is_empty() {
        Ok(format!("Loaded {count} 1Password items"))
    } else {
        Ok(format!(
            "Loaded {count} 1Password items ({} account(s) failed: {})",
            failed_accounts.len(),
            failed_accounts.join(", ")
        ))
    }
}

/// Search 1Password items. Returns vault results if loaded, or a "Load" action.
pub async fn search_onepass(query: &str) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    if crate::actions::dry_run::is_enabled() {
        eprintln!("[dry-run] search_onepass: {query}");
        return Ok(vec![]);
    }

    if onepass_vault::is_vault_loaded() {
        let results = onepass_vault::search_to_results(query);
        Ok(results)
    } else {
        Ok(vec![SearchResult {
            id: "op-load-vault".into(),
            name: "Load 1Password Data".into(),
            description: "Sign in and load all vault items into memory".into(),
            icon: "".into(),
            category: "onepass".into(),
            exec: "op-load-vault".into(),
        }])
    }
}

/// Fetch the password for a 1Password item from the vault.
pub fn get_password(item_id: &str) -> Result<String, String> {
    onepass_vault::get_password(item_id)
}

/// Fetch the username for a 1Password item from the vault.
pub fn get_username(item_id: &str) -> Result<String, String> {
    onepass_vault::get_username(item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serialize all tests that touch global state (sessions + vault)
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn session_set_get_clear() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_sessions();
        assert!(get_session("acc-1").is_none());
        set_session("acc-1", "token-a".into());
        assert_eq!(get_session("acc-1").unwrap(), "token-a");
        clear_session("acc-1");
        assert!(get_session("acc-1").is_none());
        clear_all_sessions();
    }

    #[test]
    fn per_account_sessions() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_sessions();
        set_session("acc-1", "token-a".into());
        set_session("acc-2", "token-b".into());
        assert_eq!(get_session("acc-1").unwrap(), "token-a");
        assert_eq!(get_session("acc-2").unwrap(), "token-b");
        clear_session("acc-1");
        assert!(get_session("acc-1").is_none());
        assert_eq!(get_session("acc-2").unwrap(), "token-b");
        clear_all_sessions();
    }

    #[test]
    fn session_overwrite() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_sessions();
        set_session("acc-1", "first".into());
        set_session("acc-1", "second".into());
        assert_eq!(get_session("acc-1").unwrap(), "second");
        clear_all_sessions();
    }

    #[test]
    fn extract_field_from_item_json() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{
                "fields": [
                    {"label": "username", "value": "myuser"},
                    {"label": "password", "purpose": "PASSWORD", "value": "mypass"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(extract_field(&item, "username"), Some("myuser".into()));
        assert_eq!(extract_field(&item, "password"), Some("mypass".into()));
        assert_eq!(extract_field(&item, "nonexistent"), None);
    }

    #[test]
    fn extract_field_by_purpose() {
        let item: serde_json::Value = serde_json::from_str(
            r#"{
                "fields": [
                    {"label": "pw", "purpose": "PASSWORD", "value": "secret123"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(extract_field(&item, "PASSWORD"), Some("secret123".into()));
    }

    #[test]
    fn extract_domain_from_urls() {
        let item: serde_json::Value =
            serde_json::from_str(r#"{"urls": [{"href": "https://github.com/login"}]}"#).unwrap();
        assert_eq!(extract_domain(&item), Some("github.com".into()));
    }

    #[test]
    fn extract_domain_no_urls() {
        let item: serde_json::Value = serde_json::from_str(r#"{"title": "test"}"#).unwrap();
        assert_eq!(extract_domain(&item), None);
    }

    #[test]
    fn empty_query_returns_empty() {
        let results = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(search_onepass(""))
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn get_password_delegates_to_vault() {
        // Just verify delegation works — vault tests cover the actual logic
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        onepass_vault::clear_vault();
        assert!(get_password("nonexistent").is_err());
        onepass_vault::clear_vault();
    }

    #[test]
    fn get_username_delegates_to_vault() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        onepass_vault::clear_vault();
        assert!(get_username("nonexistent").is_err());
        onepass_vault::clear_vault();
    }
}
