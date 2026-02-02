use crate::router::SearchResult;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// In-memory 1Password item cache.
struct OpCache {
    items: Vec<serde_json::Value>,
    fetched_at: Instant,
}

static OP_CACHE: Mutex<Option<OpCache>> = Mutex::new(None);
static REFRESH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
/// Cached `op signin --raw` session token. Kept until 1Password rejects it.
static OP_SESSION: Mutex<Option<String>> = Mutex::new(None);

const CACHE_TTL_SECS: u64 = 300; // 5 minutes

/// Get the cached session token, if any.
fn get_session() -> Option<String> {
    OP_SESSION.lock().ok()?.clone()
}

/// Store a new session token.
fn set_session(token: String) {
    if let Ok(mut s) = OP_SESSION.lock() {
        *s = Some(token);
    }
}

/// Clear the cached session token (called when op rejects it).
fn clear_session() {
    if let Ok(mut s) = OP_SESSION.lock() {
        *s = None;
    }
}

/// Acquire a fresh session token via `op signin --raw`.
/// This is the only call that prompts the user for their password.
fn signin() -> Result<String, String> {
    eprintln!("[1pass] signing in (requesting session token)...");
    let output = Command::new("op")
        .args(["signin", "--raw"])
        .stdin(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("Failed to run op signin: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op signin failed: {}", stderr.trim()));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("op signin returned empty token".into());
    }
    set_session(token.clone());
    eprintln!("[1pass] session token acquired");
    Ok(token)
}

/// Run an `op` command with the cached session token.
/// If the command fails and we had a token, clear it and retry once (which will prompt signin).
fn run_op_with_session(args: &[&str]) -> Result<std::process::Output, String> {
    // First attempt: with cached session if available
    if let Some(token) = get_session() {
        let mut cmd_args: Vec<&str> = args.to_vec();
        let session_flag = format!("--session={token}");
        cmd_args.push(&session_flag);

        let output = Command::new("op")
            .args(&cmd_args)
            .output()
            .map_err(|e| format!("Failed to run op: {e}"))?;

        if output.status.success() {
            return Ok(output);
        }

        // Token rejected — clear and fall through to retry with fresh signin
        eprintln!("[1pass] session token expired, re-authenticating...");
        clear_session();
    }

    // Second attempt: get fresh token then retry
    let token = signin()?;
    let mut cmd_args: Vec<&str> = args.to_vec();
    let session_flag = format!("--session={token}");
    cmd_args.push(&session_flag);

    let output = Command::new("op")
        .args(&cmd_args)
        .output()
        .map_err(|e| format!("Failed to run op: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op command failed: {}", stderr.trim()));
    }

    Ok(output)
}

fn disk_cache_path() -> std::path::PathBuf {
    super::data_dir().join("op_items.json")
}

fn load_from_disk() -> Option<Vec<serde_json::Value>> {
    let path = disk_cache_path();
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(items) => Some(items),
        Err(e) => {
            eprintln!("[1pass] failed to parse disk cache: {e}");
            None
        }
    }
}

fn save_to_disk(items: &[serde_json::Value]) {
    let dir = super::data_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[1pass] failed to create data dir {}: {e}", dir.display());
        return;
    }
    match serde_json::to_vec(items) {
        Ok(json) => {
            if let Err(e) = std::fs::write(disk_cache_path(), json) {
                eprintln!("[1pass] failed to write disk cache: {e}");
            }
        }
        Err(e) => eprintln!("[1pass] failed to serialize cache: {e}"),
    }
}

/// Fetch all account user IDs via `op account list`.
fn fetch_account_ids() -> Result<Vec<String>, String> {
    eprintln!("[1pass] fetching account list...");
    let output = run_op_with_session(&["account", "list", "--format=json"])?;

    let accounts: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;

    let ids: Vec<String> = accounts
        .iter()
        .filter_map(|a| {
            a.get("user_uuid")
                .and_then(|id| id.as_str())
                .map(String::from)
        })
        .collect();
    eprintln!("[1pass] found {} account(s)", ids.len());
    Ok(ids)
}

