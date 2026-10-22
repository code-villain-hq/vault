use std::time::Duration;

use glyph_llm::Models;
use glyph_server::config::Config;
use glyph_server::oidc::OidcLogin;
use glyph_server::{AppState, accounts, translate};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str =
    "usage: glyph-server [serve | reset-password <email> | create-workspace <name> <admin email>]";

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
            let state = match OidcLogin::from_env()? {
                Some(login) => {
                    tracing::info!("signing in with Code Villain is on: passwords are off");
                    state.with_oidc_login(login)
                }
                None => state,
            };
            let state = match std::env::var("GLYPH_SCIM_TOKEN") {
                Ok(token) if !token.trim().is_empty() => {
                    tracing::info!("groups from the Code Villain account are on");
                    state.with_scim(token.trim())
                }
                _ => {
                    tracing::info!(
                        "groups are off: set GLYPH_SCIM_TOKEN to take them from the Code Villain account"
                    );
                    state
                }
            };
            let listener = TcpListener::bind(config.addr).await?;
            tracing::info!("listening on http://{}", config.addr);
            glyph_server::serve(state, config.web_dir.as_deref(), listener, async {
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
