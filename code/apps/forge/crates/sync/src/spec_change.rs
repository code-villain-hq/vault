//! An issue's spec change takes effect when the issue is done (decision
//! "Specs change only when the work is done", FOR-74): its pull request is
//! merged, or someone moves it to a completed status. Forge then writes the
//! change into each spec, creates the new ones, and marks the decisions it
//! replaces as superseded, in the same transaction and as whoever finished
//! the issue. The spec change records when, which specs it changed, and the
//! ones that no longer fit it, which stay as they are. The specs' versions
//! saved in that transaction name it in their history (`spec_versions`).

use forge_domain::{
    Actor, ChangedSince, ModelKind, NotApplied, Operation, SpecChange, apply_delta, changed_since,
    new_spec_holds, parse_delta, same_spec_name, spec_conflicts,
};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::SyncError;
use crate::rules::uuid_at;

/// The spec changes of open issues (not done or canceled) that haven't
/// taken effect.
async fn open_changes(
    db: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<Vec<SpecChange>, SyncError> {
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(c.*) FROM spec_changes c \
         JOIN issues i ON i.id = c.issue_id JOIN workflow_states s ON s.id = i.state_id \
         WHERE c.workspace_id = $1 AND c.applied_at IS NULL \
         AND s.type NOT IN ('completed', 'canceled')",
    )
    .bind(workspace_id)
    .fetch_all(&mut *db)
    .await?;
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(row)
                .map_err(|e| SyncError::Invalid(format!("unreadable spec change: {e}")))
        })
        .collect()
}

/// An issue as people refer to it: its identifier, and who it's assigned to.
async fn named(db: &mut PgConnection, issue: Option<Uuid>) -> Result<String, SyncError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT t.key || '-' || i.number, u.name FROM issues i JOIN teams t ON t.id = i.team_id \
         LEFT JOIN users u ON u.id = i.assignee_id WHERE i.id = $1",
    )
    .bind(issue)
    .fetch_optional(&mut *db)
    .await?;
    Ok(match row {
        Some((identifier, Some(name))) => format!("{identifier} ({name})"),
        Some((identifier, None)) => identifier,
        None => "Another issue".into(),
    })
}

async fn spec_title(db: &mut PgConnection, spec: Uuid) -> Result<String, SyncError> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT title FROM specs WHERE id = $1")
            .bind(spec)
            .fetch_optional(&mut *db)
            .await?
            .map_or_else(|| "a spec".into(), |title| format!("the {title} spec")),
    )
}

