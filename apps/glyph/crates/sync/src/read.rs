use std::collections::BTreeMap;

use glyph_domain::{Actor, Bootstrap, ModelKind, SyncAction, SyncEntry};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use crate::SyncError;

/// A consistent snapshot of every record the viewer syncs in the workspace,
/// and the sync id it corresponds to.
pub async fn bootstrap(pool: &PgPool, viewer: Actor) -> Result<Bootstrap, SyncError> {
    let mut db = pool.begin().await?;
    // One snapshot for all the reads below, so records and sync id agree.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *db)
        .await?;

    let epoch: Uuid = sqlx::query_scalar("SELECT sync_epoch FROM workspaces WHERE id = $1")
        .bind(viewer.workspace_id)
        .fetch_optional(&mut *db)
        .await?
        .ok_or_else(|| SyncError::NotFound("workspace".into()))?;
    let sync_id: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sync_id), 0) FROM sync_log WHERE workspace_id = $1",
    )
    .bind(viewer.workspace_id)
    .fetch_one(&mut *db)
    .await?;

    let mut records = BTreeMap::new();
    for model in ModelKind::ALL {
        let mine = if model.private_to_user() {
            " AND t.user_id = $2"
        } else {
            ""
        };
        let sql = format!(
            "SELECT to_jsonb(t.*) FROM {} AS t WHERE t.workspace_id = $1{mine} ORDER BY t.created_at",
            model.table()
        );
        // Safe: the table name is a constant from `ModelKind`.
        let mut query = sqlx::query_scalar(AssertSqlSafe(sql)).bind(viewer.workspace_id);
        if model.private_to_user() {
            query = query.bind(viewer.user_id);
        }
        let rows: Vec<Value> = query.fetch_all(&mut *db).await?;
        records.insert(*model, rows);
    }

    db.commit().await?;
    Ok(Bootstrap {
        epoch,
        sync_id,
        records,
    })
}

/// Changes after `after` that the viewer may see, oldest first, from a page
/// of at most `limit` log entries. Also returns the last sync id looked at,
/// which can be past the last visible entry: entries meant for someone else
/// still move a client's position forward.
pub async fn entries_after(
    pool: &PgPool,
    viewer: Actor,
    after: i64,
    limit: i64,
) -> Result<(Vec<SyncEntry>, i64), SyncError> {
    type Row = (i64, Uuid, String, Uuid, String, Option<Value>, Option<Uuid>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT sync_id, transaction_id, model, model_id, action, data, visible_to FROM sync_log \
         WHERE workspace_id = $1 AND sync_id > $2 ORDER BY sync_id LIMIT $3",
    )
    .bind(viewer.workspace_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let upto = rows.last().map_or(after, |row| row.0);
    let entries = rows
        .into_iter()
        .filter(|row| row.6.is_none_or(|user| user == viewer.user_id))
        .map(
            |(sync_id, transaction_id, model, model_id, action, data, _)| {
                let model = ModelKind::from_name(&model).ok_or_else(|| {
                    SyncError::Invalid(format!("unknown model {model:?} in sync_log"))
                })?;
                let action = match action.as_str() {
                    "insert" => SyncAction::Insert,
                    "update" => SyncAction::Update,
                    "delete" => SyncAction::Delete,
                    other => {
                        return Err(SyncError::Invalid(format!(
                            "unknown action {other:?} in sync_log"
                        )));
                    }
                };
                Ok(SyncEntry {
                    sync_id,
                    transaction_id,
                    model,
                    model_id,
                    action,
                    data,
                })
            },
        )
        .collect::<Result<_, _>>()?;
    Ok((entries, upto))
}

/// The workspace's latest sync id, or 0 if nothing has changed yet.
pub async fn current_sync_id(pool: &PgPool, workspace_id: Uuid) -> Result<i64, SyncError> {
    Ok(
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(sync_id), 0) FROM sync_log WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(pool)
        .await?,
    )
}

pub async fn workspace_epoch(pool: &PgPool, workspace_id: Uuid) -> Result<Uuid, SyncError> {
    sqlx::query_scalar("SELECT sync_epoch FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SyncError::NotFound("workspace".into()))
}
