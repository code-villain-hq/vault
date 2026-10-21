//! Forge's rules for the shared sync engine ([`code_villain_sync`]): who may
//! change what, the fields only the server sets, and what else a change does,
//! such as notifying people, waking agents and spec changes taking effect.

use code_villain_sync::{Origin, Rules, apply_operation, insert, log};
use forge_domain::{Actor, ModelKind, Operation, SyncAction, Write};
use serde_json::{Map, Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError, review, spec_change, work};

/// Forge, as the sync engine sees it.
///
/// Its server's own writes ([`Origin::Server`]), such as cascades, spec
/// changes taking effect and the GitHub agent's, skip [`allowed`]. Some
/// writes are only for the server: creating teams (with their workflow).
pub struct Forge;

impl Rules for Forge {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn check_allowed(
        db: &mut PgConnection,
        actor: Actor,
        operation: &Operation,
    ) -> Result<(), SyncError> {
        let (model, write, id) = match operation {
            Operation::Create { model, id, .. } => (*model, Write::Create, *id),
            Operation::Update { model, id, .. } => (*model, Write::Update, *id),
            Operation::Delete { model, id } => (*model, Write::Delete, *id),
        };
        allowed(db, actor, model, write, id).await?;
        spec_change::check_written_first(db, actor, operation).await
    }

    async fn prepare_create(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        row: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        add_server_fields(db, actor, model, row).await
    }

    async fn prepare_update(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
        before: Option<&Value>,
        data: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if model == ModelKind::Issue {
            review::fill_in(db, actor, id, before, data).await?;
        }
        Ok(())
    }

    async fn check_rules(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
    ) -> Result<(), SyncError> {
        check_rules(db, actor, model, id).await
    }

    async fn after_write(
        db: &mut PgConnection,
        actor: Actor,
        transaction_id: Uuid,
        model: ModelKind,
        before: Option<&Value>,
        after: &Value,
    ) -> Result<Option<i64>, SyncError> {
        let mut last = notify_people(db, actor, transaction_id, model, before, after).await?;
        let (ModelKind::Issue, Some(before)) = (model, before) else {
            return Ok(last);
        };
        if let Some(sync_id) =
            work::end_moved_on_work(db, actor, transaction_id, before, after).await?
        {
            last = Some(sync_id);
        }
        // Started, or back before that: what its spec change said then.
        let mut effects = spec_change::rejoin(db, actor, before, after).await?;
        effects.extend(spec_change::mark_started(db, actor, before, after).await?);
        for effect in effects {
            if let Some(sync_id) =
                apply_operation::<Forge>(db, actor, transaction_id, &effect, Origin::Server).await?
            {
                last = Some(sync_id);
            }
        }
        // Done: its spec change takes effect.
        for effect in spec_change::take_effect(db, actor, before, after).await? {
            if let Some(sync_id) =
                apply_operation::<Forge>(db, actor, transaction_id, &effect, Origin::Server).await?
            {
                last = Some(sync_id);
            }
        }
        Ok(last)
    }

    async fn cascade(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        cascade(db, actor, model, id).await
    }

    async fn before_delete(
        db: &mut PgConnection,
        actor: Actor,
        transaction_id: Uuid,
        model: ModelKind,
        id: Uuid,
    ) -> Result<(), SyncError> {
        if model == ModelKind::Issue {
            work::end_work_on_deleted(db, actor, transaction_id, id).await?;
        }
        Ok(())
    }
}

pub(crate) fn uuid_at(record: &Value, field: &str) -> Option<Uuid> {
    record
        .get(field)
        .and_then(Value::as_str)
        .and_then(|id| id.parse().ok())
}

