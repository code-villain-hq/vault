//! Workspaces on this Mac: the desktop app runs Raven itself for one
//! person, who has no password and no email. Only the app's shell calls
//! these; no route leads here, and the app's server takes no sign-ins
//! ([`AppState::on_this_mac`](crate::AppState::on_this_mac)).

use raven_domain::{Actor, ModelKind, Operation, Transaction, clean_name};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::accounts::create_workspace;
use crate::auth::start_session;
use crate::error::AppError;

/// The local person's address: never a real one (`.invalid` can't be
/// registered), so it can't be taken for anyone's.
pub const EMAIL: &str = "me@this-mac.invalid";

/// The person on this Mac, once they've started here.
pub async fn person(pool: &PgPool) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(EMAIL)
        .fetch_optional(pool)
        .await?)
}

/// The apps on this Mac Raven watches from the start: Forge's and Glyph's
/// desktop apps, and their dev servers. Raven itself comes with the first
/// workspace.
const THIS_MAC: [(&str, &str, &str); 4] = [
    ("forge", "Forge", "http://127.0.0.1:7385/api/health"),
    ("glyph", "Glyph", "http://127.0.0.1:7485/api/health"),
    ("forge-dev", "Forge dev", "http://127.0.0.1:7380/api/health"),
    ("glyph-dev", "Glyph dev", "http://127.0.0.1:7480/api/health"),
];

/// Starting on this Mac: the person, named once, and their first workspace,
/// which they're admin of and which watches the apps on this Mac. Returns
/// the person.
pub async fn start(pool: &PgPool, name: &str, workspace: &str) -> Result<Uuid, AppError> {
    let name = clean_name(name, "name")?;
    let workspace = clean_name(workspace, "workspace name")?;
    let mut db = pool.begin().await?;
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, email, name) VALUES ($1, $2, $3)
         ON CONFLICT (email) DO UPDATE SET name = users.name RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(EMAIL)
    .bind(&name)
    .fetch_one(&mut *db)
    .await?;
    let workspace_id = create_workspace(&mut db, &workspace, user_id).await?;
    watch_this_mac(
        &mut db,
        Actor {
            user_id,
            workspace_id,
        },
    )
    .await?;
    db.commit().await?;
    Ok(user_id)
}

/// Another workspace on this Mac, empty, as a sign-up's.
pub async fn new_workspace(pool: &PgPool, user_id: Uuid, name: &str) -> Result<Uuid, AppError> {
    let name = clean_name(name, "workspace name")?;
    let mut db = pool.begin().await?;
    let workspace_id = create_workspace(&mut db, &name, user_id).await?;
    db.commit().await?;
    Ok(workspace_id)
}

/// A session for the app, made fresh on every launch, so the app keeps no
/// token of its own.
pub async fn session(pool: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    start_session(pool, user_id).await
}

async fn watch_this_mac(db: &mut PgConnection, actor: Actor) -> Result<(), AppError> {
    let operations = THIS_MAC
        .iter()
        .map(|(key, name, health_url)| Operation::Create {
            model: ModelKind::App,
            id: Uuid::now_v7(),
            data: serde_json::Map::from_iter([
                ("key".into(), json!(key)),
                ("name".into(), json!(name)),
                ("health_url".into(), json!(health_url)),
            ]),
        })
        .collect();
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    raven_sync::apply_in(db, actor, &transaction, raven_sync::Origin::Server).await?;
    Ok(())
}
