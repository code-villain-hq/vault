use std::sync::Arc;

use forge_ai::ModelProvider;
use sqlx::PgPool;
use tokio::sync::{Notify, broadcast};
use uuid::Uuid;

use crate::drift::DriftSettings;
use crate::github::GitHubApp;
use crate::github::login::GitHubLogin;
use crate::limits::Attempts;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Carries the id of a workspace whose change log grew, or the nil id to
    /// wake every connection (after missed notifications).
    pub wake: broadcast::Sender<Uuid>,
    pub cookie_secure: bool,
    /// The model drift checks use; `None` turns them off.
    pub ai: Option<Arc<dyn ModelProvider>>,
    pub drift: DriftSettings,
    /// Wakes the drift worker when a check is requested.
    pub drift_wake: Arc<Notify>,
    /// Wakes the Forgejo worker when Forgejo is connected.
    pub forgejo_wake: Arc<Notify>,
    /// The GitHub App; `None` turns the GitHub connection off.
    pub github: Option<Arc<GitHubApp>>,
    /// Signing in with GitHub; `None` turns it off.
    pub github_login: Option<Arc<GitHubLogin>>,
    /// Wrong passwords per account, to slow down guessing.
    pub password_attempts: Arc<Attempts>,
    /// Sign-ups per client address, so nobody can make accounts (or find
    /// out which emails have one) by the thousand.
    pub signup_attempts: Arc<Attempts>,
    /// Whether to take client addresses from `X-Forwarded-For`, which only
    /// a proxy in front of Forge may set (`FORGE_TRUST_FORWARDED_FOR=1`).
    pub trust_forwarded_for: bool,
    /// Where files live: attachments, and the GitHub connection's clones.
    pub data_dir: std::path::PathBuf,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool) -> Self {
        let (wake, _) = broadcast::channel(1024);
        AppState {
            pool,
            wake,
            cookie_secure,
            ai: None,
            drift: DriftSettings::default(),
            drift_wake: Arc::new(Notify::new()),
            forgejo_wake: Arc::new(Notify::new()),
            github: None,
            github_login: None,
            password_attempts: Arc::default(),
            signup_attempts: Arc::new(Attempts::sign_ups()),
            trust_forwarded_for: false,
            data_dir: "data".into(),
        }
    }

    /// Takes client addresses from `X-Forwarded-For`, for a server behind a
    /// proxy that sets it.
    pub fn trusting_forwarded_for(mut self, trust: bool) -> Self {
        self.trust_forwarded_for = trust;
        self
    }

    /// Keeps files (attachments) under `dir` instead of `./data`.
    pub fn with_data_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.data_dir = dir.into();
        self
    }

    /// Turns on signing in with GitHub.
    pub fn with_github_login(mut self, login: GitHubLogin) -> Self {
        self.github_login = Some(Arc::new(login));
        self
    }

    /// Turns on the GitHub connection; drift checks can then read its clones.
    pub fn with_github(mut self, app: GitHubApp) -> Self {
        self.drift.github_clones = Some(app.data_dir.clone());
        self.github = Some(Arc::new(app));
        self
    }

    /// Turns on drift checks with a model and settings.
    pub fn with_ai(mut self, provider: Arc<dyn ModelProvider>, settings: DriftSettings) -> Self {
        let github_clones = self.drift.github_clones.take();
        self.ai = Some(provider);
        self.drift = DriftSettings {
            github_clones: settings.github_clones.or(github_clones),
            ..settings
        };
        self
    }
}