/// A notification to create: recipient, type, issue, comment, suggestion.
type NotificationTo = (Uuid, &'static str, Option<Uuid>, Option<Uuid>, Option<Uuid>);

/// An agent to wake: the agent, why, and the issue.
type WakeUp = (Uuid, &'static str, Option<Uuid>);

/// Creates notifications for the people a change concerns, wakes the agents
/// it concerns, and returns the sync id of the last notification.
async fn notify_people(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    model: ModelKind,
    before: Option<&Value>,
    after: &Value,
) -> Result<Option<i64>, SyncError> {
    let mut notifications: Vec<NotificationTo> = Vec::new();
    let mut wakes: Vec<WakeUp> = Vec::new();
    match model {
        ModelKind::Issue => {
            let issue = uuid_at(after, "id");
            for person in newly_mentioned(db, actor, before, after, "description").await? {
                notifications.push((person, "mention", issue, None, None));
                wakes.push((person, "mention", issue));
            }
            let assignee = uuid_at(after, "assignee_id");
            if let Some(assignee) = assignee
                && assignee != actor.user_id
                && before.and_then(|b| uuid_at(b, "assignee_id")) != Some(assignee)
            {
                notifications.push((assignee, "assigned", issue, None, None));
                wakes.push((assignee, "assigned", issue));
            }
            for person in
                review::to_notify(db, actor.workspace_id, actor.user_id, before, after).await?
            {
                notifications.push((person, "review", issue, None, None));
            }

            let (old_state, new_state) = (
                before.and_then(|b| uuid_at(b, "state_id")),
                uuid_at(after, "state_id"),
            );
            if let Some(old_state) = old_state
                && Some(old_state) != new_state
            {
                let states: Vec<(Uuid, String, f64)> = sqlx::query_as(
                    "SELECT id, type, position FROM workflow_states WHERE id = ANY($1)",
                )
                .bind(vec![old_state, new_state.unwrap_or(old_state)])
                .fetch_all(&mut *db)
                .await?;
                let state = |id: Option<Uuid>| states.iter().find(|(s, ..)| Some(*s) == id);
                let type_of = |id: Option<Uuid>| state(id).map(|(_, t, _)| t.as_str());
                let position_of = |id: Option<Uuid>| state(id).map(|(.., p)| *p);
                if type_of(new_state) == Some("completed")
                    && type_of(Some(old_state)) != Some("completed")
                    && let Some(creator) = uuid_at(after, "created_by")
                    && creator != actor.user_id
                {
                    notifications.push((creator, "completed", issue, None, None));
                }
                // Sent back: from a started status to an earlier one, such as
                // from In Review to In Progress.
                if let Some(assignee) = assignee
                    && assignee != actor.user_id
                    && type_of(Some(old_state)) == Some("started")
                    && matches!(type_of(new_state), Some("started" | "unstarted"))
                    && position_of(new_state) < position_of(Some(old_state))
                    && let Some(event) = sent_back_because(db, assignee, issue).await?
                {
                    wakes.push((assignee, event, issue));
                }
                // Accepted out of Triage into work that's planned (Todo) or
                // started: the agent can go on, e.g. after asking for a
                // decision. Not into Backlog, which isn't scheduled yet.
                if let Some(assignee) = assignee
                    && assignee != actor.user_id
                    && type_of(Some(old_state)) == Some("triage")
                    && matches!(type_of(new_state), Some("unstarted" | "started"))
                {
                    wakes.push((assignee, "accepted", issue));
                }
            }
        }
        ModelKind::Comment => {
            let issue = uuid_at(after, "issue_id");
            let comment = uuid_at(after, "id");
            let mentioned = newly_mentioned(db, actor, before, after, "body").await?;
            for person in &mentioned {
                notifications.push((*person, "mention", issue, comment, None));
                wakes.push((*person, "mention", issue));
            }
            if before.is_some() {
                // Edits only notify people newly mentioned.
                wake_agents(db, actor, wakes).await?;
                return finish_notifying(db, actor, transaction_id, notifications).await;
            }
            let assignee: Option<Uuid> =
                sqlx::query_scalar("SELECT assignee_id FROM issues WHERE id = $1")
                    .bind(issue)
                    .fetch_optional(&mut *db)
                    .await?
                    .flatten();
            if let Some(assignee) = assignee
                && assignee != actor.user_id
            {
                wakes.push((assignee, "comment", issue));
            }
            // Everyone involved in the issue: its creator, assignee, reviewer
            // and commenters.
            let people: Vec<Uuid> = sqlx::query_scalar(
                "SELECT created_by FROM issues WHERE id = $1 \
                 UNION SELECT assignee_id FROM issues WHERE id = $1 AND assignee_id IS NOT NULL \
                 UNION SELECT reviewer_id FROM issues WHERE id = $1 AND reviewer_id IS NOT NULL \
                 UNION SELECT author_id FROM comments WHERE issue_id = $1",
            )
            .bind(issue)
            .fetch_all(&mut *db)
            .await?;
            // Those mentioned already hear about it.
            for person in people
                .into_iter()
                .filter(|p| *p != actor.user_id && !mentioned.contains(p))
            {
                notifications.push((person, "comment", issue, comment, None));
            }
        }
        // An issue's spec change now changes a requirement other open
        // issues changed first: their people hear about it.
        ModelKind::SpecChange => {
            let issue = uuid_at(after, "issue_id");
            let firsts =
                spec_change::newly_conflicting(db, actor.workspace_id, before, after).await?;
            // Once each, however many of their issues it now comes after.
            let people: Vec<Uuid> = sqlx::query_scalar(
                "SELECT created_by FROM issues WHERE id = ANY($1) \
                 UNION SELECT assignee_id FROM issues WHERE id = ANY($1) AND assignee_id IS NOT NULL",
            )
            .bind(&firsts)
            .fetch_all(&mut *db)
            .await?;
            for person in people.into_iter().filter(|p| *p != actor.user_id) {
                notifications.push((person, "spec_conflict", issue, None, None));
            }
        }
        // A pull request that works on an issue ran into conflicts or
        // failing checks: the agent assigned to the issue fixes it.
        ModelKind::IssueLink
            if blocks_merging(after)
                && !before.is_some_and(blocks_merging)
                && after["state"] == "open"
                && after["works_on"] == true =>
        {
            let issue = uuid_at(after, "issue_id");
            let assignee: Option<Uuid> =
                sqlx::query_scalar("SELECT assignee_id FROM issues WHERE id = $1")
                    .bind(issue)
                    .fetch_optional(&mut *db)
                    .await?
                    .flatten();
            if let Some(assignee) = assignee
                && assignee != actor.user_id
            {
                wakes.push((assignee, "pr_blocked", issue));
            }
        }
        // Triage suggestions show in Triage, next to the issue, instead.
        ModelKind::Suggestion if before.is_none() && after["kind"] != "triage" => {
            // Whoever the issue concerns; otherwise the workspace's admins.
            let issue = uuid_at(after, "issue_id");
            let mut people: Vec<Uuid> = sqlx::query_scalar(
                // People still in the workspace: someone who left can't be told,
                // and shouldn't keep the admins from being told instead.
                "SELECT u.id FROM issues i JOIN users u ON u.id IN (i.created_by, i.assignee_id) \
                 JOIN workspace_members m ON m.user_id = u.id AND m.workspace_id = i.workspace_id \
                 WHERE i.id = $1 AND u.kind = 'person'",
            )
            .bind(issue)
            .fetch_all(&mut *db)
            .await?;
            if people.is_empty() {
                people = sqlx::query_scalar(
                    "SELECT m.user_id FROM workspace_members m JOIN users u ON u.id = m.user_id \
                     WHERE m.workspace_id = $1 AND m.role = 'admin' AND u.kind = 'person'",
                )
                .bind(actor.workspace_id)
                .fetch_all(&mut *db)
                .await?;
            }
            people.dedup();
            for person in people.into_iter().filter(|p| *p != actor.user_id) {
                notifications.push((person, "suggestion", issue, None, uuid_at(after, "id")));
            }
        }
        _ => {}
    }
    wake_agents(db, actor, wakes).await?;
    finish_notifying(db, actor, transaction_id, notifications).await
}

/// Whether a pull request link says it can't be merged until someone fixes
/// it: conflicts or failing checks. Checks still running only wait.
fn blocks_merging(link: &Value) -> bool {
    matches!(
        link["blocker"].as_str(),
        Some("conflicts" | "checks_failed")
    )
}

/// Why an issue sent back wakes its agent: `pr_blocked` when a pull request
/// that works on it ran into conflicts or failing checks (which is why
/// Forge sends it back), unless such a wake-up is already waiting; otherwise
/// `sent_back`.
async fn sent_back_because(
    db: &mut PgConnection,
    agent: Uuid,
    issue: Option<Uuid>,
) -> Result<Option<&'static str>, SyncError> {
    let (blocked, waiting): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM issue_links WHERE issue_id = $1 AND kind = 'pull_request' \
         AND state = 'open' AND works_on AND blocker IN ('conflicts', 'checks_failed')), \
         EXISTS (SELECT 1 FROM agent_wakeups WHERE issue_id = $1 AND user_id = $2 \
         AND event = 'pr_blocked' AND status = 'pending')",
    )
    .bind(issue)
    .bind(agent)
    .fetch_one(&mut *db)
    .await?;
    Ok(match (blocked, waiting) {
        (false, _) => Some("sent_back"),
        (true, false) => Some("pr_blocked"),
        (true, true) => None,
    })
}