/// Fetch items from all accounts, iterating each account with `--account`.
fn fetch_op_items() -> Result<Vec<serde_json::Value>, String> {
    let account_ids = fetch_account_ids()?;
    let mut all_items = Vec::new();

    for account_id in &account_ids {
        eprintln!("[1pass] fetching items for account {account_id}...");
        let output = match run_op_with_session(&[
            "item",
            "list",
            "--account",
            account_id,
            "--format=json",
        ]) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[1pass] failed to list items for account {account_id}: {e}");
                continue;
            }
        };

        match serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout) {
            Ok(items) => {
                eprintln!("[1pass] account {account_id}: {} items", items.len());
                all_items.extend(items);
            }
            Err(e) => {
                eprintln!("[1pass] failed to parse items for account {account_id}: {e}")
            }
        }
    }

    if all_items.is_empty() && !account_ids.is_empty() {
        return Err("No items found across any account".into());
    }

    eprintln!(
        "[1pass] total: {} items across all accounts",
        all_items.len()
    );
    Ok(all_items)
}

fn refresh_cache() -> Result<Vec<serde_json::Value>, String> {
    let items = fetch_op_items()?;
    save_to_disk(&items);
    let mut cache = OP_CACHE.lock().map_err(|e| e.to_string())?;
    *cache = Some(OpCache {
        items: items.clone(),
        fetched_at: Instant::now(),
    });
    Ok(items)
}

fn get_cached_items() -> Result<(Vec<serde_json::Value>, bool), String> {
    let cache = OP_CACHE.lock().map_err(|e| e.to_string())?;
    if let Some(ref c) = *cache {
        let stale = c.fetched_at.elapsed().as_secs() > CACHE_TTL_SECS;
        return Ok((c.items.clone(), stale));
    }
    drop(cache);

    // Try disk cache
    if let Some(items) = load_from_disk() {
        let mut cache = OP_CACHE.lock().map_err(|e| e.to_string())?;
        *cache = Some(OpCache {
            items: items.clone(),
            fetched_at: Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1),
        });
        return Ok((items, true)); // Disk cache loses fetch timestamp; backdated past TTL to trigger background refresh
    }

    Err("no_cache".into())
}

/// Filter cached items by query and return SearchResults.
pub fn filter_items(items: &[serde_json::Value], query: &str) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();

    items
        .iter()
        .filter(|item| {
            item.get("title")
                .and_then(|t| t.as_str())
                .map(|t| t.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
        })
        .take(10)
        .map(|item| {
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let id = item
                .get("id")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let category = item
                .get("category")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            SearchResult {
                id: format!("op-{id}"),
                name: title,
                description: format!("{category} · ⏎ type pw · ⇧ copy pw · ^C copy user"),
                icon: "".into(),
                category: "onepass".into(),
                exec: format!("op item get {id} --otp 2>/dev/null || op open op://vault/{id}"),
            }
        })
        .collect()
}

/// Parse 1Password JSON output and filter by query.
pub fn parse_op_items(json_bytes: &[u8], query: &str) -> Result<Vec<SearchResult>, String> {
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(json_bytes).map_err(|e| e.to_string())?;
    Ok(filter_items(&items, query))
}

