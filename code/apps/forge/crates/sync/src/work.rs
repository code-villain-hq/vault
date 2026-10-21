//! What agents are working on (FOR-24). An agent says when it starts and
//! finishes an issue, and any call it makes shows it's still at it. Its user
//! record carries all three, so everyone sees them live.
//!
//! The work also ends when the issue moves on without the agent saying so:
//! to review, done or canceled, out of the started statuses, or to someone
//! else. An hour without a sign of life means the run stopped; clients show
//! that from `working_seen_at`, with the same [`WORK_LEASE_MINUTES`].

use forge_domain::{Actor, ModelKind, SyncAction};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::SyncError;
use crate::apply::{lock_workspace, log, notify, uuid_at};
use crate::spec_change;

/// How long an agent can go without a call before its work counts as
/// stopped. A long build makes no calls, so it's generous.
pub const WORK_LEASE_MINUTES: i32 = 60;

/// Signs of life are recorded at most this often.
const RENEW_EVERY_MINUTES: i32 = 1;

/// The agent (`actor`) starts on an issue. Starting again on the same issue
/// keeps its start time, unless the run had stopped.
///
/// Refused (FOR-42) while the agent is still at another issue, so an
/// overlapping run stops instead of doubling up, and while someone else is
/// at this one. Work whose run stopped doesn't count. Also refused until
/// the issue's spec change is written, and while it's held back: another
/// open issue changes the same requirement first, or what it changes has
/// changed since it was written.
pub async fn start_work(pool: &PgPool, actor: Actor, issue: Uuid) -> Result<(), SyncError> {
    let mut db = pool.begin().await?;
    lock_workspace(&mut db, actor.workspace_id).await?;
    let closed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM issues i JOIN workflow_states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.workspace_id = $2 AND s.type IN ('completed', 'canceled'))",
    )
    .bind(issue)
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?;
    if closed {
        return Err(spec_change::reopen_first(&mut db, issue).await?);
    }
    let busy_with: Option<String> = sqlx::query_scalar(
        "SELECT t.key || '-' || i.number FROM workspace_members m \
         JOIN issues i ON i.id = m.working_on_id JOIN teams t ON t.id = i.team_id \
         WHERE m.workspace_id = $1 AND m.user_id = $2 AND m.working_on_id <> $3 \
         AND m.working_seen_at > now() - make_interval(mins => $4)",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(issue)
    .bind(WORK_LEASE_MINUTES)
    .fetch_optional(&mut *db)
    .await?;
    if let Some(other) = busy_with {
        return Err(SyncError::Conflict(format!(
            "You're still working on {other}. If another run of yours is on it, stop here; \
             otherwise finish {other} first (finish_work, or hand it over)."
        )));
    }
    let taken_by: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, t.key || '-' || i.number FROM workspace_users u \
         JOIN issues i ON i.id = u.working_on_id JOIN teams t ON t.id = i.team_id \
         WHERE u.workspace_id = $1 AND u.id <> $2 AND u.working_on_id = $3 \
         AND u.working_seen_at > now() - make_interval(mins => $4) \
         ORDER BY u.working_since LIMIT 1",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(issue)
    .bind(WORK_LEASE_MINUTES)
    .fetch_optional(&mut *db)
    .await?;
    if let Some((name, identifier)) = taken_by {
        return Err(SyncError::Conflict(format!(
            "{name} is already working on {identifier}. It's free once {name} finishes, \
             hands it over, or has gone an hour without a sign of life."
        )));
    }
    if !spec_change::is_written(&mut db, actor.workspace_id, issue).await? {
        return Err(spec_change::write_it_first(&mut db, actor.workspace_id, issue).await?);
    }
    spec_change::check_not_held_back(&mut db, actor.workspace_id, issue).await?;
    sqlx::query(
        "UPDATE workspace_members SET \
           working_since = CASE WHEN working_on_id = $3 \
             AND working_seen_at > now() - make_interval(mins => $4) \
             THEN working_since ELSE now() END, \
           working_on_id = $3, working_seen_at = now() \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(issue)
    .bind(WORK_LEASE_MINUTES)
    .execute(&mut *db)
    .await?;
    // It took up the latest wake-up, so the next can go out once it's free.
    sqlx::query(
        "UPDATE agent_runners SET woken_at = NULL WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .execute(&mut *db)
    .await?;
    log_user(&mut db, actor, Uuid::now_v7(), actor.user_id).await?;
    notify(&mut db, actor.workspace_id).await?;
    db.commit().await?;
    Ok(())
}

/// The agent (`actor`) is done with an issue. Returns false, changing
/// nothing, if it wasn't working on it.
pub async fn finish_work(pool: &PgPool, actor: Actor, issue: Uuid) -> Result<bool, SyncError> {
    let mut db = pool.begin().await?;
    lock_workspace(&mut db, actor.workspace_id).await?;
    let ended = clear(
        &mut db,
        actor.workspace_id,
        issue,
        Some(actor.user_id),
        None,
    )
    .await?;
    if ended.is_empty() {
        return Ok(false);
    }
    log_user(&mut db, actor, Uuid::now_v7(), actor.user_id).await?;
    notify(&mut db, actor.workspace_id).await?;
    db.commit().await?;
    Ok(true)
}

/// Records that the agent (`actor`) is still at its work, when it's working
/// on something and hasn't stopped. A run that stopped stays stopped until
/// the agent starts again.
pub async fn renew_work(pool: &PgPool, actor: Actor) -> Result<(), SyncError> {
    // Most calls have nothing to record; find out without locking anything.
    let due: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members WHERE workspace_id = $1 AND user_id = $2 \
         AND working_on_id IS NOT NULL \
         AND working_seen_at > now() - make_interval(mins => $3) \
         AND working_seen_at < now() - make_interval(mins => $4))",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(WORK_LEASE_MINUTES)
    .bind(RENEW_EVERY_MINUTES)
    .fetch_one(pool)
    .await?;
    if !due {
        return Ok(());
    }
    let mut db = pool.begin().await?;
    lock_workspace(&mut db, actor.workspace_id).await?;
    let renewed = sqlx::query(
        "UPDATE workspace_members SET working_seen_at = now() \
         WHERE workspace_id = $1 AND user_id = $2 AND working_on_id IS NOT NULL \
         AND working_seen_at > now() - make_interval(mins => $3) \
         AND working_seen_at < now() - make_interval(mins => $4)",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .bind(WORK_LEASE_MINUTES)
    .bind(RENEW_EVERY_MINUTES)
    .execute(&mut *db)
    .await?
    .rows_affected();
    if renewed > 0 {
        log_user(&mut db, actor, Uuid::now_v7(), actor.user_id).await?;
        notify(&mut db, actor.workspace_id).await?;
    }
    db.commit().await?;
    Ok(())
}

/// Ends agents' work on an issue that a change moved on: to review, done or
/// canceled, out of the started statuses, or to someone else. Returns the
/// sync id of the last change, if any. Runs inside `apply`, which holds the
/// workspace lock.
pub(crate) async fn end_moved_on_work(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    before: &Value,
    after: &Value,
) -> Result<Option<i64>, SyncError> {
    let Some(issue) = uuid_at(after, "id") else {
        return Ok(None);
    };
    let worked_on: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members WHERE working_on_id = $1)",
    )
    .bind(issue)
    .fetch_one(&mut *db)
    .await?;
    if !worked_on {
        return Ok(None);
    }

    let new_state = uuid_at(after, "state_id");
    let moved_on = match new_state {
        Some(state) if uuid_at(before, "state_id") != new_state => {
            // Out of the started statuses, or into review: the last started
            // one, for a team with more than one (as the review bar has it).
            sqlx::query_scalar(
                "SELECT s.type <> 'started' OR ( \
                   s.id = (SELECT w.id FROM workflow_states w WHERE w.team_id = s.team_id \
                           AND w.type = 'started' ORDER BY w.position DESC LIMIT 1) \
                   AND (SELECT count(*) FROM workflow_states w WHERE w.team_id = s.team_id \
                        AND w.type = 'started') > 1) \
                 FROM workflow_states s WHERE s.id = $1",
            )
            .bind(state)
            .fetch_optional(&mut *db)
            .await?
            .unwrap_or(false)
        }
        _ => false,
    };
    let assignee = uuid_at(after, "assignee_id");
    let reassigned = uuid_at(before, "assignee_id") != assignee;

    let ended = if moved_on || (reassigned && assignee.is_none()) {
        clear(db, actor.workspace_id, issue, None, None).await?
    } else if reassigned {
        clear(db, actor.workspace_id, issue, None, assignee).await?
    } else {
        Vec::new()
    };
    end(db, actor, transaction_id, ended).await
}

