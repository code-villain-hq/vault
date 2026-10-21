//! What only Forge reads from the change log: an issue's and a spec's history.

use forge_domain::{Actor, Issue, IssueEvent, IssueVersion, SpecVersion, issue_history};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::SyncError;

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
