use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use glyph_llm::{Models, Processes};
use sqlx::PgPool;
use tokio::sync::{Semaphore, broadcast};
use uuid::Uuid;

use crate::oidc::OidcLogin;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Carries the id of a workspace whose change log grew, or the nil id to
    /// wake every connection (after missed notifications).
    pub wake: broadcast::Sender<Uuid>,
    pub cookie_secure: bool,
    /// The company the server runs for, as the sign-in button names it.
    pub company: Option<String>,
    /// Signing in with the company's account: the only way in.
    pub oidc_login: Option<Arc<OidcLogin>>,
    /// The token the company's account sends its people and groups with
    /// (`scim.token`).
    pub scim_token: Option<String>,
    /// The translation models, from the models file. Read them with
    /// [`AppState::models`].
    pub models: Arc<RwLock<Models>>,
    /// The models file, when people may add and remove models from Settings:
    /// only on the desktop app's own server. A shared server's models are
    /// its operator's, as the file names programs for the server to run.
    pub models_file: Option<PathBuf>,
    /// The model servers this server started.
    pub processes: Processes,
    /// Translation runs take turns: one model serves them all.
    pub llm_turn: Arc<Semaphore>,
    /// The desktop app's own server, for the workspaces on this Mac: nobody
    /// signs in; the app gets its session from [`crate::local`].
    pub on_this_mac: bool,
    /// Origins besides the desktop app's that may call the API across
    /// origins (the dev web app, for a desktop app in development).
    pub extra_origins: Vec<String>,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool) -> Self {
        let (wake, _) = broadcast::channel(1024);
        AppState {
            pool,
            wake,
            cookie_secure,
            company: None,
            oidc_login: None,
            scim_token: None,
            models: Arc::default(),
            models_file: None,
            processes: Processes::new(),
            llm_turn: Arc::new(Semaphore::new(1)),
            on_this_mac: false,
            extra_origins: Vec::new(),
        }
    }

    pub fn with_models(mut self, models: Models) -> Self {
        self.models = Arc::new(RwLock::new(models));
        self
    }

    /// The models as they are now.
    pub fn models(&self) -> Models {
        self.models.read().unwrap().clone()
    }

    /// Lets Settings add and remove models, kept in `file`.
    pub fn with_models_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.models_file = Some(file.into());
        self
    }

    /// The desktop app's own server: nobody signs in.
    pub fn on_this_mac(mut self) -> Self {
        self.on_this_mac = true;
        self
    }

    /// Lets `origin` call the API across origins, as the desktop app may.
    pub fn allowing_origin(mut self, origin: impl Into<String>) -> Self {
        self.extra_origins.push(origin.into());
        self
    }

    /// The company the server runs for, named on the sign-in button.
    pub fn for_company(mut self, company: impl Into<String>) -> Self {
        self.company = Some(company.into());
        self
    }

    /// Takes the company's people and groups at `/api/scim/v2` (`crate::groups`).
    pub fn with_scim(mut self, token: impl Into<String>) -> Self {
        self.scim_token = Some(token.into());
        self
    }

    /// Signs people in with the company's account.
    pub fn with_oidc_login(mut self, login: OidcLogin) -> Self {
        self.oidc_login = Some(Arc::new(login));
        self
    }
}
