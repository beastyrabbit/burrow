use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::actions::{self, modifier::Modifier};
use crate::commands::{apps, chat, health, history};
use crate::context::AppContext;
use crate::router::{self, SearchResult};

type AppState = Arc<AppContext>;

async fn search(
    State(ctx): State<AppState>,
    Json(body): Json<SearchBody>,
) -> Result<Json<Vec<SearchResult>>, (StatusCode, String)> {
    router::search(body.query, &ctx)
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
    State(ctx): State<AppState>,
    Json(body): Json<RecordLaunchBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    history::record_launch(
        &body.id,
        &body.name,
        &body.exec,
        &body.icon,
        &body.description,
        &ctx,
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
    State(ctx): State<AppState>,
    Json(body): Json<ChatAskBody>,
) -> Result<Json<String>, (StatusCode, String)> {
    chat::chat_ask(body.query, &ctx)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct ChatAskBody {
    query: String,
}

async fn health_check(
    State(ctx): State<AppState>,
) -> Result<Json<health::HealthStatus>, (StatusCode, String)> {
    health::health_check(&ctx)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn execute_action(
    State(ctx): State<AppState>,
    Json(body): Json<ExecuteActionBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    actions::execute_action(body.result, body.modifier, body.secondary_input, &ctx)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Deserialize)]
struct ExecuteActionBody {
    result: SearchResult,
    #[serde(default)]
    modifier: Modifier,
    #[serde(default)]
    secondary_input: Option<String>,
}

async fn load_vault() -> Result<Json<String>, (StatusCode, String)> {
    crate::commands::onepass::load_vault()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn get_output(
    State(ctx): State<AppState>,
    Json(body): Json<GetOutputBody>,
) -> Json<crate::output_buffers::OutputSnapshot> {
    Json(ctx.output_buffers.get_since(&body.label, body.since_index))
}

#[derive(Deserialize)]
struct GetOutputBody {
    label: String,
    #[serde(default)]
    since_index: usize,
}

async fn hide_window_noop() -> Result<Json<()>, (StatusCode, String)> {
    tracing::trace!("hide_window_noop called (no-op in HTTP bridge)");
    Ok(Json(()))
}

/// Build the axum router with all API endpoints. Shared between dev_server and test-server.
pub fn build_router(ctx: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/search", post(search))
        .route("/api/record_launch", post(record_launch))
        .route("/api/launch_app", post(launch_app))
        .route("/api/chat_ask", post(chat_ask))
        .route("/api/health_check", post(health_check))
        .route("/api/execute_action", post(execute_action))
        .route("/api/get_output", post(get_output))
        .route("/api/hide_window", post(hide_window_noop))
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
        .with_state(ctx)
}

pub fn start(app: tauri::AppHandle) {
    use tauri::Manager;

    // Reuse the AppContext already managed by Tauri — same DB connections and indexer state.
    let managed_ctx = app.state::<AppContext>();
    let ctx = Arc::new(
        AppContext::from_arcs(
            managed_ctx.db.clone(),
            managed_ctx.vector_db.clone(),
            managed_ctx.indexer.clone(),
            managed_ctx.output_buffers.clone(),
        )
        .with_app_handle(app),
    );

    let router = build_router(ctx);

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
