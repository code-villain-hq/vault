//! Triage by Forge's AI: when an issue arrives in triage, suggest its
//! labels, priority, service, project and assignee, and point out
//! duplicates. People accept or dismiss the suggestion next to the issue in
//! Triage. Runs on the drift worker, as runs of kind `triage`.
//!
//! Agents make the same suggestions through MCP (`suggest_triage`), with
//! the same checks ([`changes`], [`save`]), so triage works without a key.

use std::fmt::Write as _;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use forge_ai::{Agent, ModelProvider, Stop, ToolHandler, ToolOutput};
use forge_domain::{Actor, Issue, ModelKind, Operation};
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::check::{CheckResult, last_text};
use super::{DriftSettings, QueuedRun, ai_agent, object, write};
use crate::agents::takes_work;
use crate::auth::Member;
use crate::error::AppError;
use crate::mcp::snapshot::Snapshot;
use crate::mcp::tools::{self as mcp, IssueRef, SearchIssues};
use crate::state::AppState;

/// In a suggested issue update: label ids to add to the issue's labels when
/// the suggestion is accepted, rather than a whole new list.
pub const ADD_LABELS: &str = "add_label_ids";

/// Triage runs are short: a few searches, then one suggestion.
const MAX_TURNS: usize = 15;

/// Issues that arrived in triage within this long get triaged on their own.
const NEW_FOR: &str = "1 day";

const SYSTEM: &str = "\
You triage one new issue in an issue tracker: suggest the properties it should have, and point out \
if it duplicates or relates to an existing issue. People review your suggestion before anything \
changes.

How to work:
- Read the issue and the workspace context below. Search for possible duplicates with a few \
distinctive words from the issue (search_issues, including closed issues), and open promising \
matches with get_issue before calling something a duplicate.
- Then call suggest once, with only the properties you're fairly confident about. Leave a property \
out rather than guess, and don't change what the reporter already set unless it's clearly wrong.
- Priority: urgent for outages, data loss or security problems; high for broken core features \
without a workaround; medium for most bugs and wanted features; low for polish.
- Service: the part of the system the issue is about, judging by the services' descriptions.
- Assignee: only when it's clear who should take it: the person who has handled very similar \
issues recently, or an agent when the issue is a change to the code that an agent can build.
- If nothing is worth suggesting, call finish instead and say why in a sentence.";

/// `POST /api/w/{workspace}/issues/{issue}/triage`: asks Forge's AI to triage an issue.
pub async fn request(
    State(state): State<AppState>,
    member: Member,
    UrlPath((_, issue_id)): UrlPath<(Uuid, Uuid)>,
) -> Result<Json<QueuedRun>, AppError> {
    let provider = state.ai.as_ref().ok_or_else(|| {
        AppError::invalid("triage by Forge AI needs a model: set ANTHROPIC_API_KEY on the server")
    })?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM issues WHERE id = $1 AND workspace_id = $2)",
    )
    .bind(issue_id)
    .bind(member.actor.workspace_id)
    .fetch_one(&state.pool)
    .await?;
    if !exists {
        return Err(AppError::not_found("issue not found"));
    }
    member.require_sees_issue(&state.pool, issue_id).await?;
    let mut db = state.pool.begin().await?;
    let id = queue(&mut db, member.actor, issue_id, "manual", provider.model()).await?;
    db.commit().await?;
    state.drift_wake.notify_one();
    Ok(Json(QueuedRun { id }))
}

/// Queues a triage run unless the issue already has one waiting or running.
async fn queue(
    db: &mut PgConnection,
    actor: Actor,
    issue_id: Uuid,
    trigger: &str,
    model: &str,
) -> Result<Uuid, AppError> {
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ai_runs WHERE issue_id = $1 AND status IN ('queued', 'running'))",
    )
    .bind(issue_id)
    .fetch_one(&mut *db)
    .await?;
    if busy {
        return Err(AppError::conflict(
            "Forge AI is already looking at this issue",
        ));
    }
    if has_open_suggestion(&mut *db, issue_id).await? {
        return Err(AppError::conflict(
            "this issue already has a triage suggestion waiting",
        ));
    }
    let id = Uuid::now_v7();
    let data = object(json!({
        "kind": "triage", "issue_id": issue_id, "trigger": trigger, "model": model,
    }));
    write(
        db,
        actor,
        vec![Operation::Create {
            model: ModelKind::AiRun,
            id,
            data,
        }],
    )
    .await?;
    Ok(id)
}

