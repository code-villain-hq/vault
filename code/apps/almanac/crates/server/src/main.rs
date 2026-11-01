//! `almanac-server [--config <file>]`: Almanac for one company. Its settings
//! are in `almanac.toml` (see `almanac.example.toml`), with the environment
//! over them.

use std::path::PathBuf;
use std::time::Duration;

use almanac_server::claude::Claude;
use almanac_server::config::{Config, KNOWN, Settings};
use almanac_server::oidc;
use almanac_server::{AppState, company};
use code_villain_scim::Groups;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const USAGE: &str = "usage: almanac-server [--config <file>]";

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
    let settings = Settings::load("almanac", KNOWN, file.as_deref())?;

    // The commit it runs (dev.ts sets it) or its version.
    let version = settings
        .get("version")
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "almanac_server=debug,tower_http=info".into()),
            ),
        )
        .init();

    let config = Config::from_settings(&settings)?;
    let login = oidc::from_settings(&settings)?;
    let scim_token = settings.get("scim.token").unwrap_or_default();
    let groups = Groups::new(
        "almanac",
        settings.get("groups.users").as_deref(),
        settings.get("groups.admins").as_deref(),
        settings.get("groups.guests").as_deref(),
    );

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    almanac_sync::MIGRATOR.run(&pool).await?;
    company::start(&pool, &config.company, &groups).await?;
    tracing::info!(
        users = groups.users,
        admins = groups.admins,
        guests = groups.guests,
        "Almanac for {}: people sign in with its account and come from its groups",
        config.company
    );

    // Settings → Claude holds the key; the settings file's stands in.
    let mut claude = Claude::new(
        settings
            .get("claude.api_key")
            .or_else(|| settings.var("ANTHROPIC_API_KEY")),
    );
    if let Some(url) = settings.get("claude.url") {
        claude = claude.at(url);
    }
    let state = AppState::new(pool, config.cookie_secure, claude)
        .for_company(&config.company)
        .with_oidc_login(login)
        .with_scim(scim_token);
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!(version, "listening on http://{}", config.addr);
    almanac_server::serve(state, config.web_dir.as_deref(), listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