/// Queues a wake-up for each agent with a runner, which the server then
/// delivers. Once per agent and issue: the first reason wins. Members
/// without a runner (people, too) are skipped.
async fn wake_agents(
    db: &mut PgConnection,
    actor: Actor,
    wakes: Vec<WakeUp>,
) -> Result<(), SyncError> {
    let mut woken: Vec<(Uuid, Uuid)> = Vec::new();
    for (agent, event, issue) in wakes {
        let Some(issue) = issue else { continue };
        if woken.contains(&(agent, issue)) {
            continue;
        }
        woken.push((agent, issue));
        sqlx::query(
            "INSERT INTO agent_wakeups (id, workspace_id, user_id, event, issue_id) \
             SELECT $1, workspace_id, user_id, $4, $5 FROM agent_runners \
             WHERE workspace_id = $2 AND user_id = $3",
        )
        .bind(Uuid::now_v7())
        .bind(actor.workspace_id)
        .bind(agent)
        .bind(event)
        .bind(issue)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

/// Members (people and agents) mentioned in a text field after a change but
/// not before it, other than whoever made the change.
async fn newly_mentioned(
    db: &mut PgConnection,
    actor: Actor,
    before: Option<&Value>,
    after: &Value,
    field: &str,
) -> Result<Vec<Uuid>, SyncError> {
    let text = after[field].as_str().unwrap_or("");
    let previous = before.and_then(|b| b[field].as_str()).unwrap_or("");
    if !text.contains('@') || text == previous {
        return Ok(vec![]);
    }
    let people: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, u.name FROM workspace_members m JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1",
    )
    .bind(actor.workspace_id)
    .fetch_all(&mut *db)
    .await?;
    let already = forge_domain::mentions(previous, &people);
    Ok(forge_domain::mentions(text, &people)
        .into_iter()
        .filter(|p| *p != actor.user_id && !already.contains(p))
        .collect())
}

/// Creates the notifications for members still in the workspace, and
/// returns the sync id of the last one.
async fn finish_notifying(
    db: &mut PgConnection,
    actor: Actor,
    transaction_id: Uuid,
    notifications: Vec<NotificationTo>,
) -> Result<Option<i64>, SyncError> {
    if notifications.is_empty() {
        return Ok(None);
    }

    // Only people still in the workspace.
    let recipients: Vec<Uuid> = notifications.iter().map(|n| n.0).collect();
    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM workspace_members WHERE workspace_id = $1 AND user_id = ANY($2)",
    )
    .bind(actor.workspace_id)
    .bind(&recipients)
    .fetch_all(&mut *db)
    .await?;

    let mut last = None;
    for (user, kind, issue, comment, suggestion) in
        notifications.into_iter().filter(|n| members.contains(&n.0))
    {
        let id = Uuid::now_v7();
        let row = Map::from_iter([
            ("id".to_string(), json!(id)),
            ("workspace_id".to_string(), json!(actor.workspace_id)),
            ("user_id".to_string(), json!(user)),
            ("type".to_string(), json!(kind)),
            ("issue_id".to_string(), json!(issue)),
            ("comment_id".to_string(), json!(comment)),
            ("suggestion_id".to_string(), json!(suggestion)),
            ("actor_id".to_string(), json!(actor.user_id)),
        ]);
        let record = insert::<Forge>(db, ModelKind::Notification, row).await?;
        last = Some(
            log(
                db,
                actor,
                transaction_id,
                ModelKind::Notification,
                id,
                SyncAction::Insert,
                Some(record),
                Some(user),
            )
            .await?,
        );
    }
    Ok(last)
}

