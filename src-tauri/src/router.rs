use crate::commands::{apps, files, history, math, onepass, settings, special, ssh, vectors};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub exec: String,
}

/// Determines which provider should handle a given query.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum RouteKind {
    History,
    App,
    FileSearch,
    VectorSearch,
    OnePassword,
    Ssh,
    Math,
    Settings,
    Chat,
    Special,
}

#[allow(dead_code)]
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

    if query.starts_with(':') {
        return RouteKind::Settings;
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

#[tauri::command]
pub async fn search(query: String, app: tauri::AppHandle) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return apps::get_all_apps_with_frecency(&app);
    }

    if query.starts_with('#') {
        let q = query.trim_start_matches('#').trim();
        return special::search_special(q);
    }

    if query.starts_with('?') {
        let q = query.trim_start_matches('?').trim();
        if q.is_empty() {
            return Ok(vec![SearchResult {
                id: "chat-hint".into(),
                name: "Type a question after ?".into(),
                description: "Press Enter to ask AI".into(),
                icon: "".into(),
                category: "info".into(),
                exec: "".into(),
            }]);
        }
        return Ok(vec![SearchResult {
            id: "chat-ask".into(),
            name: format!("Ask: {q}"),
            description: "Press Enter to get an AI answer".into(),
            icon: "".into(),
            category: "chat".into(),
            exec: "".into(),
        }]);
    }

    if query.starts_with(':') {
        let cmd = query.trim_start_matches(':').trim();
        return settings::search_settings(cmd);
    }

    if query.starts_with(' ') {
        let q = query.trim_start();
        if q.starts_with('*') {
            let content_query = q.trim_start_matches('*').trim();
            if content_query.is_empty() {
                return Ok(vec![]);
            }
            return vectors::search_by_content(content_query, &app).await;
        }
        return files::search_files(q);
    }

    if query.starts_with('!') {
        let q = query.trim_start_matches('!').trim();
        return onepass::search_onepass(q).await;
    }

    if query.starts_with("ssh ") || query == "ssh" {
        let q = query.strip_prefix("ssh").unwrap_or("").trim();
        return ssh::search_ssh(q);
    }

    if let Some(result) = math::try_calculate(&query) {
        return Ok(vec![result]);
    }

    apps::search_apps(&query)
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
    fn colon_prefix_routes_to_settings() {
        assert_eq!(classify_query(":reindex"), RouteKind::Settings);
    }

    #[test]
    fn colon_alone_routes_to_settings() {
        assert_eq!(classify_query(":"), RouteKind::Settings);
    }

    #[test]
    fn colon_with_text_routes_to_settings() {
        assert_eq!(classify_query(":config"), RouteKind::Settings);
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
}
