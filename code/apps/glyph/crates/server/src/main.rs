//! `glyph-server [--config <file>]`: Glyph for one company. Its settings
//! are in `glyph.toml` (see `glyph.example.toml`), with the environment
//! over them.

use std::path::PathBuf;
use std::time::Duration;

use code_villain_scim::Groups;
use glyph_llm::Models;
use glyph_server::config::{Config, KNOWN, Settings};
use glyph_server::oidc::OidcLogin;
use glyph_server::{AppState, company, translate};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: glyph-server [--config <file>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "glyph_server=debug,tower_http=info".into()),
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
    let settings = Settings::load("glyph", KNOWN, file.as_deref())?;
    let config = Config::from_settings(&settings)?;
    let login = OidcLogin::from_settings(&settings)?;
    let scim_token = settings.get("scim.token").unwrap_or_default();
    let groups = Groups::new(
        "glyph",
        settings.get("groups.users").as_deref(),
        settings.get("groups.admins").as_deref(),
        settings.get("groups.guests").as_deref(),
    );
    let models = Models::load(&config.models_file)?;
    match models.models.len() {
        0 => tracing::warn!(
            file = %config.models_file.display(),
            "no translation models; see models.example.toml"
        ),
        n => tracing::info!(file = %config.models_file.display(), "{n} translation model(s)"),
    }

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    glyph_sync::MIGRATOR.run(&pool).await?;
    company::start(&pool, &config.company, &groups).await?;
    tracing::info!(
        users = groups.users,
        admins = groups.admins,
        guests = groups.guests,
        "Glyph for {}: people sign in with its account and come from its groups",
        config.company
    );
    translate::fail_interrupted(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("couldn't tidy up interrupted runs: {e:?}"))?;

    let state = AppState::new(pool, config.cookie_secure)
        .for_company(&config.company)
        .with_oidc_login(login)
        .with_scim(scim_token)
        .with_models(models);
    let listener = TcpListener::bind(config.addr).await?;
    tracing::info!("listening on http://{}", config.addr);
    glyph_server::serve(state, config.web_dir.as_deref(), listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
