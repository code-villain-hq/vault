use std::sync::Arc;

use forge_ai::ModelProvider;
use sqlx::PgPool;
use tokio::sync::{Notify, broadcast};
use uuid::Uuid;

use crate::drift::DriftSettings;
use crate::github::GitHubApp;
use crate::github::oauth::GitHubOAuth;
use crate::oidc::OidcLogin;

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
    /// The GitHub App's OAuth client, which confirms who installs the App.
    pub github_oauth: Option<Arc<GitHubOAuth>>,
    /// The company the server runs for, as the sign-in button names it.
    pub company: Option<String>,
    /// Signing in with the company's account: the only way in.
    pub oidc_login: Option<Arc<OidcLogin>>,
    /// The token the company's account sends its people and groups with
    /// (`scim.token`).
    pub scim_token: Option<String>,
    /// Hosts besides this one that may call `/mcp` (`mcp.allowed_hosts`).
    pub mcp_allowed_hosts: Vec<String>,
    /// Where files live: attachments, and the GitHub connection's clones.
    pub data_dir: std::path::PathBuf,
    /// The desktop app's own server, for the workspaces on this Mac: nobody
    /// signs in; the app gets its session from [`crate::local`].
    pub on_this_mac: bool,
    /// Origins besides the desktop app's that may call the API across
    /// origins (the dev web app, for a desktop app in development).
    pub extra_origins: Vec<String>,
    /// Download links to attachments, not yet used.
    pub tickets: Arc<crate::attachments::Tickets>,
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
            github_oauth: None,
            company: None,
            oidc_login: None,
            scim_token: None,
            mcp_allowed_hosts: Vec::new(),
            data_dir: "data".into(),
            on_this_mac: false,
            extra_origins: Vec::new(),
            tickets: Arc::default(),
        }
    }

    /// The desktop app's own server: no sign-ins.
    pub fn on_this_mac(mut self) -> Self {
        self.on_this_mac = true;
        self
    }

    /// Lets `origin` call the API across origins, as the desktop app may.
    pub fn allowing_origin(mut self, origin: impl Into<String>) -> Self {
        self.extra_origins.push(origin.into());
        self
    }

    /// Keeps files (attachments) under `dir` instead of `./data`.
    pub fn with_data_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.data_dir = dir.into();
        self
    }

    /// Confirms who installs the GitHub App with its OAuth client.
    pub fn with_github_oauth(mut self, oauth: GitHubOAuth) -> Self {
        self.github_oauth = Some(Arc::new(oauth));
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

    /// Lets `/mcp` be called at these hosts too, as behind a proxy.
    pub fn allowing_mcp_hosts(mut self, hosts: Vec<String>) -> Self {
        self.mcp_allowed_hosts = hosts;
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
