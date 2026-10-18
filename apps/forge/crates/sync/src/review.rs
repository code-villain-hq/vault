//! An issue's reviewer: the person asked to review it. The server fills one
//! in when the issue enters review without one, and tells the reviewer it's
//! ready ("Reviewer filled in at review" and "Ready for review").

use forge_domain::{AGENT_EMAIL_DOMAIN, Actor};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::SyncError;
use crate::apply::uuid_at;

/// A team's review status: the last of its started statuses when it has
/// several, as `forge_domain::review_state` has it.
pub(crate) async fn review_state(
    db: &mut PgConnection,
    team_id: Uuid,
) -> Result<Option<Uuid>, SyncError> {
    let started: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workflow_states WHERE team_id = $1 AND type = 'started' ORDER BY position",
    )
    .bind(team_id)
    .fetch_all(&mut *db)
    .await?;
    Ok(if started.len() < 2 {
        None
    } else {
        started.last().copied()
    })
}

/// Before a write to an issue: when it takes the issue into review and
/// leaves it without a reviewer, makes the person who last accepted it out
/// of Triage its reviewer (the actor, when this write takes it straight out
/// of Triage), or else its creator if a person created it. `before` is the
/// issue as it was (`None` when it's being created) and `data` the fields
/// written.
pub(crate) async fn fill_in(
    db: &mut PgConnection,
    actor: Actor,
    issue_id: Uuid,
    before: Option<&Value>,
    data: &mut Map<String, Value>,
) -> Result<(), SyncError> {
    let field = |name: &str| match data.get(name) {
        Some(value) => value.as_str().and_then(|v| v.parse::<Uuid>().ok()),
        None => before.and_then(|b| uuid_at(b, name)),
    };
    let (Some(team), Some(state)) = (field("team_id"), field("state_id")) else {
        return Ok(());
    };
    if field("reviewer_id").is_some()
        || before.and_then(|b| uuid_at(b, "state_id")) == Some(state)
        || review_state(db, team).await? != Some(state)
    {
        return Ok(());
    }
    let from_triage = match before.and_then(|b| uuid_at(b, "state_id")) {
        Some(from) => sqlx::query_scalar::<_, bool>(
            "SELECT type = 'triage' FROM workflow_states WHERE id = $1",
        )
        .bind(from)
        .fetch_optional(&mut *db)
        .await?
        .unwrap_or(false),
        None => false,
    };
    let reviewer = if from_triage && is_person(db, actor.workspace_id, actor.user_id).await? {
        Some(actor.user_id)
    } else {
        let creator = field("created_by");
        fallback(db, actor.workspace_id, issue_id, creator).await?
    };
    if let Some(reviewer) = reviewer {
        data.insert("reviewer_id".into(), json!(reviewer));
    }
    Ok(())
}

/// The person who last moved the issue out of a triage status, or else its
/// creator when that's a person; only people still in the workspace.
async fn fallback(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue_id: Uuid,
    creator: Option<Uuid>,
) -> Result<Option<Uuid>, SyncError> {
    let accepted: Option<Uuid> = sqlx::query_scalar(
        "SELECT v.actor_id FROM ( \
             SELECT sync_id, actor_id, data->>'state_id' AS state, \
                    lag(data->>'state_id') OVER (ORDER BY sync_id) AS previous \
             FROM sync_log WHERE workspace_id = $1 AND model = 'Issue' AND model_id = $2 \
             AND data IS NOT NULL \
         ) v \
         JOIN workflow_states p ON p.id::text = v.previous AND p.type = 'triage' \
         JOIN workflow_states n ON n.id::text = v.state AND n.type <> 'triage' \
         JOIN users u ON u.id = v.actor_id AND u.kind = 'person' \
         JOIN workspace_members m ON m.user_id = u.id AND m.workspace_id = $1 \
         ORDER BY v.sync_id DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(issue_id)
    .fetch_optional(&mut *db)
    .await?;
    if accepted.is_some() {
        return Ok(accepted);
    }
    Ok(match creator {
        Some(creator) if is_person(db, workspace_id, creator).await? => Some(creator),
        _ => None,
    })
}

/// Whether someone is a person (not an agent) in the workspace.
async fn is_person(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user: Uuid,
) -> Result<bool, SyncError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM users u JOIN workspace_members m ON m.user_id = u.id \
         AND m.workspace_id = $1 WHERE u.id = $2 AND u.kind = 'person')",
    )
    .bind(workspace_id)
    .bind(user)
    .fetch_one(&mut *db)
    .await?)
}

