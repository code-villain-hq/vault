//! The server's settings: `glyph.toml` (`--config`, `GLYPH_CONFIG`), with
//! the environment over it (`code/kit/settings`). `glyph.example.toml` in
//! the app's folder has them all.

use std::net::SocketAddr;
use std::path::PathBuf;

pub use code_villain_settings::{Error, Settings};

/// Every setting Glyph reads, as `section.key`.
pub const KNOWN: &[&str] = &[
    "company",
    "public_url",
    "addr",
    "web_dir",
    "cookie_secure",
    "models",
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
    /// The built web app to serve (`apps/web/dist`); in development Vite serves it.
    pub web_dir: Option<PathBuf>,
    /// The models file (`models`, default `./models.toml`): which
    /// translation models there are and how to start them.
    pub models_file: PathBuf,
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
                .map_err(|_| settings.invalid("addr", "is an address and port, as 0.0.0.0:7480"))?,
            None => SocketAddr::from(([127, 0, 0, 1], 7480)),
        };
        Ok(Config {
            company: settings.get("company").unwrap_or_default(),
            database_url: database_url.unwrap_or_default(),
            addr,
            cookie_secure: settings.flag("cookie_secure", true)?,
            web_dir: settings.get("web_dir").map(Into::into),
            models_file: settings
                .get("models")
                .unwrap_or_else(|| "models.toml".into())
                .into(),
        })
    }
}
