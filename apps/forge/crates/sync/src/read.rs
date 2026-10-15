use std::collections::BTreeMap;

use forge_domain::{
    Actor, Bootstrap, Issue, IssueEvent, IssueVersion, ModelKind, SpecVersion, SyncAction,
    SyncEntry, issue_history,
};
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

/// An issue's history from the change log, oldest first; `None` if the
/// workspace never had that issue.
pub async fn issue_events(
    pool: &PgPool,
    viewer: Actor,
    issue_id: Uuid,
) -> Result<Option<Vec<IssueEvent>>, SyncError> {
    type Row = (chrono::DateTime<chrono::Utc>, Uuid, String, Option<Value>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT created_at, actor_id, action, data FROM sync_log \
         WHERE model_id = $1 AND workspace_id = $2 AND model = 'Issue' AND visible_to IS NULL \
         ORDER BY sync_id",
    )
    .bind(issue_id)
    .bind(viewer.workspace_id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let versions = rows
        .into_iter()
        .map(|(at, actor_id, action, data)| IssueVersion {
            at,
            actor_id,
            created: action == "insert",
            issue: data.and_then(|d| serde_json::from_value::<Issue>(d).ok()),
        });
    Ok(Some(issue_history(versions)))
}

/// The versions of a spec's text from the change log, oldest first; `None`
/// if the workspace has no such spec. A version saved in the same
/// transaction as a spec change took effect names that spec change: the
/// first one logged after it for that spec, since each is recorded right
/// after the specs it changed, also when several take effect together.
pub async fn spec_versions(
    pool: &PgPool,
    viewer: Actor,
    spec_id: Uuid,
) -> Result<Option<Vec<SpecVersion>>, SyncError> {
    // A deleted spec's versions stay in the log, but it has no history any more.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM specs WHERE id = $1 AND workspace_id = $2)",
    )
    .bind(spec_id)
    .bind(viewer.workspace_id)
    .fetch_one(pool)
    .await?;
    if !exists {
        return Ok(None);
    }
    type Row = (
        chrono::DateTime<chrono::Utc>,
        Uuid,
        String,
        Option<String>,
        Option<Uuid>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT l.created_at, l.actor_id, l.action, l.data ->> 'body', \
         (SELECT c.model_id FROM sync_log c \
          WHERE c.transaction_id = l.transaction_id AND c.model = 'SpecChange' \
          AND c.data ->> 'applied_at' IS NOT NULL \
          AND c.data -> 'applied_spec_ids' ? l.model_id::text \
          AND c.sync_id > l.sync_id ORDER BY c.sync_id LIMIT 1) \
         FROM sync_log l \
         WHERE l.model_id = $1 AND l.workspace_id = $2 AND l.model = 'Spec' \
         AND l.visible_to IS NULL ORDER BY l.sync_id",
    )
    .bind(spec_id)
    .bind(viewer.workspace_id)
    .fetch_all(pool)
    .await?;
    let mut versions: Vec<SpecVersion> = Vec::new();
    for (at, actor_id, action, body, spec_change_id) in rows {
        let Some(body) = body else { continue };
        let created = action == "insert";
        if !created && versions.last().is_some_and(|last| last.body == body) {
            continue;
        }
        versions.push(SpecVersion {
            at,
            actor_id,
            created,
            body,
            spec_change_id,
        });
    }
    Ok(Some(versions))
}
