use crate::router::SearchResult;
use std::process::Command;

pub async fn search_onepass(query: &str) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let output = Command::new("op")
        .args(["item", "list", "--format=json"])
        .output()
        .map_err(|e| format!("Failed to run op CLI: {e}"))?;

    if !output.status.success() {
        return Err("1Password CLI not available or not signed in".into());
    }

    let items: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;

    let query_lower = query.to_lowercase();

    let results = items
        .into_iter()
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
                description: category,
                icon: "".into(),
                category: "onepass".into(),
                exec: format!("op item get {id} --otp 2>/dev/null || op open op://vault/{id}"),
            }
        })
        .collect();

    Ok(results)
}
