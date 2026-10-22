//! `raven-server [--config <file>]`: Raven for one company. Its settings
//! are in `raven.toml` (see `raven.example.toml`), with the environment
//! over them.

use std::path::PathBuf;
use std::time::Duration;

use code_villain_scim::Groups;
use raven_server::config::{Config, KNOWN, Settings};
use raven_server::oidc::OidcLogin;
use raven_server::telemetry::{Capture, Recorder};
use raven_server::{AppState, company};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const USAGE: &str = "usage: raven-server [--config <file>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        // `serve` is how it used to start.
        [] | ["serve"] => None,
        ["--config", file] | ["serve", "--config", file] => Some(PathBuf::from(file)),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let settings = Settings::load("raven", KNOWN, file.as_deref())?;

    // Raven's own spans carry the commit it runs (dev.ts sets it) or its version.
    let version = settings
        .get("version")
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    let (recorder, records) = Recorder::new(&version);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "raven_server=debug,tower_http=info".into()),
            ),
        )
        .with(Capture::new(recorder.clone()))
        .init();

    let config = Config::from_settings(&settings)?;
    let login = OidcLogin::from_settings(&settings)?;
    let scim_token = settings.get("scim.token").unwrap_or_default();
    let groups = Groups::new(
        "raven",
        settings.get("groups.users").as_deref(),
        settings.get("groups.admins").as_deref(),
        settings.get("groups.guests").as_deref(),
    );

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    raven_sync::MIGRATOR.run(&pool).await?;
    company::start(&pool, &config.company, &groups).await?;
    tracing::info!(
        users = groups.users,
        admins = groups.admins,
        guests = groups.guests,
        "Raven for {}: people sign in with its account and come from its groups",
        config.company
    );

    let state = AppState::new(pool, config.cookie_secure, recorder)
        .for_company(&config.company)
        .with_oidc_login(login)
        .with_scim(scim_token);
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!(version, "listening on http://{}", config.addr);
    raven_server::serve(state, config.web_dir.as_deref(), listener, records, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