/// Extract the 1Password item ID from an exec string like "op item get {id} --otp ...".
pub fn extract_item_id(exec: &str) -> Option<String> {
    let prefix = "op item get ";
    if let Some(start) = exec.find(prefix) {
        let rest = &exec[start + prefix.len()..];
        let id = rest.split_whitespace().next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

/// Fetch a field value from a 1Password item.
fn get_field(item_id: &str, field: &str, extra_args: &[&str]) -> Result<String, String> {
    if crate::actions::dry_run::is_enabled() {
        eprintln!("[dry-run] op get_field: {item_id} {field}");
        return Err("dry-run: 1Password field lookup skipped".into());
    }
    let mut args = vec!["item", "get", item_id, "--fields", field];
    args.extend_from_slice(extra_args);
    let output = run_op_with_session(&args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Fetch the password for a 1Password item.
pub fn get_password(item_id: &str) -> Result<String, String> {
    get_field(item_id, "password", &["--reveal"])
}

/// Fetch the username for a 1Password item.
pub fn get_username(item_id: &str) -> Result<String, String> {
    get_field(item_id, "username", &[])
}

/// Trigger background cache population. Call at app startup so the first `!` query is instant.
pub fn preload_cache() {
    if crate::actions::dry_run::is_enabled() {
        return;
    }
    // If disk cache exists, load it into memory
    if let Some(items) = load_from_disk() {
        eprintln!("[1pass] preloaded {} items from disk cache", items.len());
        if let Ok(mut cache) = OP_CACHE.lock() {
            *cache = Some(OpCache {
                items,
                fetched_at: Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1),
            });
        }
    }
    // Then kick off a background refresh regardless
    spawn_background_refresh();
}

fn spawn_background_refresh() {
    if !REFRESH_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            struct ResetGuard;
            impl Drop for ResetGuard {
                fn drop(&mut self) {
                    REFRESH_IN_PROGRESS.store(false, Ordering::SeqCst);
                }
            }
            let _guard = ResetGuard;
            if let Err(e) = refresh_cache() {
                eprintln!("[1pass] background refresh failed: {e}");
            }
        });
    }
}

pub async fn search_onepass(query: &str) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    if crate::actions::dry_run::is_enabled() {
        eprintln!("[dry-run] search_onepass: {query}");
        return Ok(vec![]);
    }

    match get_cached_items() {
        Ok((items, stale)) => {
            let mut results = filter_items(&items, query);
            if stale {
                // Only show the refresh indicator if no background refresh is running,
                // since the user can't dismiss it while a refresh is in progress.
                if !REFRESH_IN_PROGRESS.load(Ordering::SeqCst) {
                    results.insert(
                        0,
                        SearchResult {
                            id: "op-refresh".into(),
                            name: "Update 1Password items".into(),
                            description: "Cache may be outdated — select to refresh".into(),
                            icon: "".into(),
                            category: "onepass".into(),
                            exec: "op-refresh-cache".into(),
                        },
                    );
                }
                spawn_background_refresh();
            }
            Ok(results)
        }
        Err(_) => {
            // No cache yet — show loading indicator and trigger fetch
            spawn_background_refresh();
            Ok(vec![SearchResult {
                id: "op-loading".into(),
                name: "Loading 1Password items...".into(),
                description: "Fetching from all vaults — results will appear shortly".into(),
                icon: "".into(),
                category: "onepass".into(),
                exec: "".into(),
            }])
        }
    }
}

