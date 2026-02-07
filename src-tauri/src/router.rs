use crate::commands::{apps, files, math, onepass, special, ssh, vectors};
use crate::context::AppContext;
use serde::{Deserialize, Serialize};

/// Specification for optional secondary input on a result.
/// When present, frontend enters two-stage input mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputSpec {
    /// Placeholder text shown in input field
    pub placeholder: String,
    /// Template for command when input is provided. Use {} for input substitution.
    /// Example: "kitty sh -c 'cd ~/cowork && claude /init-cowork \"{}\"'"
    /// If input is empty, the base exec is used instead.
    pub template: String,
}

/// How the output of a launched command should be presented.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Launch in an external terminal emulator (default behavior).
    Terminal,
    /// Stream output into a native Burrow secondary window.
    Window,
}

/// The category of a search result, determining how it's displayed and handled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    App,
    History,
    File,
    Ssh,
    Onepass,
    Math,
    Vector,
    Chat,
    Info,
    Special,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: Category,
    pub exec: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_spec: Option<InputSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mode: Option<OutputMode>,
}

/// Determines which provider should handle a given query.
#[derive(Debug, PartialEq)]
pub enum RouteKind {
    History,
    App,
    FileSearch,
    VectorSearch,
    OnePassword,
    Ssh,
    Math,
    Chat,
    Special,
}

pub fn classify_query(query: &str) -> RouteKind {
    if query.is_empty() {
        return RouteKind::History;
    }

    if query.starts_with('#') {
        return RouteKind::Special;
    }

    if query.starts_with('?') {
        return RouteKind::Chat;
    }

    if query.starts_with(' ') {
        let q = query.trim_start();
        if q.starts_with('*') {
            return RouteKind::VectorSearch;
        }
        return RouteKind::FileSearch;
    }

    if query.starts_with('!') {
        return RouteKind::OnePassword;
    }

    if query.starts_with("ssh ") || query == "ssh" {
        return RouteKind::Ssh;
    }

    if math::try_calculate(query).is_some() {
        return RouteKind::Math;
    }

    RouteKind::App
}

fn build_chat_results(q: &str) -> Vec<SearchResult> {
    if q.is_empty() {
        vec![SearchResult {
            id: "chat-hint".into(),
            name: "Type a question after ?".into(),
            description: "Press Enter to ask AI".into(),
            icon: "".into(),
            category: Category::Info,
            exec: "".into(),
            input_spec: None,
            output_mode: None,
        }]
    } else {
        vec![SearchResult {
            id: "chat-ask".into(),
            name: format!("Ask: {q}"),
            description: "Press Enter to get an AI answer".into(),
            icon: "".into(),
            category: Category::Chat,
            exec: "".into(),
            input_spec: None,
            output_mode: None,
        }]
    }
}

/// Primary search implementation — Tauri-free.
pub async fn search(query: String, ctx: &AppContext) -> Result<Vec<SearchResult>, String> {
    match classify_query(&query) {
        RouteKind::History => apps::get_all_apps_with_frecency(ctx),
        RouteKind::Special => {
            let q = query.trim_start_matches('#').trim();
            special::search_special(q)
        }
        RouteKind::Chat => {
            let q = query.trim_start_matches('?').trim();
            Ok(build_chat_results(q))
        }
        RouteKind::VectorSearch => {
            let content_query = query.trim_start().trim_start_matches('*').trim();
            if content_query.is_empty() {
                Ok(vec![])
            } else {
                vectors::search_by_content(content_query, ctx).await
            }
        }
        RouteKind::FileSearch => {
            let q = query.trim_start();
            files::search_files(q)
        }
        RouteKind::OnePassword => {
            let q = query.trim_start_matches('!').trim();
            onepass::search_onepass(q).await
        }
        RouteKind::Ssh => {
            let q = query.strip_prefix("ssh").unwrap_or("").trim();
            ssh::search_ssh(q)
        }
        RouteKind::Math => {
            if let Some(result) = math::try_calculate(&query) {
                Ok(vec![result])
            } else {
                apps::search_apps(&query)
            }
        }
        RouteKind::App => apps::search_apps(&query),
    }
}

