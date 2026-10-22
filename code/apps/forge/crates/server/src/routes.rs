use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{any, delete, get, patch, post, put};
use axum::{Json, Router};
use forge_domain::{Bootstrap, IssueEvent, SpecVersion, Transaction, TransactionResult};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{
    accounts, agents, attachments, drift, forgejo, github, groups, mcp, members, oidc, runners,
    teams, ws,
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
        .route("/issues/{issue_id}/history", get(issue_history))
        .route("/specs/{spec_id}/history", get(spec_history))
        .route(
            "/attachments",
            post(attachments::upload).layer(DefaultBodyLimit::max(attachments::MAX_SIZE)),
        )
        .route("/attachments/{attachment_id}", get(attachments::download))
        .route("/attachments/{attachment_id}/link", post(attachments::link))
        .route("/issues/{issue_id}/triage", post(drift::triage::request))
        .route("/members", get(members::list))
        .route(
            "/members/{user_id}",
            patch(members::update).delete(members::remove),
        )
        .route("/members/{user_id}/access", put(members::update_access))
        .route("/teams", post(teams::create))
        .route("/agents", get(agents::list).post(agents::create))
        .route("/agents/{agent_id}", patch(agents::rename))
        .route("/agents/{agent_id}/tokens", post(agents::create_token))
        .route(
            "/agents/{agent_id}/runner",
            put(runners::set).delete(runners::remove),
        )
        .route(
            "/agents/{agent_id}/runner/secret",
            post(runners::new_runner_secret),
        )
        .route("/tokens/{token_id}", delete(agents::revoke_token))
        .route("/services/{service_id}/check", post(drift::request_check))
        .route("/drift-checks/{run_id}", get(drift::check_details))
        .route("/ai", get(drift::status))
        .route(
            "/forgejo",
            get(forgejo::status)
                .put(forgejo::connect)
                .delete(forgejo::disconnect),
        )
        .route("/groups", get(groups::list))
        .route("/github", get(github::status))
        .route("/github/refresh", post(github::refresh))
        .route(
            "/suggestions/{suggestion_id}/accept",
            post(drift::suggestions::accept),
        )
        .route(
            "/suggestions/{suggestion_id}/dismiss",
            post(drift::suggestions::dismiss),
        );

    let mut api = Router::new();
    if let Some(token) = &state.scim_token {
        api = api.nest(
            "/api/scim/v2",
            code_villain_scim::router::<groups::ForgeDirectory, AppState>(
                state.pool.clone(),
                token.clone(),
            ),
        );
    }

    api.route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/me", get(accounts::me).put(accounts::update_profile))
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/oidc", get(oidc::start))
        .route("/api/auth/oidc/callback", get(oidc::callback))
        .route("/api/auth/oidc/desktop", post(oidc::desktop))
        .route("/api/github/setup", get(github::setup))
        .route("/api/auth/github/callback", get(github::oauth::callback))
        .route("/api/github/webhook", post(github::webhook::receive))
        .nest("/api/w/{workspace_id}", workspace)
        .merge(mcp::router(state.clone()))
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
        forge_sync::bootstrap(&state.pool, member.actor).await?,
    ))
}

async fn transaction(
    State(state): State<AppState>,
    member: Member,
    Json(transaction): Json<Transaction>,
) -> Result<Json<TransactionResult>, AppError> {
    let sync_id = forge_sync::apply_transaction(&state.pool, member.actor, &transaction).await?;
    Ok(Json(TransactionResult { sync_id }))
}

async fn issue_history(
    State(state): State<AppState>,
    member: Member,
    Path((_, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<IssueEvent>>, AppError> {
    member.require_sees_issue(&state.pool, issue_id).await?;
    forge_sync::issue_events(&state.pool, member.actor, issue_id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("issue not found"))
}

async fn spec_history(
    State(state): State<AppState>,
    member: Member,
    Path((_, spec_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<SpecVersion>>, AppError> {
    member.require_not_guest()?;
    forge_sync::spec_versions(&state.pool, member.actor, spec_id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("spec not found"))
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
