use std::net::SocketAddr;

use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub addr: SocketAddr,
    /// Mark session cookies `Secure` (https only). On unless
    /// `RAVEN_COOKIE_SECURE=0`, which local development sets.
    pub cookie_secure: bool,
    /// The built web app to serve (`apps/web/dist`); in development Vite serves it.
    pub web_dir: Option<std::path::PathBuf>,
    /// Behind a proxy (`RAVEN_TRUST_FORWARDED_FOR=1`): clients' addresses,
    /// for limits such as on signing up, come from the `X-Forwarded-For`
    /// header it adds. Only set it when a proxy always sets that header,
    /// or anyone could claim any address.
    pub trust_forwarded_for: bool,
    /// Where each watched app answers its health check, from
    /// `RAVEN_<APP>_HEALTH_URL`; apps without one keep the address they have.
    pub health_urls: Vec<(String, String)>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        let addr: SocketAddr = std::env::var("RAVEN_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:7580".into())
            .parse()
            .context("RAVEN_ADDR is not a valid address")?;
        let cookie_secure = std::env::var("RAVEN_COOKIE_SECURE").map_or(true, |v| v != "0");
        let web_dir = std::env::var("RAVEN_WEB_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(Into::into);
        let trust_forwarded_for =
            std::env::var("RAVEN_TRUST_FORWARDED_FOR").is_ok_and(|v| v.trim() == "1");
        let mut health_urls: Vec<(String, String)> = ["forge", "glyph"]
            .into_iter()
            .filter_map(|app| {
                let var = format!("RAVEN_{}_HEALTH_URL", app.to_uppercase());
                let url = std::env::var(var).ok().filter(|u| !u.trim().is_empty())?;
                Some((app.to_string(), url.trim().to_string()))
            })
            .collect();
        // Raven checks itself the way it checks the others.
        health_urls.push(("raven".into(), format!("http://{addr}/api/health")));
        Ok(Config {
            database_url,
            addr,
            cookie_secure,
            web_dir,
            trust_forwarded_for,
            health_urls,
        })
    }
}
