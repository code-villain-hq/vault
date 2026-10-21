use std::net::SocketAddr;
use std::time::Duration;

use raven_server::config::Config;
use raven_server::oidc::OidcLogin;
use raven_server::telemetry::{self, Capture, Recorder};
use raven_server::{AppState, MIGRATOR, accounts, app, auth, health};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const USAGE: &str = "usage: raven-server [serve | reset-password <email> | ingest-key <app>]";

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
    MIGRATOR.run(&pool).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {
            for (app, url) in &config.health_urls {
                sqlx::query("UPDATE apps SET health_url = $2 WHERE key = $1")
                    .bind(app)
                    .bind(url)
                    .execute(&pool)
                    .await?;
            }
            let state = AppState::new(pool.clone(), config.cookie_secure, recorder)
                .trusting_forwarded_for(config.trust_forwarded_for);
            let state = match OidcLogin::from_env()? {
                Some(login) => {
                    tracing::info!("signing in with Code Villain is on: passwords are off");
                    state.with_oidc_login(login)
                }
                None => state,
            };
            tokio::spawn(telemetry::write(pool.clone(), records));
            tokio::spawn(health::watch(pool));
            let listener = TcpListener::bind(config.addr).await?;
            tracing::info!(version, "listening on http://{}", config.addr);
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
        ["ingest-key", app] => {
            let key = auth::new_token();
            let changed = sqlx::query("UPDATE apps SET ingest_key_hash = $2 WHERE key = $1")
                .bind(app)
                .bind(auth::hash_token(&key))
                .execute(&pool)
                .await?
                .rows_affected();
            if changed == 0 {
                anyhow::bail!("Raven doesn't watch an app called {app}");
            }
            println!("{key}");
            eprintln!(
                "{app} sends to Raven with this key from now on; its old key no longer works."
            );
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}
