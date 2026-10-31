use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::acp::Adapter;
use crate::claude::Claude;
use crate::mcp::Questions;
use crate::oidc::OidcLogin;
use crate::reminders::Due;

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
    /// Claude through the Messages API, for talks and new topics on a
    /// server.
    pub claude: Claude,
    /// Claude Code on this Mac, which answers instead, as you're signed in
    /// to it: no key.
    pub claude_code: Option<Adapter>,
    /// ChatGPT on this Mac, through Codex, which answers when Settings
    /// says so.
    pub chatgpt: Option<Adapter>,
    /// The questions Claude Code is answering, for Almanac's tools.
    pub questions: Questions,
    /// Reminders as they come due, for the desktop app to notify.
    pub due: broadcast::Sender<Due>,
    /// The desktop app's own server, for the workspaces on this Mac: nobody
    /// signs in; the app gets its session from [`crate::local`].
    pub on_this_mac: bool,
    /// Origins besides the desktop app's that may call the API across
    /// origins (the dev web app, for a desktop app in development).
    pub extra_origins: Vec<String>,
}

impl AppState {
    pub fn new(pool: PgPool, cookie_secure: bool, claude: Claude) -> Self {
        let (wake, _) = broadcast::channel(1024);
        let (due, _) = broadcast::channel(64);
        AppState {
            pool,
            wake,
            cookie_secure,
            company: None,
            oidc_login: None,
            scim_token: None,
            claude,
            claude_code: None,
            chatgpt: None,
            questions: Questions::default(),
            due,
            on_this_mac: false,
            extra_origins: Vec::new(),
        }
    }

    /// The desktop app's own server: no sign-ins.
    pub fn on_this_mac(mut self) -> Self {
        self.on_this_mac = true;
        self
    }

    /// Answers with Claude Code on this Mac, and gives it Almanac's tools
    /// at `/api/mcp`.
    pub fn with_claude_code(mut self, claude_code: Adapter) -> Self {
        self.claude_code = Some(claude_code);
        self
    }

    /// Lets ChatGPT answer on this Mac instead, when it's chosen.
    pub fn with_chatgpt(mut self, chatgpt: Adapter) -> Self {
        self.chatgpt = Some(chatgpt);
        self
    }

    /// Who answers on this Mac: ChatGPT when Settings chose it, otherwise
    /// Claude Code. None on a server.
    pub async fn answerer(&self) -> Result<Option<&Adapter>, sqlx::Error> {
        let Some(claude_code) = &self.claude_code else {
            return Ok(None);
        };
        let answers: Option<String> = sqlx::query_scalar("SELECT answers FROM claude_choices")
            .fetch_optional(&self.pool)
            .await?;
        Ok(match (answers.as_deref(), &self.chatgpt) {
            (Some("chatgpt"), Some(chatgpt)) => Some(chatgpt),
            _ => Some(claude_code),
        })
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
