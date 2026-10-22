//! The server's settings: `forge.toml` (`--config`, `FORGE_CONFIG`), with
//! the environment over it (`code/kit/settings`). `forge.example.toml` in
//! the app's folder has them all.

use std::net::SocketAddr;
use std::path::PathBuf;

pub use code_villain_settings::{Error, Settings};

/// Every setting Forge reads, as `section.key`.
pub const KNOWN: &[&str] = &[
    "company",
    "public_url",
    "addr",
    "web_dir",
    "data_dir",
    "cookie_secure",
    "local_repos_root",
    "database.url",
    "oidc.issuer",
    "oidc.client_id",
    "oidc.client_secret",
    "scim.token",
    "groups.users",
    "groups.admins",
    "groups.guests",
    "ai.api_key",
    "ai.base_url",
    "ai.model",
    "ai.effort",
    "ai.check_hours",
    "ai.max_turns",
    "ai.triage",
    "github.app_id",
    "github.app_slug",
    "github.private_key",
    "github.private_key_path",
    "github.webhook_secret",
    "github.client_id",
    "github.client_secret",
    "mcp.allowed_hosts",
];

/// What a server for a company can't run without.
pub const REQUIRED: &[&str] = &[
    "company",
    "public_url",
    "oidc.issuer",
    "oidc.client_id",
    "oidc.client_secret",
    "scim.token",
];

pub struct Config {
    /// The company the server runs for, as its people know it: "Sign in
    /// with Acme Inc".
    pub company: String,
    pub database_url: String,
    pub addr: SocketAddr,
    /// Mark session cookies `Secure` (https only). On unless
    /// `cookie_secure = false`, which local development sets.
    pub cookie_secure: bool,
    /// The built web app to serve (`apps/web/dist`); in development Vite serves it.
    pub web_dir: Option<PathBuf>,
    /// Where files live (default `./data`).
    pub data_dir: PathBuf,
}

impl Config {
    pub fn from_settings(settings: &Settings) -> Result<Self, Error> {
        let mut required = REQUIRED.to_vec();
        // The database also comes from DATABASE_URL, as tools around
        // Postgres expect.
        let database_url = settings
            .get("database.url")
            .or_else(|| settings.var("DATABASE_URL"));
        if database_url.is_none() {
            required.push("database.url");
        }
        settings.require(&required)?;
        let addr = match settings.get("addr") {
            Some(addr) => addr
                .parse()
                .map_err(|_| settings.invalid("addr", "is an address and port, as 0.0.0.0:7380"))?,
            None => SocketAddr::from(([127, 0, 0, 1], 7380)),
        };
        Ok(Config {
            company: settings.get("company").unwrap_or_default(),
            database_url: database_url.unwrap_or_default(),
            addr,
            cookie_secure: settings.flag("cookie_secure", true)?,
            web_dir: settings.get("web_dir").map(Into::into),
            data_dir: settings
                .get("data_dir")
                .unwrap_or_else(|| "data".into())
                .into(),
        })
    }
}
