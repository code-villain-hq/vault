use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::limits::Attempts;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Carries the id of a workspace whose change log grew, or the nil id to
    /// wake every connection (after missed notifications).
    pub wake: broadcast::Sender<Uuid>,
    pub cookie_secure: bool,
    /// Wrong passwords per account, to slow down guessing.
    pub password_attempts: Arc<Attempts>,
    /// Sign-ups per client address, so nobody can make accounts (or find
    /// out which emails have one) by the thousand.
    pub signup_attempts: Arc<Attempts>,
    /// Whether to take client addresses from `X-Forwarded-For`, which only
    /// a proxy in front of Glyph may set (`GLYPH_TRUST_FORWARDED_FOR=1`).
    pub trust_forwarded_for: bool,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool) -> Self {
        let (wake, _) = broadcast::channel(1024);
        AppState {
            pool,
            wake,
            cookie_secure,
            password_attempts: Arc::default(),
            signup_attempts: Arc::new(Attempts::sign_ups()),
            trust_forwarded_for: false,
        }
    }

    /// Takes client addresses from `X-Forwarded-For`, for a server behind a
    /// proxy that sets it.
    pub fn trusting_forwarded_for(mut self, trust: bool) -> Self {
        self.trust_forwarded_for = trust;
        self
    }
}
