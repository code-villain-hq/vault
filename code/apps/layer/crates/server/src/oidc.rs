//! Signing in with the company's account: shared in `code-villain-auth`,
//! named for Layer (any user; no agent filter).

use axum::http::{HeaderMap, HeaderValue};
use axum::response::Response;
use code_villain_auth::{LinkPolicy, OidcHost, OidcLogin as KitOidcLogin};
use uuid::Uuid;

use crate::accounts::signed_in;
use crate::auth::session_cookie;
use crate::config::Settings;
use crate::error::AppError;
use crate::state::AppState;

pub use code_villain_auth::{
    CallbackParams, DESKTOP_RETURN, DesktopRequest, StartParams, callback, desktop, start,
};

pub type OidcLogin = KitOidcLogin;

impl OidcHost for AppState {
    type Error = AppError;

    fn oidc_cookie() -> &'static str {
        "layer_oidc_login"
    }

    fn link_policy() -> LinkPolicy {
        LinkPolicy::AnyUser
    }

    fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    fn cookie_secure(&self) -> bool {
        self.cookie_secure
    }

    fn oidc_login(&self) -> Option<&OidcLogin> {
        self.oidc_login.as_deref()
    }

    fn session_cookie(token: &str, secure: bool) -> HeaderValue {
        session_cookie(token, secure)
    }

    fn unauthorized(message: impl Into<String>) -> Self::Error {
        AppError::unauthorized(message)
    }

    fn not_found(message: impl Into<String>) -> Self::Error {
        AppError::not_found(message)
    }

    fn from_sqlx(error: sqlx::Error) -> Self::Error {
        AppError::from(error)
    }

    async fn signed_in(
        &self,
        headers: &HeaderMap,
        token: &str,
        user_id: Uuid,
    ) -> Result<Response, Self::Error> {
        signed_in(self, headers, token, user_id).await
    }
}

/// From `[oidc]` and `public_url`, which the server requires.
pub fn from_settings(settings: &Settings) -> anyhow::Result<OidcLogin> {
    settings.require(&[
        "oidc.issuer",
        "oidc.client_id",
        "oidc.client_secret",
        "public_url",
    ])?;
    let get = |key: &str| settings.get(key).unwrap_or_default();
    OidcLogin::new(
        get("oidc.issuer"),
        get("oidc.client_id"),
        get("oidc.client_secret"),
        &get("public_url"),
        "layer",
    )
}