fn changed_since_sentence(changed: &ChangedSince) -> String {
    match changed {
        ChangedSince::Spec => {
            "The spec changed since this rewrite was written, and it would undo that.".into()
        }
        ChangedSince::Requirements(names) => format!(
            "{} changed since this was written.",
            names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Where an issue's spec change stands against the other open issues' and
/// the specs as they are now.
#[derive(Debug, Default)]
pub struct Standing {
    /// What keeps the issue from starting, in sentences: a requirement
    /// another open issue changes first, or text changed since it was
    /// written. Empty when nothing does.
    pub held_back: Vec<String>,
    /// Open issues that came after it and want to change what it changes.
    pub also_wanted: Vec<String>,
}

/// Where the issue's spec change stands (decision "A spec change is written
/// first, and only one issue changes a requirement at a time").
pub async fn spec_change_standing(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    issue: Uuid,
) -> Result<Standing, SyncError> {
    let mut db = pool.acquire().await?;
    standing(&mut db, workspace_id, issue).await
}

async fn standing(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
) -> Result<Standing, SyncError> {
    let open = open_changes(db, workspace_id).await?;
    let mut standing = Standing::default();
    let Some(mine) = open.iter().find(|c| c.issue_id == Some(issue)) else {
        return Ok(standing);
    };
    let issue_of = |change: Uuid| {
        open.iter()
            .find(|c| c.id == change)
            .and_then(|c| c.issue_id)
    };
    for conflict in spec_conflicts(&open)
        .into_iter()
        .filter(|c| c.first == mine.id || c.second == mine.id)
    {
        let what = match &conflict.requirement {
            Some(name) => format!("\"{name}\" in {}", spec_title(db, conflict.spec_id).await?),
            None => format!(
                "{} (a rewrite changes all of it)",
                spec_title(db, conflict.spec_id).await?
            ),
        };
        if conflict.second == mine.id {
            let first = named(db, issue_of(conflict.first)).await?;
            standing.held_back.push(format!(
                "{first} already changes {what}. Settle it with them before starting: wait until \
                 it's done, drop it from this spec change, combine the two issues, or split the \
                 requirement."
            ));
        } else if conflict.first == mine.id {
            let second = named(db, issue_of(conflict.second)).await?;
            standing
                .also_wanted
                .push(format!("{second} also wants to change {what}."));
        }
    }
    // A new spec another open issue writes too: the one there first writes
    // it, as `save_spec_change` has it; a reopened issue comes after.
    for hold in new_spec_holds(&open)
        .into_iter()
        .filter(|h| h.second == mine.id)
    {
        standing.held_back.push(format!(
            "{} already writes a new spec {:?}. Settle it with them before starting: wait until \
             it's done and then change that spec, or drop it from this spec change.",
            named(db, issue_of(hold.first)).await?,
            hold.title
        ));
    }
    // A spec of that name came about meanwhile: the new one can't be created.
    let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM specs WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_all(&mut *db)
        .await?;
    for title in mine
        .specs
        .iter()
        .filter(|d| d.spec_id.is_none())
        .filter_map(|d| d.new_title.as_deref())
    {
        if titles.iter().any(|t| same_spec_name(t, title)) {
            standing.held_back.push(format!(
                "There's a spec called {:?} now, so its new spec can't be created. Write that \
                 part again as a change to the spec, with `spec`.",
                title.trim()
            ));
        }
    }
    for delta in &mine.specs {
        let Some(spec) = delta.spec_id else { continue };
        let body: Option<String> = sqlx::query_scalar("SELECT body FROM specs WHERE id = $1")
            .bind(spec)
            .fetch_optional(&mut *db)
            .await?;
        if let Some(changed) = body.and_then(|body| changed_since(delta, &body)) {
            standing.held_back.push(format!(
                "Its part for {} was written against older text. {} Write it again against the \
                 spec as it is now.",
                spec_title(db, spec).await?,
                changed_since_sentence(&changed)
            ));
        }
    }
    Ok(standing)
}

/// What an agent is told when it starts a closed issue (done or canceled):
/// it reopens it to a status before work starts first, where its spec
/// change can be written, or joins the queue anew and can be held back.
pub(crate) async fn reopen_first(
    db: &mut PgConnection,
    issue: Uuid,
) -> Result<SyncError, SyncError> {
    let identifier = named(db, Some(issue)).await?;
    Ok(SyncError::Invalid(format!(
        "{identifier} is closed (done or canceled). Reopen it to Todo (or Backlog) first; then it can start once \
         nothing holds its spec change back."
    )))
}

/// Refuses starting an issue that's held back by its spec change.
pub(crate) async fn check_not_held_back(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
) -> Result<(), SyncError> {
    let held = standing(db, workspace_id, issue).await?.held_back;
    if held.is_empty() {
        return Ok(());
    }
    let identifier = named(db, Some(issue)).await?;
    Err(SyncError::Invalid(format!(
        "{identifier} can't start yet. {}",
        held.join(" ")
    )))
}

/// The open issues whose people should hear that the spec change of
/// `issue` now also changes what theirs changed first: those it newly
/// comes second to.
pub(crate) async fn newly_conflicting(
    db: &mut PgConnection,
    workspace_id: Uuid,
    before: Option<&Value>,
    after: &Value,
) -> Result<Vec<Uuid>, SyncError> {
    let Ok(after) = serde_json::from_value::<SpecChange>(after.clone()) else {
        return Ok(vec![]);
    };
    let open: Vec<SpecChange> = open_changes(db, workspace_id)
        .await?
        .into_iter()
        .filter(|c| c.id != after.id)
        .collect();
    let firsts = |open: &[SpecChange], mine: &SpecChange| -> Vec<Uuid> {
        let mut all = open.to_vec();
        all.push(mine.clone());
        let mut found: Vec<Uuid> = spec_conflicts(&all)
            .into_iter()
            .filter(|c| c.second == mine.id)
            .filter_map(|c| {
                all.iter()
                    .find(|o| o.id == c.first)
                    .and_then(|o| o.issue_id)
            })
            .collect();
        found.sort();
        found.dedup();
        found
    };
    let had = match before.and_then(|b| serde_json::from_value::<SpecChange>(b.clone()).ok()) {
        Some(before) => firsts(&open, &before),
        None => vec![],
    };
    let now = firsts(&open, &after);
    Ok(now.into_iter().filter(|i| !had.contains(i)).collect())
}

/// Whether the issue's spec change is written: it changes specs, says why
/// none changes, or took effect already.
pub(crate) async fn is_written(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
) -> Result<bool, SyncError> {
    Ok(sqlx::query_scalar(
        // One that took effect, for an issue reopened since, can't change any more.
        "SELECT EXISTS (SELECT 1 FROM spec_changes WHERE issue_id = $1 AND workspace_id = $2 \
         AND (jsonb_array_length(specs) > 0 OR no_change_reason IS NOT NULL \
           OR applied_at IS NOT NULL))",
    )
    .bind(issue)
    .bind(workspace_id)
    .fetch_one(&mut *db)
    .await?)
}

/// What an agent is told when it starts an issue whose spec change isn't
/// written yet.
pub(crate) async fn write_it_first(
    db: &mut PgConnection,
    workspace_id: Uuid,
    issue: Uuid,
) -> Result<SyncError, SyncError> {
    let identifier: String = sqlx::query_scalar(
        "SELECT t.key || '-' || i.number FROM issues i JOIN teams t ON t.id = i.team_id \
         WHERE i.id = $1 AND i.workspace_id = $2",
    )
    .bind(issue)
    .bind(workspace_id)
    .fetch_optional(&mut *db)
    .await?
    .unwrap_or_else(|| "The issue".into());
    Ok(SyncError::Invalid(format!(
        "{identifier} has no spec change yet. Write it first with save_spec_change: what the issue \
         changes in the specs, or `no_spec_change` and why none changes. Work on an issue starts \
         only once its spec change is written (decision \"A spec change is written first, and \
         only one issue changes a requirement at a time\")."
    )))
}

/// An agent starts an issue only once its spec change is written: it can't
/// move an issue into a started status, or create one there, before that.
/// Forge's own agents, such as the GitHub agent, aren't checked.
pub(crate) async fn check_written_first(
    db: &mut PgConnection,
    actor: Actor,
    operation: &Operation,
) -> Result<(), SyncError> {
    let (id, data, created) = match operation {
        Operation::Create {
            model: ModelKind::Issue,
            id,
            data,
        } => (*id, data, true),
        Operation::Update {
            model: ModelKind::Issue,
            id,
            data,
        } => (*id, data, false),
        _ => return Ok(()),
    };
    let Some(state) = data.get("state_id").and_then(Value::as_str) else {
        return Ok(());
    };
    let Ok(state) = state.parse::<Uuid>() else {
        return Ok(());
    };
    let (agent, starts, closed, ours): (bool, bool, bool, bool) = sqlx::query_as(
        // Agents added under Settings → Agents do work; Forge's own (the
        // GitHub agent, Forge AI) only move issues along.
        "SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND kind = 'agent' \
           AND email LIKE 'agent-%'), \
         COALESCE((SELECT type = 'started' FROM workflow_states \
           WHERE id = $2 AND workspace_id = $4), false) \
         AND NOT COALESCE((SELECT s.type = 'started' FROM issues i \
           JOIN workflow_states s ON s.id = i.state_id \
           WHERE i.id = $3 AND i.workspace_id = $4), false), \
         COALESCE((SELECT s.type IN ('completed', 'canceled') FROM issues i \
           JOIN workflow_states s ON s.id = i.state_id \
           WHERE i.id = $3 AND i.workspace_id = $4), false), \
         EXISTS (SELECT 1 FROM issues WHERE id = $3 AND workspace_id = $4)",
    )
    .bind(actor.user_id)
    .bind(state)
    .bind(id)
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?;
    // Another workspace's issue isn't found when it's written, and isn't named here.
    if !agent || !starts || (!created && !ours) {
        return Ok(());
    }
    if closed {
        return Err(reopen_first(db, id).await?);
    }
    if created {
        return Err(SyncError::Invalid(
            "Create the issue in a status before work starts, write its spec change with \
             save_spec_change, then start it."
                .into(),
        ));
    }
    if !is_written(db, actor.workspace_id, id).await? {
        return Err(write_it_first(db, actor.workspace_id, id).await?);
    }
    check_not_held_back(db, actor.workspace_id, id).await
}

/// What to record on the issue's spec change when the issue (`before` and
/// `after` an update) starts: what it says then, so a change afterwards
/// shows. Going back before work started, such as to Todo or Triage, clears
/// it, and starting again records it anew.
pub(crate) async fn mark_started(
    db: &mut PgConnection,
    actor: Actor,
    before: &Value,
    after: &Value,
) -> Result<Vec<Operation>, SyncError> {
    let (old_state, new_state) = (uuid_at(before, "state_id"), uuid_at(after, "state_id"));
    if old_state == new_state {
        return Ok(vec![]);
    }
    let (was, is): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT (SELECT type FROM workflow_states WHERE id = $1), \
         (SELECT type FROM workflow_states WHERE id = $2)",
    )
    .bind(old_state)
    .bind(new_state)
    .fetch_one(&mut *db)
    .await?;
    let (was_started, starts) = (
        was.as_deref() == Some("started"),
        is.as_deref() == Some("started"),
    );
    // Back before work starts, also by way of Canceled.
    let back = matches!(is.as_deref(), Some("triage" | "backlog" | "unstarted"));
    if !(starts && !was_started) && !back {
        return Ok(vec![]);
    }
    // Its id, what it says (specs, why none, decisions it replaces), and
    // whether what it said at the start is kept.
    type Row = (Uuid, Value, Option<String>, Vec<Uuid>, bool);
    let change: Option<Row> = sqlx::query_as(
        "SELECT id, specs, no_change_reason, replaces_decision_ids, as_started IS NOT NULL \
         FROM spec_changes \
         WHERE issue_id = $1 AND workspace_id = $2 AND applied_at IS NULL",
    )
    .bind(uuid_at(after, "id"))
    .bind(actor.workspace_id)
    .fetch_optional(&mut *db)
    .await?;
    let Some((id, specs, reason, replaces, recorded)) = change else {
        return Ok(vec![]);
    };
    let as_started = if back {
        if !recorded {
            return Ok(vec![]);
        }
        Value::Null
    } else {
        if recorded {
            return Ok(vec![]);
        }
        json!({ "specs": specs, "no_change_reason": reason, "replaces_decision_ids": replaces })
    };
    Ok(vec![Operation::Update {
        model: ModelKind::SpecChange,
        id,
        data: Map::from_iter([("as_started".to_string(), as_started)]),
    }])
}

/// What to change on the issue's spec change when the issue comes back
/// from Canceled: its parts join the queue anew, behind issues that changed
/// the same requirements while it was canceled.
pub(crate) async fn rejoin(
    db: &mut PgConnection,
    actor: Actor,
    before: &Value,
    after: &Value,
) -> Result<Vec<Operation>, SyncError> {
    let (old_state, new_state) = (uuid_at(before, "state_id"), uuid_at(after, "state_id"));
    if old_state == new_state {
        return Ok(vec![]);
    }
    let (was, is): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT (SELECT type FROM workflow_states WHERE id = $1), \
         (SELECT type FROM workflow_states WHERE id = $2)",
    )
    .bind(old_state)
    .bind(new_state)
    .fetch_one(&mut *db)
    .await?;
    if was.as_deref() != Some("canceled") || matches!(is.as_deref(), Some("canceled" | "completed"))
    {
        return Ok(vec![]);
    }
    let change: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, specs FROM spec_changes \
         WHERE issue_id = $1 AND workspace_id = $2 AND applied_at IS NULL",
    )
    .bind(uuid_at(after, "id"))
    .bind(actor.workspace_id)
    .fetch_optional(&mut *db)
    .await?;
    let Some((id, Value::Array(mut specs))) = change else {
        return Ok(vec![]);
    };
    let now = json!(chrono::Utc::now());
    for part in &mut specs {
        part["written_at"] = now.clone();
        if let Some(Value::Object(touched)) = part.get_mut("touched_at") {
            for at in touched.values_mut() {
                *at = now.clone();
            }
        }
    }
    Ok(vec![Operation::Update {
        model: ModelKind::SpecChange,
        id,
        data: Map::from_iter([("specs".to_string(), Value::Array(specs))]),
    }])
}

