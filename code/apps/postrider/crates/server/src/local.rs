//! Workspaces on this Mac: the desktop app runs Postrider itself for one
//! person, who has no password and no email. Only the app's shell (and the
//! dev server) call these; no route leads here, and the app's server takes
//! no sign-ins ([`AppState::on_this_mac`](crate::AppState::on_this_mac)).

use postrider_domain::clean_name;
use sqlx::PgPool;
use uuid::Uuid;

use crate::accounts::create_workspace;
use crate::auth::start_session;
use crate::error::AppError;

pub use code_villain_auth::THIS_MAC_EMAIL as EMAIL;

/// The person on this Mac, once they've started here.
pub async fn person(pool: &PgPool) -> Result<Option<Uuid>, AppError> {
    Ok(code_villain_auth::this_mac_person(pool).await?)
}

/// Starting on this Mac: the person, named after the Mac's account, and
/// their workspace, named after them ("Michael"). Returns the person.
pub async fn start(pool: &PgPool, name: &str) -> Result<Uuid, AppError> {
    let name = clean_name(name, "name").unwrap_or_else(|_| "Me".into());
    let workspace = name.split_whitespace().next().unwrap_or("Me").to_string();
    let mut db = pool.begin().await?;
    let user_id = code_villain_auth::ensure_this_mac_person(&mut db, &name).await?;
    create_workspace(&mut db, &workspace, user_id).await?;
    db.commit().await?;
    Ok(user_id)
}

/// The person on this Mac, started on first use.
pub async fn person_or_start(pool: &PgPool, name: &str) -> Result<Uuid, AppError> {
    match person(pool).await? {
        Some(person) => Ok(person),
        None => start(pool, name).await,
    }
}

/// A session for the app, made fresh on every launch, so the app keeps no
/// token of its own.
pub async fn session(pool: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    start_session(pool, user_id).await
}

/// The Mac account's full name ("Michael Lazarski").
pub fn account_name() -> String {
    std::process::Command::new("id")
        .arg("-F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default()
}
