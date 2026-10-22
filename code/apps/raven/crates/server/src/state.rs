use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::limits::Attempts;
use crate::oidc::OidcLogin;
use crate::telemetry::Recorder;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Carries the id of a workspace whose change log grew, or the nil id to
    /// wake every connection (after missed notifications).
    pub wake: broadcast::Sender<Uuid>,
    pub cookie_secure: bool,
    /// Signing in with Code Villain; when on, it's the only way in.
    pub oidc_login: Option<Arc<OidcLogin>>,
    /// The token Pocket ID sends the account's people and groups with
    /// (`RAVEN_SCIM_TOKEN`); `None` turns groups off.
    pub scim_token: Option<String>,
    /// Wrong passwords per account, to slow down guessing.
    pub password_attempts: Arc<Attempts>,
    /// Sign-ups per client address, so nobody can make accounts (or find
    /// out which emails have one) by the thousand.
    pub signup_attempts: Arc<Attempts>,
    /// Whether to take client addresses from `X-Forwarded-For`, which only
    /// a proxy in front of Raven may set (`RAVEN_TRUST_FORWARDED_FOR=1`).
    pub trust_forwarded_for: bool,
    /// Where Raven's own calls and logs go, to be watched like any app's.
    pub recorder: Recorder,
    /// The desktop app's own server, for the workspaces on this Mac: nobody
    /// signs up or in; the app gets its session from [`crate::local`].
    pub on_this_mac: bool,
    /// Origins besides the desktop app's that may call the API across
    /// origins (the dev web app, for a desktop app in development).
    pub extra_origins: Vec<String>,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool, recorder: Recorder) -> Self {
        let (wake, _) = broadcast::channel(1024);
        AppState {
            pool,
            wake,
            cookie_secure,
            oidc_login: None,
            scim_token: None,
            password_attempts: Arc::default(),
            signup_attempts: Arc::new(Attempts::sign_ups()),
            trust_forwarded_for: false,
            recorder,
            on_this_mac: false,
            extra_origins: Vec::new(),
        }
    }

    /// The desktop app's own server: no sign-ups and no sign-ins.
    pub fn on_this_mac(mut self) -> Self {
        self.on_this_mac = true;
        self
    }

    /// Lets `origin` call the API across origins, as the desktop app may.
    pub fn allowing_origin(mut self, origin: impl Into<String>) -> Self {
        self.extra_origins.push(origin.into());
        self
    }

    /// Takes the account's people and groups at `/api/scim/v2` (`crate::groups`).
    pub fn with_scim(mut self, token: impl Into<String>) -> Self {
        self.scim_token = Some(token.into());
        self
    }

    /// Turns on signing in with Code Villain, and off passwords.
    pub fn with_oidc_login(mut self, login: OidcLogin) -> Self {
        self.oidc_login = Some(Arc::new(login));
        self
    }

    /// Takes client addresses from `X-Forwarded-For`, for a server behind a
    /// proxy that sets it.
    pub fn trusting_forwarded_for(mut self, trust: bool) -> Self {
        self.trust_forwarded_for = trust;
        self
    }
}
