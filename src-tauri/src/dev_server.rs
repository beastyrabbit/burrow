use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::actions::{self, modifier::Modifier};
use crate::commands::{apps, chat, health, history};
use crate::router::{self, SearchResult};

type AppState = tauri::AppHandle;

async fn search(
    State(app): State<AppState>,
    Json(body): Json<SearchBody>,
) -> Result<Json<Vec<SearchResult>>, (StatusCode, String)> {
    router::search(body.query, app)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct SearchBody {
    #[serde(default)]
    query: String,
}

async fn record_launch(
    State(app): State<AppState>,
    Json(body): Json<RecordLaunchBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    history::record_launch(
        body.id,
        body.name,
        body.exec,
        body.icon,
        body.description,
        app,
    )
    .map(Json)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct RecordLaunchBody {
    id: String,
    name: String,
    exec: String,
    icon: String,
    description: String,
}

async fn launch_app(Json(body): Json<LaunchAppBody>) -> Result<Json<()>, (StatusCode, String)> {
    apps::launch_app(body.exec)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct LaunchAppBody {
    exec: String,
}

async fn chat_ask(
    State(app): State<AppState>,
    Json(body): Json<ChatAskBody>,
) -> Result<Json<String>, (StatusCode, String)> {
    chat::chat_ask(body.query, app)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct ChatAskBody {
    query: String,
}

async fn health_check(
    State(app): State<AppState>,
) -> Result<Json<health::HealthStatus>, (StatusCode, String)> {
    health::health_check(app)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn run_setting(
    State(app): State<AppState>,
    Json(body): Json<RunSettingBody>,
) -> Result<Json<String>, (StatusCode, String)> {
    crate::run_setting(body.action, app)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct RunSettingBody {
    action: String,
}

async fn execute_action(
    State(app): State<AppState>,
    Json(body): Json<ExecuteActionBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    actions::execute_action(body.result, body.modifier, app)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct ExecuteActionBody {
    result: SearchResult,
    #[serde(default)]
    modifier: Modifier,
}

async fn load_vault() -> Result<Json<String>, (StatusCode, String)> {
    crate::commands::onepass::load_vault()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn hide_window(State(app): State<AppState>) -> Result<Json<()>, (StatusCode, String)> {
    crate::hide_window(app)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

pub fn start(app: tauri::AppHandle) {
    let router = Router::new()
        .route("/api/search", post(search))
        .route("/api/record_launch", post(record_launch))
        .route("/api/launch_app", post(launch_app))
        .route("/api/chat_ask", post(chat_ask))
        .route("/api/health_check", post(health_check))
        .route("/api/run_setting", post(run_setting))
        .route("/api/execute_action", post(execute_action))
        .route("/api/hide_window", post(hide_window))
        .route("/api/load_vault", post(load_vault))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _| {
                    origin.as_bytes().starts_with(b"http://localhost:")
                        || origin.as_bytes().starts_with(b"http://127.0.0.1:")
                }))
                .allow_methods([axum::http::Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        )
        .with_state(app);

    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:3001").await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(error = %e, "failed to bind dev-server on port 3001");
                tracing::error!("is another instance running? Browser bridge will not work.");
                return;
            }
        };
        tracing::info!("dev-server HTTP bridge listening on http://127.0.0.1:3001");
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "dev-server exited with error");
        }
    });
}