/// Queues a triage run for each issue that arrived in triage recently and
/// hasn't been triaged, by Forge AI or an agent. Returns how many were queued.
pub async fn queue_new_issues(pool: &PgPool, model: &str) -> Result<usize, AppError> {
    let new: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT i.id, i.workspace_id FROM issues i JOIN workflow_states s ON s.id = i.state_id \
         WHERE s.type = 'triage' AND i.created_at > now() - $1::interval \
         AND NOT EXISTS (SELECT 1 FROM ai_runs r WHERE r.issue_id = i.id) \
         AND NOT EXISTS (SELECT 1 FROM suggestions g WHERE g.issue_id = i.id AND g.kind = 'triage')",
    )
    .bind(NEW_FOR)
    .fetch_all(pool)
    .await?;
    let mut queued = 0;
    for (issue_id, workspace_id) in new {
        let mut db = pool.begin().await?;
        let agent = ai_agent(&mut db, workspace_id).await?;
        match queue(&mut db, agent, issue_id, "new_issue", model).await {
            Ok(_) => {
                db.commit().await?;
                queued += 1;
            }
            Err(e) => tracing::warn!(%issue_id, error = ?e, "couldn't queue triage"),
        }
    }
    Ok(queued)
}

pub async fn run(
    pool: &PgPool,
    provider: &dyn ModelProvider,
    settings: &DriftSettings,
    agent: Actor,
    run_id: Uuid,
    issue_id: Uuid,
) -> Result<CheckResult, String> {
    let snapshot = Snapshot::load(pool, agent).await?;
    let issue = snapshot
        .issues
        .iter()
        .find(|i| i.id == issue_id)
        .ok_or("the issue was deleted")?
        .clone();
    let details = mcp::get_issue(
        pool,
        agent,
        IssueRef {
            identifier: snapshot.identifier(&issue),
        },
    )
    .await?;
    let context = context(&snapshot, &issue, &details);

    let mut tools = TriageTools {
        pool: pool.clone(),
        agent,
        snapshot,
        issue,
        run_id,
        suggestions: 0,
        summary: None,
        finished: false,
    };
    let max_turns = settings.max_turns.min(MAX_TURNS);
    let agent_loop = Agent {
        system: SYSTEM.into(),
        tools: definitions(),
        max_turns,
        max_tokens: 8_000,
    };
    let outcome = agent_loop
        .run(provider, context, &mut tools)
        .await
        .map_err(|e| e.to_string())?;
    let error = match &outcome.stop {
        Stop::Done => None,
        Stop::Refused(why) => Some(format!(
            "The model declined to continue{}",
            why.as_deref().map(|w| format!(": {w}")).unwrap_or_default()
        )),
        Stop::TurnLimit => Some(format!(
            "Stopped after {max_turns} turns without finishing."
        )),
        Stop::MaxTokens => Some("A response hit the output limit.".into()),
    };
    Ok(CheckResult {
        summary: tools
            .summary
            .clone()
            .unwrap_or_else(|| last_text(&outcome.messages)),
        error,
        input_tokens: outcome.usage.input_tokens
            + outcome.usage.cache_creation_input_tokens
            + outcome.usage.cache_read_input_tokens,
        output_tokens: outcome.usage.output_tokens,
        cost_usd: outcome.usage.cost_usd(provider.model()),
        suggestions: tools.suggestions,
        messages: outcome.messages,
    })
}