/// Force refresh the 1Password cache. Called when user selects "Update 1Password items".
pub fn refresh_op_cache() -> Result<String, String> {
    if crate::actions::dry_run::is_enabled() {
        eprintln!("[dry-run] refresh_op_cache");
        return Ok("dry-run: 1Password cache refresh skipped".into());
    }
    refresh_cache()?;
    Ok("1Password items updated".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json() {
        let json = br#"[
            {"id": "abc123", "title": "GitHub", "category": "LOGIN"},
            {"id": "def456", "title": "AWS Console", "category": "LOGIN"}
        ]"#;
        let results = parse_op_items(json, "github").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "GitHub");
        assert_eq!(results[0].id, "op-abc123");
    }

    #[test]
    fn parse_empty_array() {
        let json = b"[]";
        let results = parse_op_items(json, "anything").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn parse_invalid_json() {
        let json = b"not json at all";
        let result = parse_op_items(json, "query");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_fields() {
        let json = br#"[
            {"id": "abc"},
            {"title": "Has Title", "id": "def"}
        ]"#;
        let results = parse_op_items(json, "title").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Has Title");
    }

    #[test]
    fn result_category_is_onepass() {
        let json = br#"[{"id": "x", "title": "Test", "category": "LOGIN"}]"#;
        let results = parse_op_items(json, "test").unwrap();
        assert_eq!(results[0].category, "onepass");
    }

    #[test]
    fn result_exec_format() {
        let json = br#"[{"id": "myid", "title": "Test", "category": "LOGIN"}]"#;
        let results = parse_op_items(json, "test").unwrap();
        assert!(results[0].exec.contains("op item get myid"));
        assert!(results[0].exec.contains("op open op://vault/myid"));
    }

    #[test]
    fn description_is_op_category() {
        let json = br#"[{"id": "x", "title": "Test", "category": "SECURE_NOTE"}]"#;
        let results = parse_op_items(json, "test").unwrap();
        assert!(results[0].description.starts_with("SECURE_NOTE"));
    }

    #[test]
    fn case_insensitive_filter() {
        let json = br#"[{"id": "x", "title": "MyGitHub", "category": "LOGIN"}]"#;
        let results = parse_op_items(json, "MYGITHUB").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn limits_to_10_results() {
        let items: Vec<String> = (0..20)
            .map(|i| {
                format!(
                    r#"{{"id": "id{}", "title": "Item {}", "category": "LOGIN"}}"#,
                    i, i
                )
            })
            .collect();
        let json = format!("[{}]", items.join(","));
        let results = parse_op_items(json.as_bytes(), "item").unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn extract_item_id_from_exec() {
        let exec = "op item get abc123 --otp 2>/dev/null || op open op://vault/abc123";
        assert_eq!(extract_item_id(exec), Some("abc123".to_string()));
    }

    #[test]
    fn extract_item_id_missing() {
        assert_eq!(extract_item_id("something else"), None);
    }

    #[test]
    fn extract_item_id_empty_exec() {
        assert_eq!(extract_item_id(""), None);
    }

    #[test]
    fn empty_query_filter_returns_all_matching() {
        let json = br#"[{"id": "x", "title": "Test", "category": "LOGIN"}]"#;
        let results = parse_op_items(json, "").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn filter_items_from_values() {
        let items: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"id": "a", "title": "GitHub", "category": "LOGIN"}, {"id": "b", "title": "Slack", "category": "LOGIN"}]"#,
        ).unwrap();
        let results = filter_items(&items, "git");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "GitHub");
    }

    #[test]
    fn filter_items_empty_query_matches_all() {
        let items: Vec<serde_json::Value> =
            serde_json::from_str(r#"[{"id": "a", "title": "GitHub", "category": "LOGIN"}]"#)
                .unwrap();
        let results = filter_items(&items, "");
        assert_eq!(results.len(), 1);
    }

    // Guards env-var manipulation against parallel test execution
    static DISK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn disk_cache_roundtrip() {
        let _guard = DISK_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("BURROW_DATA_DIR", dir.path());
        }
        let items: Vec<serde_json::Value> =
            serde_json::from_str(r#"[{"id": "x", "title": "Test", "category": "LOGIN"}]"#).unwrap();
        save_to_disk(&items);
        let loaded = load_from_disk().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["title"], "Test");
        unsafe {
            std::env::remove_var("BURROW_DATA_DIR");
        }
    }

    // Single test to avoid parallel-test races on the global OP_SESSION
    #[test]
    fn session_set_get_clear_and_overwrite() {
        clear_session();
        assert!(get_session().is_none());

        set_session("test-token-123".into());
        assert_eq!(get_session().unwrap(), "test-token-123");

        clear_session();
        assert!(get_session().is_none());

        // Overwrite
        set_session("first".into());
        set_session("second".into());
        assert_eq!(get_session().unwrap(), "second");
        clear_session();
    }
}
