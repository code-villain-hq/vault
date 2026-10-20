use std::net::SocketAddr;
use std::time::Duration;

use glyph_llm::Models;
use glyph_server::config::Config;
use glyph_server::{AppState, accounts, app, models, notify, translate};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: glyph-server [serve | reset-password <email>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "glyph_server=debug,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    glyph_sync::MIGRATOR.run(&pool).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {
            let models = Models::load(&config.models_file)?;
            match models.models.len() {
                0 => tracing::warn!(
                    file = %config.models_file.display(),
                    "no translation models; see models.example.toml"
                ),
                n => {
                    tracing::info!(file = %config.models_file.display(), "{n} translation model(s)")
                }
            }
            translate::fail_interrupted(&pool)
                .await
                .map_err(|e| anyhow::anyhow!("couldn't tidy up interrupted runs: {e:?}"))?;
            let state = AppState::new(pool, config.cookie_secure)
                .trusting_forwarded_for(config.trust_forwarded_for)
                .with_models(models);
            let processes = state.processes.clone();
            tokio::spawn(notify::listen(state.pool.clone(), state.wake.clone()));
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
            processes.stop_all(models::GRACE).await;
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
