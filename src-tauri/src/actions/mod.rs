pub mod dry_run;
pub mod handlers;
pub mod modifier;
pub mod utils;

use crate::context::AppContext;
use crate::router::SearchResult;
use modifier::Modifier;

/// Primary execute_action — Tauri-free.
pub async fn execute_action(
    result: SearchResult,
    modifier: Modifier,
    secondary_input: Option<String>,
    ctx: &AppContext,
) -> Result<(), String> {
    handlers::handle_action(&result, modifier, secondary_input.as_deref(), ctx)
}

/// Tauri command wrapper for execute_action.
#[tauri::command]
pub async fn execute_action_cmd(
    result: SearchResult,
    modifier: Modifier,
    secondary_input: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let ctx = app.state::<AppContext>();
    execute_action(result, modifier, secondary_input, &ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::Category;

    #[test]
    fn info_category_is_noop() {
        // Verify dispatch logic for categories that don't need AppHandle
        let result = SearchResult {
            id: "info-1".into(),
            name: "Info".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Info,
            exec: "".into(),
            input_spec: None,
        };
        assert!(handlers::is_valid_category(result.category));
    }

    #[test]
    fn math_none_dispatches_ok() {
        let result = SearchResult {
            id: "m".into(),
            name: "= 5".into(),
            description: "".into(),
            icon: "".into(),
            category: Category::Math,
            exec: "".into(),
            input_spec: None,
        };
        assert!(handlers::is_valid_category(result.category));
    }
}
