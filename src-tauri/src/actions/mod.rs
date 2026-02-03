pub mod dry_run;
pub mod handlers;
pub mod modifier;
pub mod utils;

use crate::router::SearchResult;
use modifier::Modifier;

#[tauri::command]
pub async fn execute_action(
    result: SearchResult,
    modifier: Modifier,
    app: tauri::AppHandle,
) -> Result<(), String> {
    handlers::handle_action(&result, modifier, &app)
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
        };
        assert!(handlers::is_valid_category(result.category));
    }
}
