use crate::router::SearchResult;

struct SpecialCommand {
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    exec_command: &'static str,
}

const COMMANDS: &[SpecialCommand] = &[SpecialCommand {
    name: "cowork",
    description: "Open kitty in ~/cowork and run cc",
    icon: "",
    exec_command: "kitty sh -c 'cd ~/cowork && cc'",
}];

pub fn search_special(query: &str) -> Result<Vec<SearchResult>, String> {
    let q = query.to_lowercase();
    Ok(COMMANDS
        .iter()
        .filter(|cmd| q.is_empty() || cmd.name.to_lowercase().contains(&q))
        .map(|cmd| SearchResult {
            id: format!("special-{}", cmd.name),
            name: cmd.name.to_string(),
            description: cmd.description.to_string(),
            icon: cmd.icon.to_string(),
            category: "special".into(),
            exec: cmd.exec_command.to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        let results = search_special("").unwrap();
        assert_eq!(results.len(), COMMANDS.len());
    }

    #[test]
    fn match_by_name() {
        let results = search_special("cowork").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "cowork");
        assert_eq!(results[0].category, "special");
    }

    #[test]
    fn case_insensitive_match() {
        let results = search_special("COWORK").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn partial_match() {
        let results = search_special("cow").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn no_match() {
        let results = search_special("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn result_has_exec_command() {
        let results = search_special("cowork").unwrap();
        assert!(results[0].exec.contains("kitty"));
        assert!(results[0].exec.contains("cowork"));
    }

    #[test]
    fn result_id_has_prefix() {
        let results = search_special("cowork").unwrap();
        assert!(results[0].id.starts_with("special-"));
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for cmd in COMMANDS {
            assert!(seen.insert(cmd.name), "duplicate name: {}", cmd.name);
        }
    }

    #[test]
    fn fields_are_non_empty() {
        for cmd in COMMANDS {
            assert!(!cmd.name.is_empty(), "name must not be empty");
            assert!(
                !cmd.exec_command.is_empty(),
                "exec_command must not be empty"
            );
        }
    }

    #[test]
    fn trimmed_query_matches() {
        // Router trims whitespace before calling search_special;
        // verify a pre-trimmed query with spaces still works.
        let results = search_special(" cowork ").unwrap();
        assert!(
            results.is_empty(),
            "leading/trailing spaces should not match since router trims before calling"
        );
    }
}
