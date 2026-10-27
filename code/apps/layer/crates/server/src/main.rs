//! `layer-server [--config <file>]`: Layer for one company. Its settings
//! are in `layer.toml` (see `layer.example.toml`), with the environment
//! over them.

use std::path::PathBuf;
use std::time::Duration;

use code_villain_scim::Groups;
use layer_server::config::{Config, KNOWN, Settings};
use layer_server::oidc::OidcLogin;
use layer_server::{AppState, company};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: layer-server [--config <file>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The release profile unwinds, for the desktop app's sake (AppKit's
    // exceptions, `Cargo.toml`). A panic here still ends the server, as it
    // did under `panic = "abort"`: unwound, it would end only the task it
    // happened in, and a background task gone while the server still
    // answers is worse than a restart.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report(info);
        std::process::abort();
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "layer_server=debug,tower_http=info".into()),
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
    let settings = Settings::load("layer", KNOWN, file.as_deref())?;
    let config = Config::from_settings(&settings)?;
    let login = OidcLogin::from_settings(&settings)?;
    let scim_token = settings.get("scim.token").unwrap_or_default();
    let groups = Groups::new(
        "layer",
        settings.get("groups.users").as_deref(),
        settings.get("groups.admins").as_deref(),
        settings.get("groups.guests").as_deref(),
    );

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    layer_sync::MIGRATOR.run(&pool).await?;
    company::start(&pool, &config.company, &groups).await?;
    tracing::info!(
        users = groups.users,
        admins = groups.admins,
        guests = groups.guests,
        "Layer for {}: people sign in with its account and come from its groups",
        config.company
    );

    let mut state = AppState::new(pool, config.cookie_secure)
        .for_company(&config.company)
        .with_oidc_login(login)
        .with_scim(scim_token);
    if cfg!(debug_assertions) {
        // `bun run tauri dev` shows the desktop window from Vite, and
        // a server workspace there calls this server (PROF-15).
        state = state.allowing_origin("http://localhost:1420");
    }
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!("listening on http://{}", config.addr);
    layer_server::serve(state, config.web(), listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