/// Fills in fields only the server may set when a record is created.
async fn add_server_fields(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    row: &mut Map<String, Value>,
) -> Result<(), SyncError> {
    match model {
        ModelKind::Issue => {
            let team_id = uuid_field(row, "team_id")?;
            let number: Option<i32> = sqlx::query_scalar(
                "UPDATE teams SET issue_counter = issue_counter + 1 \
                 WHERE id = $1 AND workspace_id = $2 RETURNING issue_counter",
            )
            .bind(team_id)
            .bind(actor.workspace_id)
            .fetch_optional(&mut *db)
            .await?;
            let number = number.ok_or_else(|| SyncError::Invalid("team not found".into()))?;
            row.insert("number".into(), number.into());
            row.insert("created_by".into(), json!(actor.user_id));

            if row.get("state_id").is_none_or(Value::is_null) {
                let state: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM workflow_states WHERE team_id = $1 AND type IN ('backlog', 'unstarted') \
                     ORDER BY type = 'backlog' DESC, position LIMIT 1",
                )
                .bind(team_id)
                .fetch_optional(&mut *db)
                .await?;
                let state = state
                    .ok_or_else(|| SyncError::Invalid("the team has no backlog state".into()))?;
                row.insert("state_id".into(), json!(state));
            }
            let id = uuid_field(row, "id")?;
            review::fill_in(db, actor, id, None, row).await?;
        }
        ModelKind::Comment => {
            row.insert("author_id".into(), json!(actor.user_id));
        }
        ModelKind::Spec | ModelKind::Decision | ModelKind::AiRun | ModelKind::Suggestion => {
            row.insert("created_by".into(), json!(actor.user_id));
        }
        ModelKind::SpecChange => {
            row.insert("created_by".into(), json!(actor.user_id));
            // Written only after work on the issue started: it had none then.
            let started: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM issues i JOIN workflow_states s ON s.id = i.state_id \
                 WHERE i.id = $1 AND s.type = 'started')",
            )
            .bind(uuid_field(row, "issue_id")?)
            .fetch_one(&mut *db)
            .await?;
            if started {
                row.insert(
                    "as_started".into(),
                    json!({ "specs": [], "no_change_reason": null, "replaces_decision_ids": [] }),
                );
            }
        }
        ModelKind::Team
        | ModelKind::WorkflowState
        | ModelKind::Label
        | ModelKind::IssueRelation
        | ModelKind::Project
        | ModelKind::Initiative
        | ModelKind::User
        | ModelKind::Notification
        | ModelKind::Service
        | ModelKind::Domain
        | ModelKind::IssueLink
        | ModelKind::RequirementVerdict => {}
    }
    Ok(())
}

