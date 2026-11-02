//! Postrider's rules for the shared sync engine ([`code_villain_sync`]). On
//! this Mac the workspace is yours alone, so the rules are few: accounts
//! come and go through the server, and a new one goes after the others.

use code_villain_sync::Rules;
use postrider_domain::{Actor, ModelKind, Operation};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Postrider, as the sync engine sees it.
pub struct Postrider;

impl Rules for Postrider {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn guest_access(_db: &mut PgConnection, _actor: Actor) -> Result<Option<i64>, SyncError> {
        Ok(None)
    }

    async fn check_allowed(
        _db: &mut PgConnection,
        _actor: Actor,
        _operation: &Operation,
    ) -> Result<(), SyncError> {
        Ok(())
    }

    async fn prepare_create(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        row: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if model == ModelKind::MailAccount && row.get("position").is_none_or(Value::is_null) {
            row.insert("position".into(), json!(next_position(db, actor).await?));
        }
        Ok(())
    }

    async fn cascade(
        _db: &mut PgConnection,
        _actor: Actor,
        _model: ModelKind,
        _id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        // What Postrider keeps of an account's Inbox goes with it, by its
        // foreign key; none of it syncs.
        Ok(Vec::new())
    }

    fn constraint_words(constraint: &str) -> Option<&'static str> {
        Some(match constraint {
            "mail_accounts_workspace_id_address_key" => "Postrider already reads that address.",
            "calendars_workspace_id_key" => "Postrider already reads a calendar server.",
            _ => return None,
        })
    }
}

/// After the workspace's last account.
async fn next_position(db: &mut PgConnection, actor: Actor) -> Result<i32, SyncError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(max(position), 0) + 1 FROM mail_accounts WHERE workspace_id = $1",
    )
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?)
}