/// Tauri command wrapper for search.
#[tauri::command]
pub async fn search_cmd(query: String, app: tauri::AppHandle) -> Result<Vec<SearchResult>, String> {
    use tauri::Manager;
    let ctx = app.state::<AppContext>();
    search(query, &ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify_query ---

    #[test]
    fn empty_query_routes_to_history() {
        assert_eq!(classify_query(""), RouteKind::History);
    }

    #[test]
    fn plain_text_routes_to_app() {
        assert_eq!(classify_query("firefox"), RouteKind::App);
    }

    #[test]
    fn space_prefix_routes_to_file_search() {
        assert_eq!(classify_query(" readme"), RouteKind::FileSearch);
    }

    #[test]
    fn space_star_prefix_routes_to_vector() {
        assert_eq!(classify_query(" *hello"), RouteKind::VectorSearch);
    }

    #[test]
    fn bang_prefix_routes_to_onepass() {
        assert_eq!(classify_query("!github"), RouteKind::OnePassword);
    }

    #[test]
    fn ssh_prefix_routes_to_ssh() {
        assert_eq!(classify_query("ssh myserver"), RouteKind::Ssh);
    }

    #[test]
    fn ssh_alone_routes_to_ssh() {
        assert_eq!(classify_query("ssh"), RouteKind::Ssh);
    }

    #[test]
    fn math_expression_routes_to_math() {
        assert_eq!(classify_query("1+3"), RouteKind::Math);
    }

    #[test]
    fn complex_math_routes_to_math() {
        assert_eq!(classify_query("(2+3)*4"), RouteKind::Math);
    }

    #[test]
    fn ssh_in_app_name_routes_to_app() {
        // "sshfs" should NOT match ssh prefix
        assert_eq!(classify_query("sshfs"), RouteKind::App);
    }

    #[test]
    fn bang_empty_routes_to_onepass() {
        assert_eq!(classify_query("!"), RouteKind::OnePassword);
    }

    #[test]
    fn space_only_routes_to_file_search() {
        // " " trimmed to "" → file search with empty query
        assert_eq!(classify_query(" "), RouteKind::FileSearch);
    }

    #[test]
    fn multiple_spaces_then_text() {
        assert_eq!(classify_query("   myfile"), RouteKind::FileSearch);
    }

    #[test]
    fn text_with_numbers_routes_to_app() {
        assert_eq!(classify_query("libreoffice7"), RouteKind::App);
    }

    #[test]
    fn colon_prefix_routes_to_app() {
        // Regression guard: `:` commands were removed — colon queries fall through to app search
        assert_eq!(classify_query(":reindex"), RouteKind::App);
        assert_eq!(classify_query(":health"), RouteKind::App);
        assert_eq!(classify_query(":stats"), RouteKind::App);
    }

    #[test]
    fn question_mark_routes_to_chat() {
        assert_eq!(classify_query("?what is rust"), RouteKind::Chat);
    }

    #[test]
    fn question_mark_alone_routes_to_chat() {
        assert_eq!(classify_query("?"), RouteKind::Chat);
    }

    #[test]
    fn hash_prefix_routes_to_special() {
        assert_eq!(classify_query("#cowork"), RouteKind::Special);
    }

    #[test]
    fn hash_alone_routes_to_special() {
        assert_eq!(classify_query("#"), RouteKind::Special);
    }

    // --- build_chat_results ---

    #[test]
    fn chat_results_empty_query_returns_hint() {
        let results = build_chat_results("");
        assert_eq!(
            results.len(),
            1,
            "expected one hint result, got: {}",
            results.len()
        );
        assert_eq!(
            results[0].id, "chat-hint",
            "expected chat-hint id, got: {}",
            results[0].id
        );
    }

    #[test]
    fn chat_results_non_empty_query_returns_ask() {
        let results = build_chat_results("hello");
        assert_eq!(
            results.len(),
            1,
            "expected one ask result, got: {}",
            results.len()
        );
        assert_eq!(
            results[0].id, "chat-ask",
            "expected chat-ask id, got: {}",
            results[0].id
        );
        assert!(
            results[0].name.contains("hello"),
            "expected name to contain query, got: {}",
            results[0].name
        );
    }

    // --- Category serialization ---

    #[test]
    fn category_serializes_to_lowercase() {
        use serde_json;

        assert_eq!(serde_json::to_string(&Category::App).unwrap(), "\"app\"");
        assert_eq!(
            serde_json::to_string(&Category::History).unwrap(),
            "\"history\""
        );
        assert_eq!(serde_json::to_string(&Category::File).unwrap(), "\"file\"");
        assert_eq!(serde_json::to_string(&Category::Ssh).unwrap(), "\"ssh\"");
        assert_eq!(
            serde_json::to_string(&Category::Onepass).unwrap(),
            "\"onepass\""
        );
        assert_eq!(serde_json::to_string(&Category::Math).unwrap(), "\"math\"");
        assert_eq!(
            serde_json::to_string(&Category::Vector).unwrap(),
            "\"vector\""
        );
        assert_eq!(serde_json::to_string(&Category::Chat).unwrap(), "\"chat\"");
        assert_eq!(serde_json::to_string(&Category::Info).unwrap(), "\"info\"");
        assert_eq!(
            serde_json::to_string(&Category::Special).unwrap(),
            "\"special\""
        );
    }

    #[test]
    fn category_deserializes_from_lowercase() {
        use serde_json;

        assert_eq!(
            serde_json::from_str::<Category>("\"app\"").unwrap(),
            Category::App
        );
        assert_eq!(
            serde_json::from_str::<Category>("\"math\"").unwrap(),
            Category::Math
        );
        assert_eq!(
            serde_json::from_str::<Category>("\"ssh\"").unwrap(),
            Category::Ssh
        );
    }

    #[test]
    fn search_result_serialization_roundtrip() {
        use serde_json;

        let result = SearchResult {
            id: "test-id".into(),
            name: "Test".into(),
            description: "A test result".into(),
            icon: "".into(),
            category: Category::Math,
            exec: "".into(),
            input_spec: None,
            output_mode: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(
            json.contains("\"category\":\"math\""),
            "Expected category to serialize as \"math\", got: {}",
            json
        );
        // input_spec should be omitted when None (skip_serializing_if)
        assert!(
            !json.contains("input_spec"),
            "input_spec should be omitted when None, got: {}",
            json
        );
        // output_mode should be omitted when None (skip_serializing_if)
        assert!(
            !json.contains("output_mode"),
            "output_mode should be omitted when None, got: {}",
            json
        );

        let parsed: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.category, Category::Math);
        assert_eq!(parsed.output_mode, None);
    }

    #[test]
    fn search_result_with_input_spec_serializes() {
        use serde_json;

        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "base-command".into(),
            input_spec: Some(InputSpec {
                placeholder: "Enter value".into(),
                template: "command --arg \"{}\"".into(),
            }),
            output_mode: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(
            json.contains("input_spec"),
            "input_spec should be present when Some, got: {}",
            json
        );
        assert!(
            json.contains("Enter value"),
            "placeholder should be in JSON, got: {}",
            json
        );

        let parsed: SearchResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.input_spec.is_some());
        let spec = parsed.input_spec.unwrap();
        assert_eq!(spec.placeholder, "Enter value");
        assert_eq!(spec.template, "command --arg \"{}\"");
    }

    // --- OutputMode serialization ---

    #[test]
    fn output_mode_serializes_to_lowercase() {
        use serde_json;

        assert_eq!(
            serde_json::to_string(&OutputMode::Terminal).unwrap(),
            "\"terminal\""
        );
        assert_eq!(
            serde_json::to_string(&OutputMode::Window).unwrap(),
            "\"window\""
        );
    }

    #[test]
    fn output_mode_deserializes_from_lowercase() {
        use serde_json;

        assert_eq!(
            serde_json::from_str::<OutputMode>("\"terminal\"").unwrap(),
            OutputMode::Terminal
        );
        assert_eq!(
            serde_json::from_str::<OutputMode>("\"window\"").unwrap(),
            OutputMode::Window
        );
    }

    #[test]
    fn search_result_with_output_mode_roundtrip() {
        use serde_json;

        let result = SearchResult {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Special,
            exec: "cmd".into(),
            input_spec: None,
            output_mode: Some(OutputMode::Window),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(
            json.contains("\"output_mode\":\"window\""),
            "output_mode should serialize as \"window\", got: {}",
            json
        );

        let parsed: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.output_mode, Some(OutputMode::Window));
    }

    #[test]
    fn search_result_without_output_mode_defaults_to_none() {
        use serde_json;

        // JSON without output_mode field should deserialize to None
        let json = r#"{"id":"t","name":"T","description":"","icon":"","category":"app","exec":""}"#;
        let parsed: SearchResult = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.output_mode, None,
            "missing output_mode should default to None"
        );
    }
}