/// Who may make a write from a client. Workspace admins may do everything
/// but edit someone else's comment. Members and agents may create and edit
/// issues, comments, specs, decisions, projects, services, labels and
/// initiatives; they may delete issues they created and comments they wrote,
/// but deleting the rest is for admins, and so are team settings (teams and
/// their workflow statuses). Notifications are their recipient's alone.
async fn allowed(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    write: Write,
    id: Uuid,
) -> Result<(), SyncError> {
    let refuse =
        |message: &str| -> Result<(), SyncError> { Err(SyncError::Forbidden(message.to_string())) };
    // Whether the record belongs to someone else: created, written or
    // received by them. A record that doesn't exist belongs to nobody.
    let not_mine = async |db: &mut PgConnection| -> Result<bool, SyncError> {
        Ok(owner_of(db, actor, model, id)
            .await?
            .is_some_and(|owner| owner != actor.user_id))
    };
    match (model, write) {
        (ModelKind::Team, _) if !is_admin(db, actor).await? => {
            refuse("Only admins can change a team's settings.")
        }
        (ModelKind::WorkflowState, _) if !is_admin(db, actor).await? => {
            refuse("Only admins can change workflow statuses.")
        }
        (ModelKind::Issue, Write::Delete)
            if not_mine(db).await? && !is_admin(db, actor).await? =>
        {
            refuse("Only admins and whoever created an issue can delete it.")
        }
        // Not even admins: a comment says who wrote it.
        (ModelKind::Comment, Write::Update) if not_mine(db).await? => {
            refuse("Only its author can edit a comment.")
        }
        (ModelKind::Comment, Write::Delete)
            if not_mine(db).await? && !is_admin(db, actor).await? =>
        {
            refuse("Only its author or an admin can delete a comment.")
        }
        (
            ModelKind::Spec
            | ModelKind::Decision
            | ModelKind::Project
            | ModelKind::Service
            | ModelKind::Domain
            | ModelKind::Label
            | ModelKind::Initiative,
            Write::Delete,
        ) if !is_admin(db, actor).await? => {
            let what = match model {
                ModelKind::Spec => "specs",
                ModelKind::Decision => "decisions",
                ModelKind::Project => "projects",
                ModelKind::Service => "services",
                ModelKind::Domain => "domains",
                ModelKind::Label => "labels",
                _ => "initiatives",
            };
            refuse(&format!("Only admins can delete {what}."))
        }
        // As far as anyone else can tell, someone's notifications don't exist.
        (ModelKind::Notification, Write::Update | Write::Delete) if not_mine(db).await? => {
            Err(SyncError::NotFound("Notification".into()))
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

/// Whom a record belongs to: an issue's creator, a comment's author, a
/// notification's recipient. `None` for other records and missing ones.
async fn owner_of(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
) -> Result<Option<Uuid>, SyncError> {
    let column = match model {
        ModelKind::Issue => "created_by",
        ModelKind::Comment => "author_id",
        ModelKind::Notification => "user_id",
        _ => return Ok(None),
    };
    let sql = format!(
        "SELECT {column} FROM {} WHERE id = $1 AND workspace_id = $2",
        model.table()
    );
    // Safe: the column and table names are constants.
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_optional(&mut *db)
        .await?)
}

/// Rules checked after a write, which the database's constraints can't express.
async fn check_rules(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
) -> Result<(), SyncError> {
    if model == ModelKind::Domain {
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM domains d JOIN domains o ON o.workspace_id = d.workspace_id \
             AND o.id <> d.id AND lower(btrim(o.name)) = lower(btrim(d.name)) WHERE d.id = $1)",
        )
        .bind(id)
        .fetch_one(&mut *db)
        .await?;
        if taken {
            return Err(SyncError::Invalid(
                "There's already a domain with that name.".into(),
            ));
        }
        return Ok(());
    }
    if model == ModelKind::Spec {
        // A spec's services must exist in the workspace.
        let unknown: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM specs sp CROSS JOIN LATERAL unnest(sp.service_ids) AS s(id) \
             WHERE sp.id = $1 AND NOT EXISTS \
             (SELECT 1 FROM services WHERE services.id = s.id AND services.workspace_id = $2)",
        )
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_one(&mut *db)
        .await?;
        if unknown > 0 {
            return Err(SyncError::Invalid("service not found".into()));
        }
        return Ok(());
    }
    if model != ModelKind::Issue {
        return Ok(());
    }

    // Labels must exist in the workspace.
    let unknown_labels: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM issues i CROSS JOIN LATERAL unnest(i.label_ids) AS l(id) \
         WHERE i.id = $1 AND NOT EXISTS (SELECT 1 FROM labels WHERE labels.id = l.id AND labels.workspace_id = $2)",
    )
    .bind(id)
    .bind(actor.workspace_id)
    .fetch_one(&mut *db)
    .await?;
    if unknown_labels > 0 {
        return Err(SyncError::Invalid("label not found".into()));
    }

    // Reviewing is a person's call.
    let agent_reviews: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM issues i JOIN users u ON u.id = i.reviewer_id \
         WHERE i.id = $1 AND u.kind = 'agent')",
    )
    .bind(id)
    .fetch_one(&mut *db)
    .await?;
    if agent_reviews {
        return Err(SyncError::Invalid(
            "A reviewer is a person, not an agent.".into(),
        ));
    }

    // An issue can't end up as its own ancestor.
    let cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE ancestors(id, depth) AS ( \
             SELECT parent_id, 1 FROM issues WHERE id = $1 AND parent_id IS NOT NULL \
             UNION ALL \
             SELECT i.parent_id, a.depth + 1 FROM issues i JOIN ancestors a ON i.id = a.id \
             WHERE i.parent_id IS NOT NULL AND a.depth < 100 \
         ) SELECT EXISTS (SELECT 1 FROM ancestors WHERE id = $1)",
    )
    .bind(id)
    .fetch_one(&mut *db)
    .await?;
    if cycle {
        return Err(SyncError::Invalid(
            "an issue can't be a sub-issue of its own sub-issue".into(),
        ));
    }
    Ok(())
}

