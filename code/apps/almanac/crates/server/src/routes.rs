use almanac_domain::{Bootstrap, Transaction, TransactionResult};
use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{any, get, patch, post, put};
use axum::{Json, Router};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{
    accounts, groups, handed, keys, mcp, members, oidc, photos, reading, talks, topics, ws,
};

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
        .route("/topics", post(topics::start))
        .route("/topics/{topic_id}/set_up", post(topics::set_up_made))
        .route(
            "/topics/{topic_id}/ask",
            // A question may come with photos, a few MB each.
            post(talks::ask).layer(DefaultBodyLimit::max(48 * 1024 * 1024)),
        )
        .route(
            "/photos",
            post(photos::upload).layer(DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route("/photos/{photo_id}", get(photos::show))
        .route("/messages/{message_id}/undo", post(topics::undo))
        .route(
            "/claude",
            get(keys::show).put(keys::set).delete(keys::forget),
        )
        .route("/claude/web", put(keys::set_web))
        .route("/claude/answers", put(keys::set_answers))
        .route("/claude/chatgpt/sign-in", post(keys::sign_in_chatgpt))
        .route(
            "/reading",
            get(reading::show)
                .post(reading::make)
                .delete(reading::revoke),
        );

    // Claude Code's way to Almanac's tools, by each question's token: on
    // this Mac, and on a server for a question handed to the asker's Mac,
    // which says there what it made of it.
    let mut api = Router::new()
        .route("/api/mcp", post(mcp::handle))
        .route("/api/answered", post(handed::answered));
    // Another app's way to read the workspace, by a member's token.
    api = api.route("/api/mcp/read", post(reading::handle));
    if let Some(token) = &state.scim_token {
        api = api.nest(
            "/api/scim/v2",
            code_villain_scim::router::<groups::AlmanacDirectory, AppState>(
                state.pool.clone(),
                token.clone(),
            ),
        );
    }

    api.route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/oidc", get(oidc::start::<AppState>))
        .route("/api/auth/oidc/callback", get(oidc::callback::<AppState>))
        .route("/api/auth/oidc/desktop", post(oidc::desktop::<AppState>))
        .route("/api/me", get(accounts::me).put(accounts::update_profile))
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
    // The company's workspace is made before anyone is in it: its own topic
    // comes with the first snapshot.
    crate::own::ensure_alone(&state.pool, member.actor).await;
    Ok(Json(
        almanac_sync::bootstrap(&state.pool, member.actor).await?,
    ))
}

async fn transaction(
    State(state): State<AppState>,
    member: Member,
    Json(transaction): Json<Transaction>,
) -> Result<Json<TransactionResult>, AppError> {
    let sync_id = almanac_sync::apply_transaction(&state.pool, member.actor, &transaction).await?;
    Ok(Json(TransactionResult { sync_id }))
}

/// `GET /api/auth/providers`: how to sign in. A server signs in with its
/// company's account, named on the button; the desktop app's own server
/// signs nobody in.
async fn providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    let company = state
        .company
        .as_ref()
        .filter(|_| state.oidc_login.is_some() && !state.on_this_mac);
    Json(serde_json::json!({ "oidc": company.is_some(), "company": company }))
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