/// The issue and what it could be given, for the first message.
fn context(s: &Snapshot, issue: &Issue, details: &str) -> String {
    let team = s.team_of(issue);
    let mut out = String::new();
    let _ = writeln!(out, "# The new issue\n\n```json\n{details}\n```\n");
    let labels: Vec<&str> = s
        .labels
        .iter()
        .filter(|l| l.team_id.is_none_or(|t| Some(t) == team.map(|t| t.id)))
        .map(|l| l.name.as_str())
        .collect();
    let _ = writeln!(out, "# Labels it can have\n\n{}\n", list(&labels));
    let _ = writeln!(out, "# Services\n");
    if s.services.is_empty() {
        let _ = writeln!(out, "(none)");
    }
    for service in &s.services {
        let description = service.description.lines().next().unwrap_or("").trim();
        let _ = writeln!(out, "- {}: {description}", service.name);
    }
    let projects: Vec<&str> = s
        .projects
        .iter()
        .filter(|p| {
            !matches!(
                p.status,
                forge_domain::ProjectStatus::Completed | forge_domain::ProjectStatus::Canceled
            )
        })
        .map(|p| p.name.as_str())
        .collect();
    let _ = writeln!(out, "\n# Active projects\n\n{}\n", list(&projects));
    let (people, agents): (Vec<_>, Vec<_>) = s
        .users
        .iter()
        .filter(|u| takes_work(u))
        .partition(|u| u.kind == forge_domain::UserKind::Person);
    fn names(users: Vec<&forge_domain::User>) -> Vec<&str> {
        users.into_iter().map(|u| u.name.as_str()).collect()
    }
    let _ = writeln!(out, "# People\n\n{}\n", list(&names(people)));
    let _ = writeln!(
        out,
        "# Agents\n\nCoding agents, who build changes to the code: {}",
        list(&names(agents))
    );
    out
}

fn list(items: &[&str]) -> String {
    if items.is_empty() {
        "(none)".into()
    } else {
        items.join(", ")
    }
}

fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "search_issues",
            "description": "Find issues whose identifier, title or description contains the query (case-insensitive), most recently updated first.",
            "input_schema": { "type": "object", "required": ["query"], "properties": {
                "query": { "type": "string", "description": "A word or short phrase, or an identifier like FOR-12." },
                "include_closed": { "type": "boolean", "description": "Include completed and canceled issues. Use it when looking for duplicates." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 25 },
            }},
        }),
        json!({
            "name": "get_issue",
            "description": "Everything about one issue: description, status, comments, relations.",
            "input_schema": { "type": "object", "required": ["identifier"], "properties": {
                "identifier": { "type": "string", "description": "e.g. FOR-12" },
            }},
        }),
        json!({
            "name": "suggest",
            "description": "Suggest properties for the new issue and its duplicates or related issues. Ends the triage. Leave out anything you aren't fairly confident about.",
            "input_schema": { "type": "object", "required": ["explanation"], "properties": {
                "explanation": { "type": "string", "description": "Markdown, a few sentences: why these properties, and the evidence for any duplicate." },
                "priority": { "type": "string", "enum": ["urgent", "high", "medium", "low"] },
                "labels": { "type": "array", "items": { "type": "string" }, "description": "Label names to add." },
                "service": { "type": "string", "description": "Service name." },
                "project": { "type": "string", "description": "Project name." },
                "assignee": { "type": "string", "description": "A person's or agent's name." },
                "duplicate_of": { "type": "string", "description": "Identifier of the issue this one duplicates." },
                "related": { "type": "array", "items": { "type": "string" }, "description": "Identifiers of closely related issues." },
            }},
        }),
        json!({
            "name": "finish",
            "description": "End the triage without suggesting anything, saying why.",
            "input_schema": { "type": "object", "required": ["summary"], "properties": {
                "summary": { "type": "string" },
            }},
        }),
    ]
}

#[derive(Deserialize)]
struct Search {
    query: String,
    include_closed: Option<bool>,
    limit: Option<usize>,
}

/// What a triage suggests, by name, as the model or an agent gives it.
#[derive(Debug, Default, Deserialize)]
pub struct Suggest {
    pub explanation: String,
    pub priority: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub service: Option<String>,
    pub project: Option<String>,
    pub assignee: Option<String>,
    pub duplicate_of: Option<String>,
    #[serde(default)]
    pub related: Vec<String>,
}

/// A triage suggestion checked against the workspace: the changes in words,
/// and the same as operations, leaving out what the issue already has.
pub struct Changes {
    pub changes: Vec<String>,
    pub operations: Vec<Operation>,
    pub service_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct Finish {
    summary: String,
}

fn parse<T: serde::de::DeserializeOwned>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("invalid input: {e}"))
}