/// Ends all work on an issue about to be deleted. Runs inside `apply`.
pub(crate) async fn end_work_on_deleted(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    issue: Uuid,
) -> Result<Option<i64>, SyncError> {
    let ended = clear(db, actor.workspace_id, issue, None, None).await?;
    end(db, actor, transaction_id, ended).await
}

async fn end(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    agents: Vec<Uuid>,
) -> Result<Option<i64>, SyncError> {
    let mut last = None;
    for agent in agents {
        last = Some(log_user(db, actor, transaction_id, agent).await?);
    }
    Ok(last)
}

/// Clears the work on `issue` of `only` (or everyone working on it), except
/// `except`, and returns whose it cleared.
async fn clear(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
    only: Option<Uuid>,
    except: Option<Uuid>,
) -> Result<Vec<Uuid>, SyncError> {
    Ok(sqlx::query_scalar(
        "UPDATE workspace_members SET working_on_id = NULL, working_since = NULL, working_seen_at = NULL \
         WHERE workspace_id = $1 AND working_on_id = $2 \
         AND ($3::uuid IS NULL OR user_id = $3) AND ($4::uuid IS NULL OR user_id <> $4) \
         RETURNING user_id",
    )
    .bind(workspace_id)
    .bind(issue)
    .bind(only)
    .bind(except)
    .fetch_all(&mut *db)
    .await?)
}

/// Tells clients a member's record changed.
async fn log_user(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    user_id: Uuid,
) -> Result<i64, SyncError> {
    let data: Value = sqlx::query_scalar(
        "SELECT to_jsonb(u.*) FROM workspace_users u WHERE u.id = $1 AND u.workspace_id = $2",
    )
    .bind(user_id)
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?;
    log(
        db,
        actor,
        transaction_id,
        ModelKind::User,
        user_id,
        SyncAction::Update,
        Some(data),
        None,
    )
    .await
}
