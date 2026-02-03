use crate::commands::onepass_vault;
use crate::router::{Category, SearchResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use zeroize::Zeroizing;

/// Typed 1Password account from `op account list`.
/// Schema mismatches surface immediately as parse errors.
#[derive(Debug, Clone, Deserialize)]
pub struct OpAccount {
    /// Account UUID (preferred identifier)
    #[serde(alias = "user_uuid")]
    pub account_uuid: Option<String>,
}

impl OpAccount {
    /// Get the account ID, preferring account_uuid over user_uuid.
    pub fn id(&self) -> Option<&str> {
        self.account_uuid.as_deref()
    }
}

/// Typed 1Password item from `op item list`.
/// Contains only the fields needed for listing/identification.
#[derive(Debug, Clone, Deserialize)]
pub struct OpListItem {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub category: String,
}

/// URL entry in a 1Password item.
#[derive(Debug, Clone, Deserialize)]
pub struct OpUrl {
    pub href: Option<String>,
}

/// Field entry in a 1Password item (for extracting username/password).
#[derive(Debug, Clone, Deserialize)]
pub struct OpField {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub purpose: String,
    pub value: Option<String>,
}

/// Typed 1Password item detail from `op item get --reveal`.
/// Contains full item data including secrets.
#[derive(Debug, Clone, Deserialize)]
pub struct OpItemDetail {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub urls: Vec<OpUrl>,
    #[serde(default)]
    pub fields: Vec<OpField>,
}

impl OpItemDetail {
    /// Extract a field value by label, id, or purpose (case-insensitive).
    pub fn get_field(&self, field_name: &str) -> Option<&str> {
        self.fields.iter().find_map(|f| {
            if f.label.eq_ignore_ascii_case(field_name)
                || f.id.eq_ignore_ascii_case(field_name)
                || f.purpose.eq_ignore_ascii_case(field_name)
            {
                f.value.as_deref()
            } else {
                None
            }
        })
    }

    /// Extract the primary domain from the item's URLs.
    pub fn primary_domain(&self) -> Option<String> {
        self.urls
            .first()
            .and_then(|u| u.href.as_ref())
            .and_then(|href| url::Url::parse(href).ok())
            .and_then(|u| u.host_str().map(String::from))
    }
}

/// Per-account session tokens (account_id → session token).
static OP_SESSIONS: Mutex<Option<HashMap<String, Zeroizing<String>>>> = Mutex::new(None);

fn get_session(account_id: &str) -> Option<Zeroizing<String>> {
    OP_SESSIONS.lock().ok()?.as_ref()?.get(account_id).cloned()
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
/// Uses a 120-second timeout to allow for user interaction (password prompts).
fn signin(account_id: &str) -> Result<String, String> {
    use std::io::Read;
    use wait_timeout::ChildExt;

    eprintln!("[1pass] signing in to account {account_id}...");
    let mut child = Command::new("op")
        .args(["signin", "--account", account_id, "--raw"])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn op signin: {e}"))?;

    match child
        .wait_timeout(Duration::from_secs(120))
        .map_err(|e| e.to_string())?
    {
        Some(status) if status.success() => {
            let mut stdout = String::new();
            if let Some(mut out) = child.stdout.take() {
                out.read_to_string(&mut stdout).ok();
            }
            let token = stdout.trim().to_string();
            if token.is_empty() {
                return Err(format!("op signin returned empty token for {account_id}"));
            }
            set_session(account_id, token.clone());
            eprintln!("[1pass] session token acquired for {account_id}");
            Ok(token)
        }
        Some(_) => {
            let mut stderr = String::new();
            if let Some(mut err) = child.stderr.take() {
                err.read_to_string(&mut stderr).ok();
            }
            Err(format!(
                "op signin failed for {account_id}: {}",
                stderr.trim()
            ))
        }
        None => {
            child.kill().ok();
            child.wait().ok();
            Err(format!("op signin timed out for {account_id}"))
        }
    }
}

/// Run `op` with the given args and a `--session` flag.
/// Uses a 30-second timeout to prevent hanging on unresponsive CLI calls.
fn run_op_once(args: &[&str], token: &str) -> Result<std::process::Output, String> {
    use std::io::Read;
    use wait_timeout::ChildExt;

    let session_flag = format!("--session={token}");
    let mut cmd_args: Vec<&str> = args.to_vec();
    cmd_args.push(&session_flag);

    let mut child = Command::new("op")
        .args(&cmd_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn op: {e}"))?;

    match child
        .wait_timeout(Duration::from_secs(30))
        .map_err(|e| e.to_string())?
    {
        Some(status) => {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                out.read_to_end(&mut stdout).ok();
            }
            if let Some(mut err) = child.stderr.take() {
                err.read_to_end(&mut stderr).ok();
            }
            Ok(std::process::Output {
                status,
                stdout,
                stderr,
            })
        }
        None => {
            child.kill().ok();
            child.wait().ok();
            Err("op command timed out after 30s".into())
        }
    }
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
    use std::io::Read;
    use wait_timeout::ChildExt;

    eprintln!("[1pass] fetching account list...");
    let mut child = Command::new("op")
        .args(["account", "list", "--format=json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn op account list: {e}"))?;

    let status = match child
        .wait_timeout(Duration::from_secs(30))
        .map_err(|e| e.to_string())?
    {
        Some(s) => s,
        None => {
            child.kill().ok();
            child.wait().ok();
            return Err("op account list timed out after 30s".into());
        }
    };

    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_end(&mut stdout).ok();
    }

    if !status.success() {
        let mut stderr = Vec::new();
        if let Some(mut err) = child.stderr.take() {
            err.read_to_end(&mut stderr).ok();
        }
        return Err(format!(
            "op account list failed: {}",
            String::from_utf8_lossy(&stderr).trim()
        ));
    }

    let accounts: Vec<OpAccount> = serde_json::from_slice(&stdout)
        .map_err(|e| format!("Failed to parse account list: {e}"))?;

    let ids: Vec<String> = accounts
        .iter()
        .filter_map(|a| a.id().map(String::from))
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
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;
    let resp = client.get(&url).send().ok()?;
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
    let mut all_list_items: Vec<(String, OpListItem)> = Vec::new();
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

        match serde_json::from_slice::<Vec<OpListItem>>(&output.stdout) {
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
        let item_id = &list_item.id;

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

        let detail: OpItemDetail = match serde_json::from_slice(&detail_output.stdout) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[1pass] failed to parse item {item_id}: {e}");
                continue;
            }
        };

        let username = detail.get_field("username").unwrap_or_default().to_string();
        let password = detail.get_field("password").unwrap_or_default().to_string();

        // Fetch icon
        let icon_b64 = detail
            .primary_domain()
            .and_then(|domain| fetch_icon_for_domain(&domain))
            .unwrap_or_default();

        vault_items.push(onepass_vault::VaultItemInput {
            id: item_id.clone(),
            title: detail.title,
            category: detail.category,
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
            category: Category::Onepass,
            exec: "op-load-vault".into(),
        }])
    }
}

