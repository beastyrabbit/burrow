use crate::router::SearchResult;

struct SettingDef {
    id: &'static str,
    name: &'static str,
    description: &'static str,
}

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        id: "reindex",
        name: ":reindex",
        description: "Reindex all files (full rebuild)",
    },
    SettingDef {
        id: "update",
        name: ":update",
        description: "Update index (incremental)",
    },
    SettingDef {
        id: "config",
        name: ":config",
        description: "Open config file",
    },
    SettingDef {
        id: "stats",
        name: ":stats",
        description: "Index statistics",
    },
    SettingDef {
        id: "progress",
        name: ":progress",
        description: "Show indexer progress",
    },
];

/// Search settings actions by query. Empty query returns all actions.
///
/// # Examples
///
/// ```
/// use burrow_lib::commands::settings::search_settings;
///
/// let all = search_settings("").unwrap();
/// assert_eq!(all.len(), 5);
///
/// let filtered = search_settings("config").unwrap();
/// assert_eq!(filtered.len(), 1);
/// assert_eq!(filtered[0].id, "config");
/// ```
pub fn search_settings(query: &str) -> Result<Vec<SearchResult>, String> {
    let q = query.to_lowercase();
    let results: Vec<SearchResult> = SETTINGS
        .iter()
        .filter(|s| {
            if q.is_empty() {
                return true;
            }
            s.id.to_lowercase().contains(&q)
                || s.name.to_lowercase().contains(&q)
                || s.description.to_lowercase().contains(&q)
        })
        .map(|s| SearchResult {
            id: s.id.to_string(),
            name: s.name.to_string(),
            description: s.description.to_string(),
            icon: "".into(),
            category: "action".into(),
            exec: "".into(),
        })
        .collect();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        let results = search_settings("").unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn reindex_matches() {
        let results = search_settings("rei").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "reindex");
    }

    #[test]
    fn config_matches() {
        let results = search_settings("config").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "config");
    }

    #[test]
    fn stats_matches() {
        let results = search_settings("stat").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "stats");
    }

    #[test]
    fn update_matches() {
        let results = search_settings("upd").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "update");
    }

    #[test]
    fn unknown_returns_empty() {
        let results = search_settings("zzzzz").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn results_have_action_category() {
        let results = search_settings("").unwrap();
        for r in &results {
            assert_eq!(r.category, "action");
        }
    }

    #[test]
    fn description_search_works() {
        let results = search_settings("incremental").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "update");
    }

    #[test]
    fn case_insensitive_search() {
        let results = search_settings("REINDEX").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "reindex");
    }

    #[test]
    fn partial_match_re() {
        let results = search_settings("re").unwrap();
        assert!(results.iter().any(|r| r.id == "reindex"));
    }

    #[test]
    fn all_results_have_empty_exec() {
        let results = search_settings("").unwrap();
        for r in &results {
            assert!(r.exec.is_empty());
        }
    }
}