/// The operations that must happen before a record is deleted: deleting what
/// belongs to it and clearing references to it.
async fn cascade(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    id: Uuid,
) -> Result<Vec<Operation>, SyncError> {
    let ids = async |db: &mut PgConnection, sql: &'static str| -> Result<Vec<Uuid>, SyncError> {
        Ok(sqlx::query_scalar(sql)
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_all(&mut *db)
            .await?)
    };
    let delete = |model: ModelKind| move |id: Uuid| Operation::Delete { model, id };
    let clear = |model: ModelKind, field: &'static str| {
        move |id: Uuid| Operation::Update {
            model,
            id,
            data: Map::from_iter([(field.to_string(), Value::Null)]),
        }
    };

    let mut operations = Vec::new();
    match model {
        ModelKind::Issue => {
            let comments = ids(
                db,
                "SELECT id FROM comments WHERE issue_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(comments.into_iter().map(delete(ModelKind::Comment)));
            // A spec change that never took effect goes with the issue; one
            // that did is part of its specs' history and stays, without it.
            let pending = ids(
                db,
                "SELECT id FROM spec_changes WHERE issue_id = $1 AND workspace_id = $2 AND applied_at IS NULL",
            )
            .await?;
            operations.extend(pending.into_iter().map(delete(ModelKind::SpecChange)));
            let applied = ids(
                db,
                "SELECT id FROM spec_changes WHERE issue_id = $1 AND workspace_id = $2 AND applied_at IS NOT NULL",
            )
            .await?;
            operations.extend(
                applied
                    .into_iter()
                    .map(clear(ModelKind::SpecChange, "issue_id")),
            );
            let notifications = ids(
                db,
                "SELECT id FROM notifications WHERE issue_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                notifications
                    .into_iter()
                    .map(delete(ModelKind::Notification)),
            );
            let suggestions = ids(
                db,
                "SELECT id FROM suggestions WHERE issue_id = $1 AND workspace_id = $2",
            )
            .await?;
            let links = ids(
                db,
                "SELECT id FROM issue_links WHERE issue_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(links.into_iter().map(delete(ModelKind::IssueLink)));
            operations.extend(
                suggestions
                    .into_iter()
                    .map(clear(ModelKind::Suggestion, "issue_id")),
            );
            let runs = ids(
                db,
                "SELECT id FROM ai_runs WHERE issue_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(runs.into_iter().map(clear(ModelKind::AiRun, "issue_id")));
            let relations = ids(
                db,
                "SELECT id FROM issue_relations WHERE (issue_id = $1 OR related_issue_id = $1) AND workspace_id = $2",
            )
            .await?;
            operations.extend(relations.into_iter().map(delete(ModelKind::IssueRelation)));
            let children = ids(
                db,
                "SELECT id FROM issues WHERE parent_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                children
                    .into_iter()
                    .map(clear(ModelKind::Issue, "parent_id")),
            );
        }
        ModelKind::Project => {
            for (model, table) in [
                (ModelKind::Issue, "issues"),
                (ModelKind::Spec, "specs"),
                (ModelKind::Decision, "decisions"),
            ] {
                let sql =
                    format!("SELECT id FROM {table} WHERE project_id = $1 AND workspace_id = $2");
                // Safe: table names are constants above.
                let found: Vec<Uuid> = sqlx::query_scalar(AssertSqlSafe(sql))
                    .bind(id)
                    .bind(actor.workspace_id)
                    .fetch_all(&mut *db)
                    .await?;
                operations.extend(found.into_iter().map(clear(model, "project_id")));
            }
        }
        ModelKind::Domain => {
            let specs = ids(
                db,
                "SELECT id FROM specs WHERE domain_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(specs.into_iter().map(clear(ModelKind::Spec, "domain_id")));
        }
        ModelKind::Service => {
            for (model, table) in [
                (ModelKind::Issue, "issues"),
                (ModelKind::Decision, "decisions"),
                (ModelKind::Suggestion, "suggestions"),
                (ModelKind::AiRun, "ai_runs"),
            ] {
                let sql =
                    format!("SELECT id FROM {table} WHERE service_id = $1 AND workspace_id = $2");
                // Safe: table names are constants above.
                let found: Vec<Uuid> = sqlx::query_scalar(AssertSqlSafe(sql))
                    .bind(id)
                    .bind(actor.workspace_id)
                    .fetch_all(&mut *db)
                    .await?;
                operations.extend(found.into_iter().map(clear(model, "service_id")));
            }
            // Its specs stay, belonging to their other services, if any.
            let specs: Vec<(Uuid, Vec<Uuid>)> = sqlx::query_as(
                "SELECT id, service_ids FROM specs WHERE $1 = ANY (service_ids) AND workspace_id = $2",
            )
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_all(&mut *db)
            .await?;
            operations.extend(specs.into_iter().map(|(spec, services)| {
                let remaining: Vec<Uuid> = services
                    .into_iter()
                    .filter(|service| *service != id)
                    .collect();
                Operation::Update {
                    model: ModelKind::Spec,
                    id: spec,
                    data: Map::from_iter([("service_ids".to_string(), json!(remaining))]),
                }
            }));
            // A new spec proposed for the service keeps its place, without the service.
            operations.extend(
                fix_spec_deltas(db, actor, "new_service_id", id, |specs| {
                    specs
                        .into_iter()
                        .map(|mut d| {
                            if uuid_at(&d, "new_service_id") == Some(id) {
                                d["new_service_id"] = Value::Null;
                            }
                            d
                        })
                        .collect()
                })
                .await?,
            );
        }
        ModelKind::Decision => {
            let later = ids(
                db,
                "SELECT id FROM decisions WHERE superseded_by_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                later
                    .into_iter()
                    .map(clear(ModelKind::Decision, "superseded_by_id")),
            );
            let suggestions = ids(
                db,
                "SELECT id FROM suggestions WHERE decision_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                suggestions
                    .into_iter()
                    .map(clear(ModelKind::Suggestion, "decision_id")),
            );
            let replacing: Vec<(Uuid, Vec<Uuid>, Option<Value>)> = sqlx::query_as(
                // Only ones still to take effect: what one did stays as it was.
                "SELECT id, replaces_decision_ids, as_started FROM spec_changes \
                 WHERE ($1 = ANY (replaces_decision_ids) \
                   OR as_started -> 'replaces_decision_ids' ? $1::text) \
                 AND workspace_id = $2 AND applied_at IS NULL",
            )
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_all(&mut *db)
            .await?;
            operations.extend(replacing.into_iter().map(|(change, decisions, started)| {
                let remaining: Vec<Uuid> = decisions.into_iter().filter(|d| *d != id).collect();
                let mut data =
                    Map::from_iter([("replaces_decision_ids".to_string(), json!(remaining))]);
                // What it replaced when work started loses it too, so the
                // deletion doesn't count as a change after the start.
                if let Some(mut started) = started.filter(Value::is_object) {
                    if let Some(Value::Array(list)) = started.get_mut("replaces_decision_ids") {
                        list.retain(|d| d.as_str() != Some(id.to_string().as_str()));
                    }
                    data.insert("as_started".into(), started);
                }
                Operation::Update {
                    model: ModelKind::SpecChange,
                    id: change,
                    data,
                }
            }));
        }
        ModelKind::Spec => {
            let verdicts = ids(
                db,
                "SELECT id FROM requirement_verdicts WHERE spec_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                verdicts
                    .into_iter()
                    .map(delete(ModelKind::RequirementVerdict)),
            );
            let suggestions = ids(
                db,
                "SELECT id FROM suggestions WHERE spec_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                suggestions
                    .into_iter()
                    .map(clear(ModelKind::Suggestion, "spec_id")),
            );
            // An issue's change to the spec goes with it.
            operations.extend(
                fix_spec_deltas(db, actor, "spec_id", id, |specs| {
                    specs
                        .into_iter()
                        .filter(|d| uuid_at(d, "spec_id") != Some(id))
                        .collect()
                })
                .await?,
            );
        }
        ModelKind::Initiative => {
            let projects = ids(
                db,
                "SELECT id FROM projects WHERE initiative_id = $1 AND workspace_id = $2",
            )
            .await?;
            operations.extend(
                projects
                    .into_iter()
                    .map(clear(ModelKind::Project, "initiative_id")),
            );
        }
        ModelKind::Label => {
            let issues: Vec<(Uuid, Vec<Uuid>)> = sqlx::query_as(
                "SELECT id, label_ids FROM issues WHERE $1 = ANY (label_ids) AND workspace_id = $2",
            )
            .bind(id)
            .bind(actor.workspace_id)
            .fetch_all(&mut *db)
            .await?;
            operations.extend(issues.into_iter().map(|(issue, labels)| {
                let remaining: Vec<Uuid> =
                    labels.into_iter().filter(|label| *label != id).collect();
                Operation::Update {
                    model: ModelKind::Issue,
                    id: issue,
                    data: Map::from_iter([("label_ids".to_string(), json!(remaining))]),
                }
            }));
        }
        // A state still in use can't be deleted: the foreign key refuses.
        ModelKind::WorkflowState
        | ModelKind::Comment
        | ModelKind::IssueRelation
        | ModelKind::Notification
        | ModelKind::AiRun
        | ModelKind::Suggestion
        | ModelKind::IssueLink
        | ModelKind::RequirementVerdict
        | ModelKind::SpecChange => {}
        ModelKind::Team | ModelKind::User => {
            return Err(SyncError::Invalid(format!(
                "{} records can't be deleted this way",
                model.name()
            )));
        }
    }
    Ok(operations)
}

/// Changes `fix` makes to the parts of spec changes still to take effect
/// that name `id` in `field`, both as they are and as they were when work
/// on the issue started, so a deletion doesn't count as a change after the
/// start. One that took effect is history, and stays as it was.
async fn fix_spec_deltas(
    db: &mut PgConnection,
    actor: Actor,
    field: &str,
    id: Uuid,
    fix: impl Fn(Vec<Value>) -> Vec<Value>,
) -> Result<Vec<Operation>, SyncError> {
    let found: Vec<(Uuid, Value, Option<Value>)> = sqlx::query_as(
        "SELECT id, specs, as_started FROM spec_changes \
         WHERE (specs @> jsonb_build_array(jsonb_build_object($1::text, $2::uuid)) \
           OR as_started -> 'specs' @> jsonb_build_array(jsonb_build_object($1::text, $2::uuid))) \
         AND workspace_id = $3 AND applied_at IS NULL",
    )
    .bind(field)
    .bind(id)
    .bind(actor.workspace_id)
    .fetch_all(&mut *db)
    .await?;
    let list = |value: Value| match value {
        Value::Array(parts) => parts,
        _ => Vec::new(),
    };
    Ok(found
        .into_iter()
        .map(|(change, specs, started)| {
            let mut data = Map::from_iter([("specs".to_string(), Value::Array(fix(list(specs))))]);
            if let Some(mut started) = started.filter(Value::is_object) {
                let parts = list(started["specs"].take());
                started["specs"] = Value::Array(fix(parts));
                data.insert("as_started".into(), started);
            }
            Operation::Update {
                model: ModelKind::SpecChange,
                id: change,
                data,
            }
        })
        .collect())
}

fn uuid_field(row: &Map<String, Value>, field: &str) -> Result<Uuid, SyncError> {
    row.get(field)
        .and_then(Value::as_str)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| SyncError::Invalid(format!("{field} is required")))
}
