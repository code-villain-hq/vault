use std::sync::Arc;

use glyph_llm::{Models, Processes};
use sqlx::PgPool;
use tokio::sync::{Semaphore, broadcast};
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
    /// The translation models, from the models file.
    pub models: Arc<Models>,
    /// The model servers this server started.
    pub processes: Processes,
    /// Translation runs take turns: one model serves them all.
    pub llm_turn: Arc<Semaphore>,
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
            models: Arc::default(),
            processes: Processes::new(),
            llm_turn: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn with_models(mut self, models: Models) -> Self {
        self.models = Arc::new(models);
        self
    }

    /// Takes client addresses from `X-Forwarded-For`, for a server behind a
    /// proxy that sets it.
    pub fn trusting_forwarded_for(mut self, trust: bool) -> Self {
        self.trust_forwarded_for = trust;
        self
    }
}
