//! `postrider-server`: Postrider for development. It runs as the desktop
//! app's own server does, for the person on this Mac, on the database in
//! `DATABASE_URL`, and writes this launch's session to `.dev-session`,
//! which Vite hands the web app. `POSTRIDER_DEV_PASSWORD` is the password
//! for the stand-in mail and calendar servers (`docker compose`), so
//! nothing goes to the keychain; without it, the keychain is used.

use std::net::SocketAddr;
use std::time::Duration;

use code_villain_claude::{Adapter, Kind};
use postrider_server::ask::{MCP_PATH, POSTRIDER};
use postrider_server::secrets::Secrets;
use postrider_server::{AppState, local};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postrider_server=debug,tower_http=info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let addr: SocketAddr = std::env::var("POSTRIDER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:7880".into())
        .parse()?;
    let secrets = match std::env::var("POSTRIDER_DEV_PASSWORD") {
        Ok(password) if !password.is_empty() => Secrets::Fixed(password),
        _ => Secrets::Keychain,
    };

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;
    postrider_sync::MIGRATOR.run(&pool).await?;

    let person = local::person_or_start(&pool, &local::account_name())
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    let token = local::session(&pool, person)
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    let session_file = std::env::var("POSTRIDER_SESSION_FILE").unwrap_or(".dev-session".into());
    std::fs::write(&session_file, &token)?;

    let mut state = AppState::new(pool, secrets).on_this_mac();
    if let Ok(origin) = std::env::var("POSTRIDER_WEB_ORIGIN") {
        state = state.allowing_origin(origin);
    }
    // Claude Code on this Mac answers what's asked, as you're signed in to
    // it, in an empty folder of its own, with this server's tools.
    let folder = std::env::temp_dir().join("postrider-dev-claude");
    let mcp = format!("http://{addr}{MCP_PATH}");
    state = state.with_claude_code(Adapter::new(POSTRIDER, Kind::ClaudeCode, folder, mcp));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    postrider_server::serve(state, listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