/// Who hears that an issue is ready for review after a change: its
/// reviewer when it enters review (or, without one, the admins who are
/// people), or a new reviewer while it's in review. Never whoever made the
/// change, nor, unless they're newly its reviewer or it was sent back since,
/// anyone with an unread review notification for it already.
pub(crate) async fn to_notify(
    db: &mut PgConnection,
    workspace_id: Uuid,
    actor: Uuid,
    before: Option<&Value>,
    after: &Value,
) -> Result<Vec<Uuid>, SyncError> {
    let (Some(issue), Some(team), Some(state)) = (
        uuid_at(after, "id"),
        uuid_at(after, "team_id"),
        uuid_at(after, "state_id"),
    ) else {
        return Ok(vec![]);
    };
    let reviewer = uuid_at(after, "reviewer_id");
    let moved = before.and_then(|b| uuid_at(b, "state_id")) != Some(state);
    let new_reviewer = before.and_then(|b| uuid_at(b, "reviewer_id")) != reviewer;
    if !moved && !new_reviewer {
        return Ok(vec![]);
    }
    if review_state(db, team).await? != Some(state) {
        return Ok(vec![]);
    }
    let mut people = match reviewer {
        Some(reviewer) => vec![reviewer],
        // Changing the reviewer to nobody tells nobody.
        None if !moved => vec![],
        None => {
            sqlx::query_scalar(
                "SELECT m.user_id FROM workspace_members m JOIN users u ON u.id = m.user_id \
                 WHERE m.workspace_id = $1 AND m.role = 'admin' AND u.kind = 'person'",
            )
            .bind(workspace_id)
            .fetch_all(&mut *db)
            .await?
        }
    };
    people.retain(|p| *p != actor);
    // A new reviewer is asked, read or not, and so is the reviewer when it was
    // sent back and resubmitted. Only Forge bouncing it (for conflicts or
    // failing checks) and returning it waits for the unread one.
    if (new_reviewer && reviewer.is_some())
        || (moved && sent_back(db, workspace_id, issue, state).await?)
    {
        return Ok(people);
    }
    let waiting: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM notifications WHERE issue_id = $1 AND type = 'review' \
         AND read_at IS NULL AND user_id = ANY($2)",
    )
    .bind(issue)
    .bind(&people)
    .fetch_all(&mut *db)
    .await?;
    people.retain(|p| !waiting.contains(p));
    Ok(people)
}

/// Whether the issue last left review because someone sent it back, not
/// because Forge did: Forge's own agents (`<name>@<workspace>.agents.forge.invalid`)
/// move it back when its pull request runs into conflicts or failing checks.
async fn sent_back(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
    review: Uuid,
) -> Result<bool, SyncError> {
    let by_forge: Option<bool> = sqlx::query_scalar(
        "SELECT u.email LIKE $4 FROM ( \
             SELECT sync_id, actor_id, data->>'state_id' AS state, \
                    lag(data->>'state_id') OVER (ORDER BY sync_id) AS previous \
             FROM sync_log WHERE workspace_id = $1 AND model = 'Issue' AND model_id = $2 \
             AND data IS NOT NULL \
         ) v JOIN users u ON u.id = v.actor_id \
         WHERE v.previous = $3::text AND v.state <> $3::text \
         ORDER BY v.sync_id DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(issue)
    .bind(review)
    .bind(format!("%@{workspace_id}.{AGENT_EMAIL_DOMAIN}"))
    .fetch_optional(&mut *db)
    .await?;
    Ok(by_forge == Some(false))
}
