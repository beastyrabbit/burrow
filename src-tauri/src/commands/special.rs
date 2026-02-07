use crate::router::{Category, InputSpec, OutputMode, SearchResult};

struct SpecialCommand {
    name: &'static str,
    description: &'static str,
    icon: &'static str,
    exec_command: &'static str,
    /// Optional: (placeholder, template) for secondary input mode
    input_spec: Option<(&'static str, &'static str)>,
    /// How output is displayed. None = fire-and-forget (default).
    output_mode: Option<OutputMode>,
}

const COMMANDS: &[SpecialCommand] = &[
    SpecialCommand {
        name: "cowork",
        description: "Open kitty in ~/cowork and run Claude Code",
        icon: "",
        exec_command: "kitty sh -c 'cd ~/cowork && claude /init-cowork'",
        input_spec: Some((
            "Enter topic or press Enter to skip",
            "kitty sh -c \"cd $HOME/cowork && claude /init-cowork\\ {}\"",
        )),
        output_mode: None,
    },
    SpecialCommand {
        name: "kub-merge",
        description: "Run kub-merge in output window",
        icon: "",
        exec_command: "cd ~/cowork && claude -p \"/kub-merge\"",
        input_spec: None,
        output_mode: Some(OutputMode::Window),
    },
    SpecialCommand {
        name: "test-output",
        description: "Test output window with streaming lines",
        icon: "",
        exec_command: "for i in $(seq 1 20); do echo \"[stdout] Line $i: $(date +%H:%M:%S)\"; sleep 0.3; done; echo 'Stream complete.'",
        input_spec: None,
        output_mode: Some(OutputMode::Window),
    },
];

fn command_to_result(cmd: &SpecialCommand) -> SearchResult {
    SearchResult {
        id: format!("special-{}", cmd.name),
        name: cmd.name.to_string(),
        description: cmd.description.to_string(),
        icon: cmd.icon.to_string(),
        category: Category::Special,
        exec: cmd.exec_command.to_string(),
        input_spec: cmd.input_spec.map(|(placeholder, template)| InputSpec {
            placeholder: placeholder.to_string(),
            template: template.to_string(),
        }),
        output_mode: cmd.output_mode,
    }
}

/// Resolve a canonical special command result by `special-*` id.
pub fn resolve_special_by_id(id: &str) -> Option<SearchResult> {
    let name = id.strip_prefix("special-")?;
    COMMANDS
        .iter()
        .find(|cmd| cmd.name == name)
        .map(command_to_result)
}

pub fn search_special(query: &str) -> Result<Vec<SearchResult>, String> {
    let q = query.to_lowercase();
    Ok(COMMANDS
        .iter()
        .filter(|cmd| q.is_empty() || cmd.name.to_lowercase().contains(&q))
        .map(command_to_result)
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
        assert_eq!(results[0].category, Category::Special);
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

    #[test]
    fn cowork_has_input_spec() {
        let results = search_special("cowork").unwrap();
        assert_eq!(results.len(), 1);
        let spec = results[0]
            .input_spec
            .as_ref()
            .expect("cowork should have input_spec");
        assert!(
            !spec.placeholder.is_empty(),
            "placeholder should not be empty"
        );
        assert!(
            spec.template.contains("{}"),
            "template should contain {{}} placeholder"
        );
    }

    #[test]
    fn input_spec_template_uses_init_cowork() {
        let results = search_special("cowork").unwrap();
        let spec = results[0].input_spec.as_ref().unwrap();
        assert!(
            spec.template.contains("/init-cowork"),
            "template should use /init-cowork command, got: {}",
            spec.template
        );
        // User input ({}) must not appear inside unquoted or double-quoted context
        // where shell metacharacters could expand. resolve_exec wraps {} in single quotes,
        // so the template itself just needs {} positioned where a single-quoted arg is valid.
        assert!(
            !spec.template.contains("'{}"),
            "template should not mix single-quote boundaries with {{}}, got: {}",
            spec.template
        );
    }

    #[test]
    fn kub_merge_appears_in_search() {
        let results = search_special("kub").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "kub-merge");
        assert_eq!(results[0].category, Category::Special);
    }

    #[test]
    fn kub_merge_has_no_input_spec() {
        let results = search_special("kub-merge").unwrap();
        assert!(results[0].input_spec.is_none());
    }

    #[test]
    fn kub_merge_exec_runs_claude_in_cowork() {
        let results = search_special("kub-merge").unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0].exec.contains("cd ~/cowork"),
            "kub-merge should run from ~/cowork, got: {}",
            results[0].exec
        );
        assert!(
            results[0].exec.contains("claude"),
            "kub-merge should invoke claude, got: {}",
            results[0].exec
        );
    }

    #[test]
    fn kub_merge_has_window_output_mode() {
        let results = search_special("kub-merge").unwrap();
        assert_eq!(
            results[0].output_mode,
            Some(OutputMode::Window),
            "kub-merge should use Window output mode"
        );
    }

    #[test]
    fn cowork_has_no_output_mode() {
        let results = search_special("cowork").unwrap();
        assert_eq!(
            results[0].output_mode, None,
            "cowork should use default output mode (None)"
        );
    }

    #[test]
    fn test_output_has_window_output_mode() {
        let results = search_special("test-output").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test-output");
        assert_eq!(
            results[0].output_mode,
            Some(OutputMode::Window),
            "test-output should use Window output mode"
        );
    }

    #[test]
    fn test_output_search_match() {
        let results = search_special("test").unwrap();
        assert!(
            results.iter().any(|r| r.name == "test-output"),
            "searching 'test' should find test-output"
        );
    }

    #[test]
    fn resolve_special_by_id_returns_canonical_command() {
        let result = resolve_special_by_id("special-cowork").expect("special-cowork should exist");
        assert_eq!(result.id, "special-cowork");
        assert!(result.exec.contains("claude"));
    }

    #[test]
    fn resolve_special_by_id_unknown_returns_none() {
        assert!(resolve_special_by_id("special-unknown").is_none());
    }
}
