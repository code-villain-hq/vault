use axum::extract::State;
use axum::routing::{any, delete, get, patch, post, put};
use axum::{Json, Router};
use glyph_domain::{Bootstrap, Transaction, TransactionResult};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, invites, members, models, oidc, translate, ws};

/// The API, plus the built web app from `web_dir` when given (in production
/// the server serves both; in development Vite serves the app).
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
        .route("/", patch(accounts::rename_workspace))
        .route("/sync/bootstrap", get(bootstrap))
        .route("/sync/transactions", post(transaction))
        .route("/sync/ws", get(ws::handler))
        .route("/members", get(members::list))
        .route(
            "/members/{user_id}",
            patch(members::update).delete(members::remove),
        )
        .route("/invites", get(invites::list).post(invites::create))
        .route("/invites/{invite_id}", delete(invites::revoke))
        .route("/runs", post(translate::start))
        .route("/runs/{run_id}/cancel", post(translate::cancel))
        .route("/glossary/variants", post(translate::variants));

    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/signup", post(accounts::signup))
        .route("/api/auth/login", post(accounts::login))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/oidc", get(oidc::start))
        .route("/api/auth/oidc/callback", get(oidc::callback))
        .route("/api/auth/oidc/desktop", post(oidc::desktop))
        .route("/api/me", get(accounts::me).put(accounts::update_profile))
        .route("/api/me/password", put(accounts::change_password))
        .route("/api/invites/{token}", get(invites::preview))
        .route("/api/invites/{token}/accept", post(invites::accept))
        .route("/api/models", get(models::list).post(models::add))
        .route("/api/models/{name}", delete(models::remove))
        .route("/api/models/{name}/start", post(models::start))
        .route("/api/models/{name}/stop", post(models::stop))
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
        glyph_sync::bootstrap(&state.pool, member.actor).await?,
    ))
}

async fn transaction(
    State(state): State<AppState>,
    member: Member,
    Json(transaction): Json<Transaction>,
) -> Result<Json<TransactionResult>, AppError> {
    let sync_id = glyph_sync::apply_transaction(&state.pool, member.actor, &transaction).await?;
    Ok(Json(TransactionResult { sync_id }))
}

/// `GET /api/auth/providers`: the ways to sign in. Signing in with Code
/// Villain, when on, is the only one.
async fn providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    if state.on_this_mac {
        return Json(serde_json::json!({ "oidc": false, "password": false }));
    }
    let oidc = state.oidc_login.is_some();
    Json(serde_json::json!({ "oidc": oidc, "password": !oidc }))
}

/// The packaged desktop app runs from its own origin (`tauri://localhost`,
/// or `http://tauri.localhost` on Windows) and signs requests with a bearer
/// token, so it may call the API across origins. No cookies are involved.
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
