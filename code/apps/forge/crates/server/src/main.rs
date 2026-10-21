use std::net::SocketAddr;
use std::time::Duration;

use forge_ai::{ClaudeProvider, ModelProvider};
use forge_server::config::Config;
use forge_server::drift::{self, DriftSettings};
use forge_server::github::GitHubApp;
use forge_server::github::login::GitHubLogin;
use forge_server::oidc::OidcLogin;
use forge_server::{AppState, accounts, app, forgejo, runners};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: forge-server [serve | reset-password <email>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge_server=debug,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    forge_sync::MIGRATOR.run(&pool).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {
            let mut state = AppState::new(pool, config.cookie_secure)
                .with_data_dir(&config.data_dir)
                .trusting_forwarded_for(config.trust_forwarded_for);
            match ClaudeProvider::from_env() {
                Some(provider) => {
                    tracing::info!(model = provider.model(), "drift checks are on");
                    state = state.with_ai(drift::shared(provider), DriftSettings::from_env());
                }
                None => {
                    tracing::info!("drift checks are off: set ANTHROPIC_API_KEY to turn them on")
                }
            }
            match GitHubApp::from_env() {
                Some(app) => {
                    tracing::info!(app = %app.slug, "the GitHub connection is on");
                    state = state.with_github(app);
                }
                None => {
                    tracing::info!("the GitHub connection is off: set FORGE_GITHUB_* to turn it on")
                }
            }
            if let Some(login) = GitHubLogin::from_env() {
                tracing::info!("signing in with GitHub is on");
                state = state.with_github_login(login);
            }
            if let Some(login) = OidcLogin::from_env()? {
                tracing::info!(
                    "signing in with Code Villain is on: passwords and GitHub sign-in are off"
                );
                state = state.with_oidc_login(login);
            }
            tokio::spawn(code_villain_sync::listen(
                state.pool.clone(),
                forge_sync::NOTIFY_CHANNEL,
                state.wake.clone(),
            ));
            tokio::spawn(drift::worker(state.clone()));
            tokio::spawn(runners::worker(state.clone()));
            tokio::spawn(forgejo::worker(state.clone()));
            let listener = TcpListener::bind(config.addr).await?;
            tracing::info!("listening on http://{}", config.addr);
            // With each connection's address, for limits per client.
            let service = app(state, config.web_dir.as_deref())
                .into_make_service_with_connect_info::<SocketAddr>();
            axum::serve(listener, service)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
        ["reset-password", email] => {
            let password = accounts::reset_password(&pool, email).await?;
            println!("New password for {email}: {password}");
            println!("They're signed out everywhere and should change it after signing in.");
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}
