use std::time::Duration;

use raven_server::config::Config;
use raven_server::oidc::OidcLogin;
use raven_server::telemetry::{Capture, Recorder};
use raven_server::{AppState, accounts};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const USAGE: &str =
    "usage: raven-server [serve | reset-password <email> | create-workspace <name> <admin email>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Raven's own spans carry the commit it runs (dev.ts sets it) or its version.
    let version = std::env::var("RAVEN_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
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

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    raven_sync::MIGRATOR.run(&pool).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {
            let state = AppState::new(pool.clone(), config.cookie_secure, recorder)
                .trusting_forwarded_for(config.trust_forwarded_for);
            let state = match OidcLogin::from_env()? {
                Some(login) => {
                    tracing::info!("signing in with Code Villain is on: passwords are off");
                    state.with_oidc_login(login)
                }
                None => state,
            };
            let state = match std::env::var("RAVEN_SCIM_TOKEN") {
                Ok(token) if !token.trim().is_empty() => {
                    tracing::info!("groups from the Code Villain account are on");
                    state.with_scim(token.trim())
                }
                _ => {
                    tracing::info!(
                        "groups are off: set RAVEN_SCIM_TOKEN to take them from the Code Villain account"
                    );
                    state
                }
            };
            let listener = TcpListener::bind(config.addr).await?;
            tracing::info!(version, "listening on http://{}", config.addr);
            raven_server::serve(state, config.web_dir.as_deref(), listener, records, async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        }
        ["reset-password", email] => {
            let password = accounts::reset_password(&pool, email).await?;
            println!("New password for {email}: {password}");
            println!("They're signed out everywhere and should change it after signing in.");
        }
        ["create-workspace", name, email] => {
            let url_key = accounts::create_workspace_for(&pool, name, email).await?;
            println!("Made {name} with {email} as its admin: /{url_key}");
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}
