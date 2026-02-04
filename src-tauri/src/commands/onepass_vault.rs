use std::sync::Mutex;
use std::time::{Duration, Instant};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::router::{Category, SearchResult};

#[derive(Zeroize, ZeroizeOnDrop)]
struct SecretFields {
    username: String,
    password: String,
}

/// A single 1Password item with metadata and zeroize-protected secrets.
struct VaultItem {
    id: String,
    title: String,
    category: String,
    icon_b64: String,
    account_id: String,
    secrets: SecretFields,
}

struct Vault {
    items: Vec<VaultItem>,
    #[allow(dead_code)]
    loaded_at: Instant,
    last_access: Instant,
    timeout: Duration,
}

static VAULT: Mutex<Option<Vault>> = Mutex::new(None);

/// Check whether the vault is loaded and not expired.
/// Also refreshes the last_access timestamp to prevent TOCTOU race conditions.
pub fn is_vault_loaded() -> bool {
    let mut guard = match VAULT.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("vault mutex poisoned in is_vault_loaded, recovering");
            poisoned.into_inner()
        }
    };
    match guard.as_mut() {
        Some(v) => {
            if v.last_access.elapsed() < v.timeout {
                // Refresh last_access to prevent TOCTOU race
                v.last_access = Instant::now();
                true
            } else {
                false
            }
        }
        None => false,
    }
}

/// Store items in the vault, replacing any existing contents.
/// Recovers from poisoned mutex to ensure items are always stored.
pub fn store_items(items: Vec<VaultItemInput>, timeout: Duration) {
    let vault_items: Vec<VaultItem> = items
        .into_iter()
        .map(|i| VaultItem {
            id: i.id,
            title: i.title,
            category: i.category,
            icon_b64: i.icon_b64,
            account_id: i.account_id,
            secrets: SecretFields {
                username: i.username,
                password: i.password,
            },
        })
        .collect();

    let item_count = vault_items.len();
    let now = Instant::now();
    let mut guard = match VAULT.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("vault mutex poisoned in store_items, recovering");
            poisoned.into_inner()
        }
    };
    *guard = Some(Vault {
        items: vault_items,
        loaded_at: now,
        last_access: now,
        timeout,
    });
    tracing::debug!(item_count, "vault items stored");
}

/// Input struct for populating the vault (no zeroize needed — caller builds and hands off).
pub struct VaultItemInput {
    pub id: String,
    pub title: String,
    pub category: String,
    pub icon_b64: String,
    pub account_id: String,
    pub username: String,
    pub password: String,
}

/// Access the vault with expiry check and last-access touch.
/// Returns `Err` if the vault is not loaded or expired.
/// Recovers from poisoned mutex gracefully.
/// The callback receives the live vault reference.
fn with_vault<T>(f: impl FnOnce(&Vault) -> Result<T, String>) -> Result<T, String> {
    let mut guard = match VAULT.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("vault mutex poisoned in with_vault, recovering");
            poisoned.into_inner()
        }
    };
    let vault = guard.as_mut().ok_or("Vault not loaded")?;

    if vault.last_access.elapsed() >= vault.timeout {
        *guard = None;
        return Err("Vault expired".into());
    }
    vault.last_access = Instant::now();

    f(vault)
}

/// Find a vault item by ID, returning a mapped value via the provided extractor.
fn get_item_field(
    id: &str,
    extractor: impl FnOnce(&VaultItem) -> String,
) -> Result<String, String> {
    with_vault(|vault| {
        vault
            .items
            .iter()
            .find(|item| item.id == id)
            .map(extractor)
            .ok_or_else(|| format!("Item {id} not found in vault"))
    })
}

/// Retrieve the password for a vault item by ID.
/// Returns `Zeroizing<String>` so the secret is zeroed when the caller drops it.
pub fn get_password(id: &str) -> Result<Zeroizing<String>, String> {
    get_item_field(id, |item| item.secrets.password.clone()).map(Zeroizing::new)
}

/// Retrieve the username for a vault item by ID.
/// Returns `Zeroizing<String>` so the secret is zeroed when the caller drops it.
pub fn get_username(id: &str) -> Result<Zeroizing<String>, String> {
    get_item_field(id, |item| item.secrets.username.clone()).map(Zeroizing::new)
}

/// Non-secret metadata returned by search.
pub struct ItemMeta {
    pub id: String,
    pub title: String,
    pub category: String,
    pub icon_b64: String,
    pub account_id: String,
}