struct TriageTools {
    pool: PgPool,
    agent: Actor,
    snapshot: Snapshot,
    issue: Issue,
    run_id: Uuid,
    suggestions: i32,
    summary: Option<String>,
    finished: bool,
}

impl ToolHandler for TriageTools {
    fn finished(&self) -> bool {
        self.finished
    }

    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let result = match name {
                "search_issues" => match parse::<Search>(input) {
                    Ok(i) => {
                        mcp::search_issues(
                            &self.pool,
                            self.agent,
                            SearchIssues {
                                query: Some(i.query),
                                team: None,
                                status: None,
                                assignee: None,
                                project: None,
                                service: None,
                                include_closed: i.include_closed,
                                needs_triage: None,
                                limit: Some(i.limit.unwrap_or(10).min(25)),
                            },
                        )
                        .await
                    }
                    Err(e) => Err(e),
                },
                "get_issue" => match parse::<IssueRef>(input) {
                    Ok(i) => mcp::get_issue(&self.pool, self.agent, i).await,
                    Err(e) => Err(e),
                },
                "suggest" => match parse::<Suggest>(input) {
                    Ok(s) => self.suggest(s).await,
                    Err(e) => Err(e),
                },
                "finish" => match parse::<Finish>(input) {
                    Ok(f) => {
                        self.summary = Some(f.summary);
                        self.finished = true;
                        Ok("Done.".into())
                    }
                    Err(e) => Err(e),
                },
                other => Err(format!("there's no tool called {other}")),
            };
            match result {
                Ok(content) => ToolOutput::ok(content),
                Err(error) => ToolOutput::error(error),
            }
        })
    }
}

impl TriageTools {
    async fn suggest(&mut self, suggestion: Suggest) -> Result<String, String> {
        let found = changes(&self.snapshot, &self.issue, &suggestion)?;
        self.finished = true;
        self.summary = Some(suggestion.explanation.clone());
        if found.operations.is_empty() {
            return Ok("Nothing to change, so nothing was suggested.".into());
        }
        save(
            &self.pool,
            self.agent,
            &self.snapshot,
            &self.issue,
            &suggestion.explanation,
            found,
            Some(self.run_id),
        )
        .await?;
        self.suggestions = 1;
        Ok("Suggested. People will review it.".into())
    }
}

