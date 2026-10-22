//! The server's settings: `layer.toml` (`--config`, `LAYER_CONFIG`), with
//! the environment over it (`code/kit/settings`). `layer.example.toml` in
//! the app's folder has them all.

use std::net::SocketAddr;
use std::path::PathBuf;

pub use code_villain_settings::{Error, Settings};

/// Every setting Layer reads, as `section.key`.
pub const KNOWN: &[&str] = &[
    "company",
    "public_url",
    "addr",
    "web_dir",
    "web_dev_url",
    "cookie_secure",
    "database.url",
    "oidc.issuer",
    "oidc.client_id",
    "oidc.client_secret",
    "scim.token",
    "groups.users",
    "groups.admins",
    "groups.guests",
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
    /// The built web app to serve (`dist-web`); in development Vite serves it.
    pub web_dir: Option<PathBuf>,
    /// Without `web_dir`, where the web app's Vite is (`web_dev_url`, which
    /// `bun run dev:server` sets): pages asked of the server are sent on there.
    pub web_dev_url: Option<String>,
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
                .map_err(|_| settings.invalid("addr", "is an address and port, as 0.0.0.0:7680"))?,
            None => SocketAddr::from(([127, 0, 0, 1], 7680)),
        };
        Ok(Config {
            company: settings.get("company").unwrap_or_default(),
            database_url: database_url.unwrap_or_default(),
            addr,
            cookie_secure: settings.flag("cookie_secure", true)?,
            web_dir: settings.get("web_dir").map(Into::into),
            web_dev_url: settings.get("web_dev_url"),
        })
    }

    pub fn web(&self) -> crate::Web<'_> {
        match (&self.web_dir, &self.web_dev_url) {
            (Some(dir), _) => crate::Web::Dir(dir),
            (None, Some(url)) => crate::Web::SentTo(url),
            (None, None) => crate::Web::None,
        }
    }
}
