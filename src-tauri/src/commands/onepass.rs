use crate::router::SearchResult;
use std::process::Command;

/// Parse 1Password JSON output and filter by query.
pub fn parse_op_items(json_bytes: &[u8], query: &str) -> Result<Vec<SearchResult>, String> {
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(json_bytes).map_err(|e| e.to_string())?;

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

    parse_op_items(&output.stdout, query)
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
        // Items without "title" should be filtered out (title match returns false)
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
        assert_eq!(results[0].description, "SECURE_NOTE");
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
            .map(|i| format!(r#"{{"id": "id{}", "title": "Item {}", "category": "LOGIN"}}"#, i, i))
            .collect();
        let json = format!("[{}]", items.join(","));
        let results = parse_op_items(json.as_bytes(), "item").unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn empty_query_filter_returns_all_matching() {
        // parse_op_items with empty string query matches nothing (substring match on empty = all)
        let json = br#"[{"id": "x", "title": "Test", "category": "LOGIN"}]"#;
        let results = parse_op_items(json, "").unwrap();
        // Empty query substring matches everything
        assert_eq!(results.len(), 1);
    }
}