/// The changes that make the issue's spec change take effect, if the issue
/// (`before` and `after` an update) has just been done and has one that
/// hasn't yet.
pub(crate) async fn take_effect(
    db: &mut PgConnection,
    actor: Actor,
    before: &Value,
    after: &Value,
) -> Result<Vec<Operation>, SyncError> {
    let (old_state, new_state) = (uuid_at(before, "state_id"), uuid_at(after, "state_id"));
    if old_state == new_state {
        return Ok(vec![]);
    }
    let (was, is): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT (SELECT type FROM workflow_states WHERE id = $1), \
         (SELECT type FROM workflow_states WHERE id = $2)",
    )
    .bind(old_state)
    .bind(new_state)
    .fetch_one(&mut *db)
    .await?;
    if is.as_deref() != Some("completed") || was.as_deref() == Some("completed") {
        return Ok(vec![]);
    }
    let change: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(c.*) FROM spec_changes c \
         WHERE issue_id = $1 AND workspace_id = $2 AND applied_at IS NULL",
    )
    .bind(uuid_at(after, "id"))
    .bind(actor.workspace_id)
    .fetch_optional(&mut *db)
    .await?;
    let Some(change) = change else {
        return Ok(vec![]);
    };
    let change: SpecChange = serde_json::from_value(change)
        .map_err(|e| SyncError::Invalid(format!("unreadable spec change: {e}")))?;

    let mut operations = Vec::new();
    let mut applied: Vec<Uuid> = Vec::new();
    let mut not_applied: Vec<NotApplied> = Vec::new();
    for delta in &change.specs {
        let (operation, title) = match delta.spec_id {
            Some(spec_id) => {
                let spec: Option<(String, String)> = sqlx::query_as(
                    "SELECT title, body FROM specs WHERE id = $1 AND workspace_id = $2",
                )
                .bind(spec_id)
                .bind(actor.workspace_id)
                .fetch_optional(&mut *db)
                .await?;
                // Deleting a spec takes it out of spec changes, so it's there.
                let Some((title, body)) = spec else { continue };
                let new_body = if delta.rewrite {
                    Ok(delta.text.clone())
                } else {
                    parse_delta(&delta.text).and_then(|parsed| apply_delta(&body, &parsed))
                }
                // Written against older text: taking effect would undo what
                // changed since. What no longer fits at all says so first.
                .and_then(|new| match changed_since(delta, &body) {
                    Some(changed) => Err(vec![changed_since_sentence(&changed)]),
                    None => Ok(new),
                });
                let operation = new_body.and_then(|body| {
                    let data = Map::from_iter([("body".to_string(), json!(body))]);
                    ModelKind::Spec
                        .validate_patch(&data)
                        .map_err(|e| vec![format!("The new text can't be saved: {e}.")])?;
                    Ok((
                        spec_id,
                        Operation::Update {
                            model: ModelKind::Spec,
                            id: spec_id,
                            data,
                        },
                    ))
                });
                (operation, title)
            }
            None => {
                let title = delta.new_title.clone().unwrap_or_default();
                let taken: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM specs \
                     WHERE workspace_id = $1 AND lower(trim(title)) = lower(trim($2)))",
                )
                .bind(actor.workspace_id)
                .bind(&title)
                .fetch_one(&mut *db)
                .await?;
                let data = Map::from_iter([
                    ("title".to_string(), json!(title.trim())),
                    ("body".to_string(), json!(delta.text)),
                    ("status".to_string(), json!("active")),
                    (
                        "service_ids".to_string(),
                        json!(delta.new_service_id.into_iter().collect::<Vec<_>>()),
                    ),
                ]);
                let operation = if taken {
                    Err(vec![format!(
                        "There's already a spec called \"{}\".",
                        title.trim()
                    )])
                } else {
                    ModelKind::Spec
                        .validate_create(&data)
                        .map_err(|e| vec![format!("It can't be saved: {e}.")])
                        .map(|()| {
                            let id = Uuid::now_v7();
                            (
                                id,
                                Operation::Create {
                                    model: ModelKind::Spec,
                                    id,
                                    data,
                                },
                            )
                        })
                };
                (operation, title)
            }
        };
        match operation {
            Ok((id, operation)) => {
                applied.push(id);
                operations.push(operation);
            }
            Err(problems) => not_applied.push(NotApplied {
                spec_id: delta.spec_id,
                title,
                reason: problems.join(" "),
            }),
        }
    }

    let replaced: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM decisions \
         WHERE id = ANY($1) AND workspace_id = $2 AND status <> 'superseded'",
    )
    .bind(&change.replaces_decision_ids)
    .bind(actor.workspace_id)
    .fetch_all(&mut *db)
    .await?;
    operations.extend(replaced.into_iter().map(|id| Operation::Update {
        model: ModelKind::Decision,
        id,
        data: Map::from_iter([("status".to_string(), json!("superseded"))]),
    }));

    operations.push(Operation::Update {
        model: ModelKind::SpecChange,
        id: change.id,
        data: Map::from_iter([
            ("applied_at".to_string(), json!(chrono::Utc::now())),
            ("applied_spec_ids".to_string(), json!(applied)),
            ("not_applied".to_string(), json!(not_applied)),
        ]),
    });
    Ok(operations)
}
