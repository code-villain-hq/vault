use std::sync::Arc;

use code_villain_claude::{Adapter, Questions};

use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::ask::{Answers, Asking};
use crate::mail::Mail;
use crate::secrets::Secrets;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Carries the id of a workspace whose change log grew, or the nil id to
    /// wake every connection (after missed notifications).
    pub wake: broadcast::Sender<Uuid>,
    /// Where the accounts' passwords are: the macOS keychain.
    pub secrets: Secrets,
    /// The Inboxes, as Postrider reads them.
    pub mail: Arc<Mail>,
    /// The desktop app's own server, for the workspaces on this Mac: nobody
    /// signs in; the app gets its session from [`crate::local`].
    pub on_this_mac: bool,
    /// Origins besides the desktop app's that may call the API across
    /// origins (the dev web app, for a desktop app in development).
    pub extra_origins: Vec<String>,
    /// Claude Code on this Mac, which answers what you ask
    /// (`ask.rs`); `None` where it can't run.
    pub claude_code: Option<Adapter>,
    /// The questions Claude Code is answering, by their token.
    pub asking: Questions<Asking>,
    /// What answers changed, for Undo.
    pub answers: Answers,
}

impl AppState {
    pub fn new(pool: PgPool, secrets: Secrets) -> Self {
        let (wake, _) = broadcast::channel(1024);
        AppState {
            mail: Arc::new(Mail::new(pool.clone(), secrets.clone())),
            pool,
            wake,
            secrets,
            on_this_mac: false,
            extra_origins: Vec::new(),
            claude_code: None,
            asking: Questions::default(),
            answers: Answers::default(),
        }
    }

    /// The desktop app's own server: no sign-ins.
    pub fn on_this_mac(mut self) -> Self {
        self.on_this_mac = true;
        self
    }

    /// Claude Code answers what you ask, through this adapter.
    pub fn with_claude_code(mut self, adapter: Adapter) -> Self {
        self.claude_code = Some(adapter);
        self
    }

    /// Lets `origin` call the API across origins, as the desktop app may.
    pub fn allowing_origin(mut self, origin: impl Into<String>) -> Self {
        self.extra_origins.push(origin.into());
        self
    }
}
