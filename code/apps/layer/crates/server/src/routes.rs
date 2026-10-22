use axum::extract::State;
use axum::routing::{any, get, patch, post, put};
use axum::{Json, Router};
use layer_domain::{Bootstrap, Transaction, TransactionResult};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, groups, members, oidc, ws};

/// What the server answers outside `/api`.
#[derive(Clone, Copy)]
pub enum Web<'a> {
    /// Nothing: the desktop app's own server, which only the app calls.
    None,
    /// The built web app (`dist-web`): in production the server serves both.
    Dir(&'a std::path::Path),
    /// Development, where Vite serves the web app: its pages are sent on
    /// there, so a link to the server (Members…) opens the page.
    SentTo(&'a str),
}

pub fn app(state: AppState, web: Web<'_>) -> Router {
    let router = router(state);
    let unknown_api = any(|| async { AppError::not_found("no such endpoint") });
    match web {
        Web::None => router,
        Web::Dir(dir) => {
            // Paths the app doesn't know get index.html, so its own router can show them.
            let files = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
            router
                .route("/api/{*rest}", unknown_api)
                .fallback_service(files)
        }
        Web::SentTo(origin) => {
            let origin = origin.trim_end_matches('/').to_owned();
            router.route("/api/{*rest}", unknown_api).fallback(
                move |uri: axum::http::Uri| async move {
                    let path = uri.path_and_query().map_or("/", |p| p.as_str());
                    axum::response::Redirect::temporary(&format!("{origin}{path}"))
                },
            )
        }
    }
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
        .route("/members/{user_id}/access", put(members::update_access));

    let mut api = Router::new();
    if let Some(token) = &state.scim_token {
        api = api.nest(
            "/api/scim/v2",
            code_villain_scim::router::<groups::LayerDirectory, AppState>(
                state.pool.clone(),
                token.clone(),
            ),
        );
    }

    api.route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/logout", post(accounts::logout))
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/oidc", get(oidc::start))
        .route("/api/auth/oidc/callback", get(oidc::callback))
        .route("/api/auth/oidc/desktop", post(oidc::desktop))
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
    Ok(Json(
        layer_sync::bootstrap(&state.pool, member.actor).await?,
    ))
}

async fn transaction(
    State(state): State<AppState>,
    member: Member,
    Json(transaction): Json<Transaction>,
) -> Result<Json<TransactionResult>, AppError> {
    let sync_id = layer_sync::apply_transaction(&state.pool, member.actor, &transaction).await?;
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
