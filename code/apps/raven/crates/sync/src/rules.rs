//! Raven's rules for the shared sync engine ([`code_villain_sync`]): admins
//! choose what a workspace watches, and Raven itself stays watched.

use code_villain_sync::Rules;
use raven_domain::{Actor, ModelKind, Operation};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Raven, as the sync engine sees it.
pub struct Raven;

impl Rules for Raven {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn guest_access(db: &mut PgConnection, actor: Actor) -> Result<Option<i64>, SyncError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT role, access_version FROM workspace_members \
             WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(actor.workspace_id)
        .bind(actor.user_id)
        .fetch_optional(&mut *db)
        .await?;
        let (role, version) = row.ok_or_else(|| SyncError::NotFound("workspace".into()))?;
        Ok((role == "guest").then_some(version))
    }

    async fn check_allowed(
        db: &mut PgConnection,
        actor: Actor,
        operation: &Operation,
    ) -> Result<(), SyncError> {
        check_allowed(db, actor, operation).await
    }

    async fn prepare_create(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        row: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if model == ModelKind::App && row.get("position").is_none_or(Value::is_null) {
            row.insert("position".into(), json!(next_position(db, actor).await?));
        }
        Ok(())
    }

    async fn cascade(
        _db: &mut PgConnection,
        _actor: Actor,
        model: ModelKind,
        _id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        match model {
            // What Raven heard from the app goes with it, by its foreign
            // keys; none of it syncs.
            ModelKind::App => Ok(Vec::new()),
            ModelKind::User => Err(SyncError::Invalid(format!(
                "{} records can't be deleted this way",
                model.name()
            ))),
        }
    }

    fn constraint_words(constraint: &str) -> Option<&'static str> {
        plain_words(constraint)
    }
}

/// Who may make a write from a client. The apps a workspace watches are for
/// its admins. Raven itself can't be removed, and the server checks it at
/// its own address.
async fn check_allowed(
    db: &mut PgConnection,
    actor: Actor,
    operation: &Operation,
) -> Result<(), SyncError> {
    let refuse = |message: &str| Err(SyncError::Forbidden(message.to_string()));
    let (Operation::Create { model, .. }
    | Operation::Update { model, .. }
    | Operation::Delete { model, .. }) = operation;
    if *model != ModelKind::App {
        return Ok(());
    }
    if !is_admin(db, actor).await? {
        return refuse("Only admins can change which apps the workspace watches.");
    }
    match operation {
        Operation::Delete { id, .. } if is_self(db, actor, *id).await? => {
            refuse("Raven itself stays watched.")
        }
        Operation::Update { id, data, .. }
            if data.contains_key("health_url") && is_self(db, actor, *id).await? =>
        {
            refuse("Raven checks itself at its own address.")
        }
        _ => Ok(()),
    }
}

/// Whether the actor is an admin of the workspace.
async fn is_admin(db: &mut PgConnection, actor: Actor) -> Result<bool, SyncError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .fetch_optional(&mut *db)
    .await?;
    Ok(role.as_deref() == Some("admin"))
}

/// Whether the app is Raven itself.
async fn is_self(db: &mut PgConnection, actor: Actor, id: Uuid) -> Result<bool, SyncError> {
    let is_self: Option<bool> =
        sqlx::query_scalar("SELECT is_self FROM apps WHERE id = $1 AND workspace_id = $2")
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_optional(&mut *db)
            .await?;
    Ok(is_self == Some(true))
}

/// After the workspace's last app.
async fn next_position(db: &mut PgConnection, actor: Actor) -> Result<i32, SyncError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(max(position), 0) + 1 FROM apps WHERE workspace_id = $1",
    )
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?)
}

/// What a broken constraint means, for the ones people run into.
fn plain_words(constraint: &str) -> Option<&'static str> {
    Some(match constraint {
        "apps_workspace_id_key_key" => {
            "The workspace already watches an app with that service name."
        }
        _ => return None,
    })
}
