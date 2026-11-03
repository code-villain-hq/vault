use std::collections::{BTreeMap, HashSet};

use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
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
    // Authorization, log records and guest filters describe one snapshot.
    let mut db = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *db)
        .await?;
    let guest = R::guest_access(&mut db, viewer).await?;
    let rows: Vec<LogRow> = sqlx::query_as(
        "SELECT sync_id, transaction_id, model, model_id, action, data, visible_to FROM sync_log \
         WHERE workspace_id = $1 AND sync_id > $2 ORDER BY sync_id LIMIT $3",
    )
    .bind(viewer.workspace_id)
    .bind(after)
    .bind(limit)
    .fetch_all(&mut *db)
    .await?;
    let upto = rows.last().map_or(after, |row| row.sync_id);
    let visible = match guest {
        None => None,
        Some(_) => Some(guest_sees_of::<R::Model>(&mut db, viewer, &rows).await?),
    };
    db.commit().await?;
    Ok((visible_entries(rows, viewer, visible.as_ref())?, upto))
}

/// One entry of `sync_log`, as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct LogRow {
    pub sync_id: i64,
    pub transaction_id: Uuid,
    pub model: String,
    pub model_id: Uuid,
    pub action: String,
    pub data: Option<Value>,
    pub visible_to: Option<Uuid>,
}

/// Of the records `rows` change (not delete), the ones the guest `viewer`
/// may see now. Run it in the snapshot the guest's access was read in.
pub(crate) async fn guest_sees_of<M: Model>(
    db: &mut PgConnection,
    viewer: Actor,
    rows: &[LogRow],
) -> Result<HashSet<(M, Uuid)>, SyncError> {
    let changed = rows
        .iter()
        .filter(|row| row.data.is_some() && row.visible_to.is_none_or(|u| u == viewer.user_id))
        .filter_map(|row| M::from_name(&row.model).map(|model| (model, row.model_id)));
    guest_filter(db, viewer, changed).await
}

/// The entries of `rows` that `viewer` gets: not those private to someone
/// else, and for a guest (`visible` is `Some`) only deletes and the
/// records they may see.
pub(crate) fn visible_entries<M: Model>(
    rows: impl IntoIterator<Item = LogRow>,
    viewer: Actor,
    visible: Option<&HashSet<(M, Uuid)>>,
) -> Result<Vec<SyncEntry<M>>, SyncError> {
    rows.into_iter()
        .filter(|row| row.visible_to.is_none_or(|user| user == viewer.user_id))
        .filter(|row| {
            let Some(visible) = visible else { return true };
            row.data.is_none()
                || M::from_name(&row.model)
                    .is_some_and(|model| visible.contains(&(model, row.model_id)))
        })
        .map(|row| {
            let model = M::from_name(&row.model).ok_or_else(|| {
                SyncError::Invalid(format!("unknown model {:?} in sync_log", row.model))
            })?;
            let action = match row.action.as_str() {
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
                sync_id: row.sync_id,
                transaction_id: row.transaction_id,
                model,
                model_id: row.model_id,
                action,
                data: row.data,
            })
        })
        .collect()
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
