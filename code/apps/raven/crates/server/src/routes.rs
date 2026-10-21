use axum::extract::State;
use axum::routing::{any, get, post};
use axum::{Json, Router, middleware};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, metrics, oidc, otlp, telemetry};

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
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/signup", post(accounts::signup))
        .route("/api/auth/login", post(accounts::login))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/oidc", get(oidc::start))
        .route("/api/auth/oidc/callback", get(oidc::callback))
        .route("/api/auth/oidc/desktop", post(oidc::desktop))
        .route("/api/me", get(accounts::me))
        .route("/api/apps", get(metrics::apps))
        .route("/api/apps/{app}/health", get(metrics::health_page))
        .route("/api/apps/{app}/technical", get(metrics::technical_page))
        .route("/api/apps/{app}/action", get(metrics::action_page))
        .route("/api/apps/{app}/calls/{trace_id}", get(metrics::call_page))
        .route("/v1/traces", post(otlp::traces))
        .route("/v1/logs", post(otlp::logs))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            telemetry::record_calls,
        ))
        // Outside the recording, so the desktop app's preflights aren't calls.
        .layer(desktop_cors())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

/// `GET /api/auth/providers`: the ways to sign in, and whether Raven still
/// needs its first account. Signing in with Code Villain, when on, is the
/// only way.
async fn providers(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let oidc = state.oidc_login.is_some();
    let setup = !oidc && !accounts::has_accounts(&state.pool).await?;
    Ok(Json(
        serde_json::json!({ "oidc": oidc, "password": !oidc, "setup": setup }),
    ))
}

/// The packaged desktop app runs from its own origin (`tauri://localhost`,
/// or `http://tauri.localhost` on Windows) and signs requests with a bearer
/// token, so it may call the API across origins. No cookies are involved.
fn desktop_cors() -> CorsLayer {
    use axum::http::{HeaderName, HeaderValue, Method, header};
    CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
        ]))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static(crate::auth::CLIENT_HEADER),
        ])
        .expose_headers([HeaderName::from_static(crate::auth::SESSION_HEADER)])
}
