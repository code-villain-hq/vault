use std::sync::Arc;

use sqlx::PgPool;

use crate::limits::Attempts;
use crate::oidc::OidcLogin;
use crate::telemetry::Recorder;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cookie_secure: bool,
    /// Signing in with Code Villain; when on, it's the only way in.
    pub oidc_login: Option<Arc<OidcLogin>>,
    /// Wrong passwords per account, to slow down guessing.
    pub password_attempts: Arc<Attempts>,
    /// Sign-ups per client address.
    pub signup_attempts: Arc<Attempts>,
    /// Whether to take client addresses from `X-Forwarded-For`, which only
    /// a proxy in front of Raven may set (`RAVEN_TRUST_FORWARDED_FOR=1`).
    pub trust_forwarded_for: bool,
    /// Where Raven's own calls and logs go, to be watched like any app's.
    pub recorder: Recorder,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool, recorder: Recorder) -> Self {
        AppState {
            pool,
            cookie_secure,
            oidc_login: None,
            password_attempts: Arc::default(),
            signup_attempts: Arc::new(Attempts::sign_ups()),
            trust_forwarded_for: false,
            recorder,
        }
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
