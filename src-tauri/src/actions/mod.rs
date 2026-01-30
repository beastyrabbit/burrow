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