/// Checks a triage suggestion against the workspace and turns it into
/// changes: unknown names, an issue relating to itself or an assignee who
/// can't take work are refused.
pub fn changes(s: &Snapshot, issue: &Issue, suggestion: &Suggest) -> Result<Changes, String> {
    let team = s.team_of(issue).ok_or("the issue has no team")?;
    let mut changes = Vec::new();
    let mut fields = serde_json::Map::new();
    let mut operations = Vec::new();

    if let Some(name) = suggestion.priority.as_deref() {
        let priority = mcp::priority_number(name)?;
        if priority != issue.priority {
            changes.push(format!("Priority: {}", name.to_lowercase()));
            fields.insert("priority".into(), json!(priority));
        }
    }
    // Only the labels to add: accepting adds them to whatever the issue
    // has by then (see `suggestions::resolve`), so nothing is lost.
    let mut add = Vec::new();
    for name in &suggestion.labels {
        let label = s.label(team, name)?;
        if !issue.label_ids.contains(&label.id) && !add.contains(&label.id) {
            add.push(label.id);
            changes.push(format!("Label: {}", label.name));
        }
    }
    if !add.is_empty() {
        fields.insert(ADD_LABELS.into(), json!(add));
    }
    let service = suggestion
        .service
        .as_deref()
        .map(|name| s.service(name))
        .transpose()?;
    if let Some(service) = service.filter(|sv| issue.service_id != Some(sv.id)) {
        changes.push(format!("Service: {}", service.name));
        fields.insert("service_id".into(), json!(service.id));
    }
    if let Some(project) = suggestion
        .project
        .as_deref()
        .map(|p| s.project(p))
        .transpose()?
        && issue.project_id != Some(project.id)
    {
        changes.push(format!("Project: {}", project.name));
        fields.insert("project_id".into(), json!(project.id));
    }
    if let Some(assignee) = suggestion
        .assignee
        .as_deref()
        .map(|a| s.user(a))
        .transpose()?
    {
        if !takes_work(assignee) {
            return Err(format!(
                "{} is part of Forge and can't be assigned issues",
                assignee.name
            ));
        }
        if issue.assignee_id != Some(assignee.id) {
            changes.push(format!("Assignee: {}", assignee.name));
            fields.insert("assignee_id".into(), json!(assignee.id));
        }
    }
    if !fields.is_empty() {
        operations.push(Operation::Update {
            model: ModelKind::Issue,
            id: issue.id,
            data: fields,
        });
    }

    let mut related_to = Vec::new();
    let mut relate = |other: &str, kind: &str| -> Result<(), String> {
        let other = s.issue(other)?;
        if other.id == issue.id {
            return Err("an issue can't relate to itself".into());
        }
        // Once per issue, and not again if they're already related.
        let known = s.relations.iter().any(|r| {
            (r.issue_id, r.related_issue_id) == (issue.id, other.id)
                || (r.issue_id, r.related_issue_id) == (other.id, issue.id)
        });
        if known || related_to.contains(&other.id) {
            return Ok(());
        }
        related_to.push(other.id);
        let words = if kind == "duplicate" {
            "Duplicate of"
        } else {
            "Related to"
        };
        changes.push(format!("{words} {}: {}", s.identifier(other), other.title));
        operations.push(Operation::Create {
            model: ModelKind::IssueRelation,
            id: Uuid::now_v7(),
            data: object(json!({
                "issue_id": issue.id, "related_issue_id": other.id, "type": kind,
            })),
        });
        Ok(())
    };
    if let Some(original) = suggestion.duplicate_of.as_deref() {
        relate(original, "duplicate")?;
    }
    for other in &suggestion.related {
        relate(other, "related")?;
    }
    Ok(Changes {
        changes,
        operations,
        service_id: service.map(|sv| sv.id),
    })
}

/// Saves a triage suggestion for the issue, credited to `actor`, unless one
/// is already waiting: an issue has at most one open triage suggestion.
pub async fn save(
    pool: &PgPool,
    actor: Actor,
    s: &Snapshot,
    issue: &Issue,
    why: &str,
    found: Changes,
    run_id: Option<Uuid>,
) -> Result<Uuid, String> {
    let identifier = s.identifier(issue);
    let operations = found
        .operations
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let id = Uuid::now_v7();
    let data = object(json!({
        "kind": "triage",
        "title": format!("Triage {identifier}"),
        "body": why,
        "changes": found.changes,
        "operations": operations,
        "issue_id": issue.id,
        "service_id": found.service_id,
        "run_id": run_id,
    }));
    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    // The workspace first, as every write does, so two triages of the same
    // issue can't both find no suggestion waiting.
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(actor.workspace_id)
        .execute(&mut *db)
        .await
        .map_err(|e| e.to_string())?;
    if has_open_suggestion(&mut db, issue.id)
        .await
        .map_err(|e| format!("{e:?}"))?
    {
        return Err(format!(
            "{identifier} already has a triage suggestion waiting for people to apply or dismiss"
        ));
    }
    write(
        &mut db,
        actor,
        vec![Operation::Create {
            model: ModelKind::Suggestion,
            id,
            data,
        }],
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    db.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

async fn has_open_suggestion(db: &mut PgConnection, issue_id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM suggestions \
         WHERE issue_id = $1 AND kind = 'triage' AND status = 'open')",
    )
    .bind(issue_id)
    .fetch_one(&mut *db)
    .await?)
}

/// Issues that don't need triaging (any more): those with a triage
/// suggestion, waiting or resolved, or with Forge AI looking at them.
pub async fn triaged(pool: &PgPool, workspace_id: Uuid) -> Result<Vec<Uuid>, String> {
    sqlx::query_scalar(
        "SELECT issue_id FROM suggestions WHERE workspace_id = $1 AND kind = 'triage' \
         AND issue_id IS NOT NULL \
         UNION SELECT issue_id FROM ai_runs WHERE workspace_id = $1 AND kind = 'triage' \
         AND status IN ('queued', 'running') AND issue_id IS NOT NULL",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}
