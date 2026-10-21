use std::time::Duration;

use layer_server::config::Config;
use layer_server::oidc::OidcLogin;
use layer_server::{AppState, accounts};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: layer-server [serve | reset-password <email>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "layer_server=debug,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    layer_sync::MIGRATOR.run(&pool).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {
            let mut state = AppState::new(pool.clone(), config.cookie_secure)
                .trusting_forwarded_for(config.trust_forwarded_for);
            if cfg!(debug_assertions) {
                // `bun run tauri dev` shows the desktop window from Vite, and
                // a server workspace there calls this server (PROF-15).
                state = state.allowing_origin("http://localhost:1420");
            }
            let state = match OidcLogin::from_env()? {
                Some(login) => {
                    tracing::info!("signing in with Code Villain is on: passwords are off");
                    state.with_oidc_login(login)
                }
                None => state,
            };
            let listener = TcpListener::bind(config.addr).await?;
            tracing::info!("listening on http://{}", config.addr);
            layer_server::serve(state, config.web(), listener, async {
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
