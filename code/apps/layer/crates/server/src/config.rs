use std::net::SocketAddr;

use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub addr: SocketAddr,
    /// Mark session cookies `Secure` (https only). On unless
    /// `LAYER_COOKIE_SECURE=0`, which local development sets.
    pub cookie_secure: bool,
    /// The built web app to serve (`dist-web`); in development Vite serves it.
    pub web_dir: Option<std::path::PathBuf>,
    /// Without `web_dir`, where the web app's Vite is
    /// (`LAYER_WEB_DEV_URL`, which `bun run dev:server` sets): pages asked
    /// of the server are sent on there.
    pub web_dev_url: Option<String>,
    /// Behind a proxy (`LAYER_TRUST_FORWARDED_FOR=1`): clients' addresses,
    /// for limits such as on signing up, come from the `X-Forwarded-For`
    /// header it adds. Only set it when a proxy always sets that header,
    /// or anyone could claim any address.
    pub trust_forwarded_for: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        let addr: SocketAddr = std::env::var("LAYER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:7680".into())
            .parse()
            .context("LAYER_ADDR is not a valid address")?;
        let cookie_secure = std::env::var("LAYER_COOKIE_SECURE").map_or(true, |v| v != "0");
        let web_dir = std::env::var("LAYER_WEB_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(Into::into);
        let web_dev_url = std::env::var("LAYER_WEB_DEV_URL")
            .ok()
            .filter(|u| !u.is_empty());
        let trust_forwarded_for =
            std::env::var("LAYER_TRUST_FORWARDED_FOR").is_ok_and(|v| v.trim() == "1");
        Ok(Config {
            database_url,
            addr,
            cookie_secure,
            web_dir,
            web_dev_url,
            trust_forwarded_for,
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