/// Search the vault by title substring, returning non-secret metadata.
pub fn search_vault(query: &str) -> Vec<ItemMeta> {
    with_vault(|vault| {
        let query_lower = query.to_lowercase();
        Ok(vault
            .items
            .iter()
            .filter(|item| item.title.to_lowercase().contains(&query_lower))
            .take(10)
            .map(|item| ItemMeta {
                id: item.id.clone(),
                title: item.title.clone(),
                category: item.category.clone(),
                icon_b64: item.icon_b64.clone(),
                account_id: item.account_id.clone(),
            })
            .collect())
    })
    .unwrap_or_default()
}

/// Convert vault search results to SearchResult format.
pub fn search_to_results(query: &str) -> Vec<SearchResult> {
    search_vault(query)
        .into_iter()
        .map(|m| SearchResult {
            id: format!("op-{}", m.id),
            name: m.title,
            description: format!("{} · ⏎ type pw · ⇧ copy pw · ^C copy user", m.category),
            icon: m.icon_b64,
            category: Category::Onepass,
            exec: format!("op-vault-item:{}", m.id),
            input_spec: None,
        })
        .collect()
}

/// Clear the vault, zeroizing all secrets on drop.
/// SECURITY: Always clears the vault, even if mutex is poisoned.
pub fn clear_vault() {
    let mut guard = match VAULT.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("vault mutex poisoned in clear_vault, recovering to clear secrets");
            poisoned.into_inner()
        }
    };
    *guard = None;
    tracing::debug!("vault cleared");
}

#[cfg(test)]
mod tests {
    use super::*;

    // All vault tests must hold this lock since they share global VAULT state
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_items(n: usize) -> Vec<VaultItemInput> {
        (0..n)
            .map(|i| VaultItemInput {
                id: format!("id-{i}"),
                title: format!("Item {i}"),
                category: "LOGIN".into(),
                icon_b64: String::new(),
                account_id: "acc-1".into(),
                username: format!("user{i}"),
                password: format!("pass{i}"),
            })
            .collect()
    }

    #[test]
    fn store_and_retrieve_password() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(3), Duration::from_secs(600));
        assert!(is_vault_loaded());
        assert_eq!(&*get_password("id-1").unwrap(), "pass1");
        assert_eq!(&*get_username("id-1").unwrap(), "user1");
        clear_vault();
    }

    #[test]
    fn not_found_returns_error() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(1), Duration::from_secs(600));
        assert!(get_password("nonexistent").is_err());
        clear_vault();
    }

    #[test]
    fn search_filters_by_title() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        let mut items = make_items(3);
        items[0].title = "GitHub".into();
        items[1].title = "GitLab".into();
        items[2].title = "Slack".into();
        store_items(items, Duration::from_secs(600));

        let results = search_vault("git");
        assert_eq!(results.len(), 2);
        clear_vault();
    }

    #[test]
    fn clear_vault_makes_unloaded() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(1), Duration::from_secs(600));
        assert!(is_vault_loaded());
        clear_vault();
        assert!(!is_vault_loaded());
    }

    #[test]
    fn timeout_expires_vault() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(1), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(10));
        assert!(!is_vault_loaded());
        assert!(get_password("id-0").is_err());
        clear_vault();
    }

    #[test]
    fn search_to_results_format() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        let mut items = make_items(1);
        items[0].title = "GitHub".into();
        store_items(items, Duration::from_secs(600));

        let results = search_to_results("git");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "op-id-0");
        assert_eq!(results[0].exec, "op-vault-item:id-0");
        assert_eq!(results[0].category, Category::Onepass);
        clear_vault();
    }

    #[test]
    fn empty_vault_not_loaded() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        assert!(!is_vault_loaded());
        assert!(get_password("any").is_err());
    }

    #[test]
    fn timeout_expires_all_accessors() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(2), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(10));
        assert!(get_username("id-0").is_err());
        assert!(search_vault("Item").is_empty());
        clear_vault();
    }

    #[test]
    fn search_vault_caps_at_10() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(15), Duration::from_secs(600));
        let results = search_vault("Item");
        assert_eq!(results.len(), 10, "search_vault should cap results at 10");
        clear_vault();
    }

    #[test]
    fn search_empty_query_returns_all() {
        let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_vault();
        store_items(make_items(3), Duration::from_secs(600));
        let results = search_vault("");
        assert_eq!(results.len(), 3);
        clear_vault();
    }
}
