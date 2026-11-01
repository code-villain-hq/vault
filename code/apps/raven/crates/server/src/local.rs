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

pub use code_villain_auth::THIS_MAC_EMAIL as EMAIL;

/// The person on this Mac, once they've started here.
pub async fn person(pool: &PgPool) -> Result<Option<Uuid>, AppError> {
    Ok(code_villain_auth::this_mac_person(pool).await?)
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
    let user_id = code_villain_auth::ensure_this_mac_person(&mut db, &name).await?;
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
