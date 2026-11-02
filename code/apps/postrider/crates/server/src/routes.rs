use axum::extract::State;
use axum::routing::{any, delete, get, post};
use axum::{Json, Router};
use postrider_domain::{Bootstrap, Transaction, TransactionResult};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, archive, ask, caldav, connect, files, folders, mail, reply, ws};

/// The API, plus the built web app from `web_dir` when given.
pub fn app(state: AppState, web_dir: Option<&std::path::Path>) -> Router {
    let router = router(state);
    let Some(dir) = web_dir else { return router };
    // Paths the app doesn't know get index.html, so its own router can show them.
    let files = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
    router
        .route(
            "/api/{*rest}",
            any(|| async { AppError::not_found("no such endpoint") }),
        )
        .fallback_service(files)
}

pub fn router(state: AppState) -> Router {
    // Everything under /api/w/{workspace_id} requires membership (`Member`).
    let workspace = Router::new()
        .route("/sync/bootstrap", get(bootstrap))
        .route("/sync/transactions", post(transaction))
        .route("/sync/ws", get(ws::handler))
        .route("/inbox", get(mail::inbox))
        .route("/ask", post(ask::ask))
        .route("/answers/{answer}/undo", post(ask::undo))
        .route("/mail/sync", post(mail::sync))
        .route("/mails/{account_id}/{uid}", get(mail::open))
        .route("/mails/{account_id}/{uid}/archive", post(archive::archive))
        .route("/mails/{account_id}/{uid}/trash", post(archive::trash))
        .route("/mails/{account_id}/{uid}/move", post(archive::move_to))
        .route("/mails/{account_id}/{uid}/reply", post(reply::reply))
        .route("/mails/{account_id}/{uid}/files/{index}", get(files::file))
        .route("/folders", get(folders::folders))
        .route("/folders/{key}", get(folders::folder))
        .route("/folders/{key}/rename", post(folders::rename))
        .route("/search", get(folders::search))
        .route("/day", get(caldav::day))
        .route("/calendars", get(caldav::calendars))
        .route("/accounts", post(connect::add_account))
        .route("/accounts/{id}", delete(connect::remove_account))
        .route(
            "/calendar",
            post(connect::add_calendar).delete(connect::remove_calendar),
        );

    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route(ask::MCP_PATH, post(ask::mcp_endpoint))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/auth/providers", get(providers))
        .route("/api/me", get(accounts::me))
        .nest("/api/w/{workspace_id}", workspace)
        .layer(desktop_cors(&state.extra_origins))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

async fn bootstrap(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Bootstrap>, AppError> {
    Ok(Json(
        postrider_sync::bootstrap(&state.pool, member.actor).await?,
    ))
}

async fn transaction(
    State(state): State<AppState>,
    member: Member,
    Json(transaction): Json<Transaction>,
) -> Result<Json<TransactionResult>, AppError> {
    let sync_id =
        postrider_sync::apply_transaction(&state.pool, member.actor, &transaction).await?;
    Ok(Json(TransactionResult { sync_id }))
}

/// `GET /api/auth/providers`: how to sign in. Postrider runs only on this
/// Mac so far, where nobody signs in.
async fn providers() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "oidc": false, "company": null }))
}

/// The packaged desktop app runs from its own origin (`tauri://localhost`)
/// and signs requests with a bearer token, so it may call the API across
/// origins. No cookies are involved.
fn desktop_cors(extra_origins: &[String]) -> CorsLayer {
    use axum::http::{HeaderName, HeaderValue, Method, header};
    let origins = ["tauri://localhost", "http://tauri.localhost"]
        .into_iter()
        .map(HeaderValue::from_static)
        .chain(
            extra_origins
                .iter()
                .filter_map(|o| HeaderValue::from_str(o).ok()),
        );
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static(crate::auth::CLIENT_HEADER),
        ])
        .expose_headers([HeaderName::from_static(crate::auth::SESSION_HEADER)])
}
