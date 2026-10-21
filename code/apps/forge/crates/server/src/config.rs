use std::net::SocketAddr;

use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub addr: SocketAddr,
    /// Mark session cookies `Secure` (https only). On unless
    /// `FORGE_COOKIE_SECURE=0`, which local development sets.
    pub cookie_secure: bool,
    /// The built web app to serve (`apps/web/dist`); in development Vite serves it.
    pub web_dir: Option<std::path::PathBuf>,
    /// Where files live (`FORGE_DATA_DIR`, default `./data`).
    pub data_dir: std::path::PathBuf,
    /// Behind a proxy (`FORGE_TRUST_FORWARDED_FOR=1`): clients' addresses,
    /// for limits such as on signing up, come from the `X-Forwarded-For`
    /// header it adds. Only set it when a proxy always sets that header,
    /// or anyone could claim any address.
    pub trust_forwarded_for: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        let addr = std::env::var("FORGE_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:7380".into())
            .parse()
            .context("FORGE_ADDR is not a valid address")?;
        let cookie_secure = std::env::var("FORGE_COOKIE_SECURE").map_or(true, |v| v != "0");
        let web_dir = std::env::var("FORGE_WEB_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(Into::into);
        let data_dir = std::env::var("FORGE_DATA_DIR")
            .ok()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| "data".into())
            .into();
        let trust_forwarded_for =
            std::env::var("FORGE_TRUST_FORWARDED_FOR").is_ok_and(|v| v.trim() == "1");
        Ok(Config {
            database_url,
            addr,
            cookie_secure,
            web_dir,
            data_dir,
            trust_forwarded_for,
        })
    }
}
