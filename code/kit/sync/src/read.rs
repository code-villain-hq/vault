use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use crate::scope::{condition, guest_filter, served_epoch};
use crate::{Actor, Bootstrap, GuestView, Model, Rules, SyncAction, SyncEntry, SyncError};

/// A consistent snapshot of every record the viewer syncs in the workspace,
/// and the sync id it corresponds to. A guest gets only what
/// [`Model::guest_view`] lets through, in an epoch of their own.
pub async fn bootstrap<R: Rules>(
    pool: &PgPool,
    viewer: Actor,
) -> Result<Bootstrap<R::Model>, SyncError> {
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
    let guest = R::guest_access(&mut db, viewer).await?;
    let epoch = served_epoch(epoch, guest);
    let sync_id: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sync_id), 0) FROM sync_log WHERE workspace_id = $1",
    )
    .bind(viewer.workspace_id)
    .fetch_one(&mut *db)
    .await?;

    let mut records = BTreeMap::new();
    for model in <R::Model as Model>::ALL {
        let mut only = String::new();
        if model.private_to_user() {
            only.push_str(" AND t.user_id = $2");
        }
        if guest.is_some() {
            match model.guest_view() {
                GuestView::All => {}
                GuestView::Where(sql) => only.push_str(&format!(" AND {}", condition(sql))),
                GuestView::Hidden => {
                    records.insert(*model, Vec::new());
                    continue;
                }
            }
        }
        let sql = format!(
            "SELECT to_jsonb(t.*) FROM {} AS t WHERE t.workspace_id = $1{only} ORDER BY t.created_at",
            model.table()
        );
        // Safe: the table name and the conditions are constants from the app's `Model`.
        let mut query = sqlx::query_scalar(AssertSqlSafe(sql)).bind(viewer.workspace_id);
        if !only.is_empty() {
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
///
/// A guest gets the entries for records they may see as they are now, and
/// every delete: an id they never had is nothing to their client. A record
/// that comes into their view later arrives with their next bootstrap,
/// since what changes their view changes their epoch.
pub async fn entries_after<R: Rules>(
    pool: &PgPool,
    viewer: Actor,
    after: i64,
    limit: i64,
) -> Result<(Vec<SyncEntry<R::Model>>, i64), SyncError> {
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
    let rows: Vec<Row> = rows
        .into_iter()
        .filter(|row| row.6.is_none_or(|user| user == viewer.user_id))
        .collect();

    let mut db = pool.acquire().await?;
    let visible = match R::guest_access(&mut db, viewer).await? {
        None => None,
        Some(_) => {
            let changed = rows.iter().filter(|row| row.5.is_some()).filter_map(|row| {
                <R::Model as Model>::from_name(&row.2).map(|model| (model, row.3))
            });
            Some(guest_filter(&mut db, viewer, changed).await?)
        }
    };
    drop(db);

    let entries = rows
        .into_iter()
        .filter(|row| {
            let Some(visible) = &visible else { return true };
            row.5.is_none()
                || <R::Model as Model>::from_name(&row.2)
                    .is_some_and(|model| visible.contains(&(model, row.3)))
        })
        .map(
            |(sync_id, transaction_id, model, model_id, action, data, _)| {
                let model = <R::Model as Model>::from_name(&model).ok_or_else(|| {
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
