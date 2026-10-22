//! `forge-server [--config <file>]`: Forge for one company. Its settings
//! are in `forge.toml` (see `forge.example.toml`), with the environment
//! over them.

use std::path::PathBuf;
use std::time::Duration;

use code_villain_scim::Groups;
use forge_ai::{ClaudeProvider, ModelProvider};
use forge_server::config::{Config, KNOWN, Settings};
use forge_server::drift::{self, DriftSettings};
use forge_server::github::GitHubApp;
use forge_server::github::oauth::GitHubOAuth;
use forge_server::oidc::OidcLogin;
use forge_server::{AppState, company};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: forge-server [--config <file>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge_server=debug,tower_http=info".into()),
        )
        .init();

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
    let settings = Settings::load("forge", KNOWN, file.as_deref())?;
    let config = Config::from_settings(&settings)?;
    let login = OidcLogin::from_settings(&settings)?;
    let scim_token = settings.get("scim.token").unwrap_or_default();
    let groups = Groups::new(
        "forge",
        settings.get("groups.users").as_deref(),
        settings.get("groups.admins").as_deref(),
        settings.get("groups.guests").as_deref(),
    );

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    forge_sync::MIGRATOR.run(&pool).await?;
    company::start(&pool, &config.company, &groups).await?;
    tracing::info!(
        users = groups.users,
        admins = groups.admins,
        guests = groups.guests,
        "Forge for {}: people sign in with its account and come from its groups",
        config.company
    );

    let mut state = AppState::new(pool, config.cookie_secure)
        .with_data_dir(&config.data_dir)
        .for_company(&config.company)
        .with_oidc_login(login)
        .with_scim(scim_token)
        .allowing_mcp_hosts(
            settings
                .get("mcp.allowed_hosts")
                .map(|hosts| {
                    hosts
                        .split(',')
                        .map(|h| h.trim().to_string())
                        .filter(|h| !h.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        );
    match ClaudeProvider::from_settings(|name| settings.var(name)) {
        Some(provider) => {
            tracing::info!(model = provider.model(), "drift checks are on");
            state = state.with_ai(
                drift::shared(provider),
                DriftSettings::from_settings(&settings)?,
            );
        }
        None => tracing::info!("drift checks are off: set ai.api_key to turn them on"),
    }
    match GitHubApp::from_settings(&settings) {
        Some(app) => {
            tracing::info!(app = %app.slug, "the GitHub connection is on");
            state = state.with_github(app);
        }
        None => tracing::info!("the GitHub connection is off: set [github] to turn it on"),
    }
    if let Some(oauth) = GitHubOAuth::from_settings(&settings) {
        state = state.with_github_oauth(oauth);
    }
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!("listening on http://{}", config.addr);
    forge_server::serve(state, config.web_dir.as_deref(), listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