/// Fetch the password for a 1Password item from the vault.
pub fn get_password(item_id: &str) -> Result<Zeroizing<String>, String> {
    onepass_vault::get_password(item_id)
}

/// Fetch the username for a 1Password item from the vault.
pub fn get_username(item_id: &str) -> Result<Zeroizing<String>, String> {
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
        assert_eq!(&*get_session("acc-1").unwrap(), "token-a");
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
        assert_eq!(&*get_session("acc-1").unwrap(), "token-a");
        assert_eq!(&*get_session("acc-2").unwrap(), "token-b");
        clear_session("acc-1");
        assert!(get_session("acc-1").is_none());
        assert_eq!(&*get_session("acc-2").unwrap(), "token-b");
        clear_all_sessions();
    }

    #[test]
    fn session_overwrite() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_sessions();
        set_session("acc-1", "first".into());
        set_session("acc-1", "second".into());
        assert_eq!(&*get_session("acc-1").unwrap(), "second");
        clear_all_sessions();
    }

    #[test]
    fn op_item_detail_get_field() {
        let item: OpItemDetail = serde_json::from_str(
            r#"{
                "id": "test-id",
                "title": "Test",
                "category": "LOGIN",
                "urls": [],
                "fields": [
                    {"label": "username", "value": "myuser"},
                    {"label": "password", "purpose": "PASSWORD", "value": "mypass"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(item.get_field("username"), Some("myuser"));
        assert_eq!(item.get_field("password"), Some("mypass"));
        assert_eq!(item.get_field("nonexistent"), None);
    }

    #[test]
    fn op_item_detail_get_field_by_purpose() {
        let item: OpItemDetail = serde_json::from_str(
            r#"{
                "id": "test-id",
                "title": "Test",
                "fields": [
                    {"label": "pw", "purpose": "PASSWORD", "value": "secret123"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(item.get_field("PASSWORD"), Some("secret123"));
    }

    #[test]
    fn op_item_detail_primary_domain() {
        let item: OpItemDetail = serde_json::from_str(
            r#"{"id": "test", "urls": [{"href": "https://github.com/login"}]}"#,
        )
        .unwrap();
        assert_eq!(item.primary_domain(), Some("github.com".into()));
    }

    #[test]
    fn op_item_detail_no_urls() {
        let item: OpItemDetail =
            serde_json::from_str(r#"{"id": "test", "title": "test"}"#).unwrap();
        assert_eq!(item.primary_domain(), None);
    }

    #[test]
    fn op_account_id_parsing() {
        // Test account_uuid takes precedence
        let account: OpAccount = serde_json::from_str(r#"{"account_uuid": "uuid-123"}"#).unwrap();
        assert_eq!(account.id(), Some("uuid-123"));

        // Test user_uuid fallback via alias
        let account: OpAccount = serde_json::from_str(r#"{"user_uuid": "user-456"}"#).unwrap();
        assert_eq!(account.id(), Some("user-456"));

        // Test empty account
        let account: OpAccount = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(account.id(), None);
    }

    #[test]
    fn op_list_item_parsing() {
        let item: OpListItem =
            serde_json::from_str(r#"{"id": "item-1", "title": "GitHub", "category": "LOGIN"}"#)
                .unwrap();
        assert_eq!(item.id, "item-1");
        assert_eq!(item.title, "GitHub");
        assert_eq!(item.category, "LOGIN");

        // Test with missing optional fields (should use defaults)
        let item: OpListItem = serde_json::from_str(r#"{"id": "item-2"}"#).unwrap();
        assert_eq!(item.id, "item-2");
        assert_eq!(item.title, "");
        assert_eq!(item.category, "");
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
