//! What the MCP tools do, as plain functions over the database so they're
//! easy to test. `mcp/mod.rs` exposes them over MCP.
//!
//! Reads use a [`Snapshot`]; writes go through the sync engine as the agent,
//! so they follow the same rules as the app and show up live for everyone.

use chrono::Utc;
use forge_domain::{
    Actor, Decision, DecisionStatus, Issue, IssueChange, IssuePatch, ModelKind, Operation, Spec,
    SpecChange, SpecDelta, SpecStatus, StateType, Transaction, UserKind, Validate,
    convention_warnings, parse_delta, queue_order, requirement_warnings, requirements, touched,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::snapshot::{Snapshot, ToolResult};
use crate::drift::triage;

const PRIORITIES: [&str; 5] = ["none", "urgent", "high", "medium", "low"];

/// Clears a field when passed instead of a value.
const NONE: &str = "none";

fn is_none(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case(NONE) || value.trim().is_empty()
}

pub(crate) fn priority_number(name: &str) -> ToolResult<i16> {
    PRIORITIES
        .iter()
        .position(|p| p.eq_ignore_ascii_case(name.trim()))
        .map(|p| p as i16)
        .ok_or_else(|| format!("priority must be one of {}", PRIORITIES.join(", ")))
}

fn pretty(value: Value) -> ToolResult<String> {
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

async fn apply(pool: &PgPool, actor: Actor, operations: Vec<Operation>) -> ToolResult<()> {
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    forge_sync::apply_transaction(pool, actor, &transaction)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

// Overview

pub async fn workspace_overview(pool: &PgPool, actor: Actor) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let teams: Vec<Value> = s
        .teams
        .iter()
        .map(|team| {
            let mut states: Vec<_> = s.states.iter().filter(|st| st.team_id == team.id).collect();
            states.sort_by(|a, b| a.position.total_cmp(&b.position));
            json!({
                "key": team.key,
                "name": team.name,
                "statuses": states.iter().map(|st| json!({ "name": st.name, "type": st.kind })).collect::<Vec<_>>(),
            })
        })
        .collect();
    pretty(json!({
        "you": s.user_name(Some(actor.user_id)),
        "teams": teams,
        "members": s.users.iter().map(|u| json!({ "name": u.name, "email": u.email, "kind": u.kind })).collect::<Vec<_>>(),
        "labels": s.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
        "projects": s.projects.iter().map(|p| json!({ "id": p.id, "name": p.name, "status": p.status })).collect::<Vec<_>>(),
        "services": s.services.iter().map(|sv| json!({
            "id": sv.id, "name": sv.name, "repository": sv.repository, "paths": sv.paths,
        })).collect::<Vec<_>>(),
        "priorities": PRIORITIES,
    }))
}

// Issues

fn issue_summary(s: &Snapshot, issue: &Issue) -> Value {
    let state = s.state_of(issue);
    let mut summary = json!({
        "identifier": s.identifier(issue),
        "title": issue.title,
        "status": state.map(|st| &st.name),
        "status_type": state.map(|st| st.kind),
        "priority": PRIORITIES[issue.priority.clamp(0, 4) as usize],
        "assignee": s.user_name(issue.assignee_id),
        "reviewer": s.user_name(issue.reviewer_id),
    });
    if issue.no_code {
        summary["no_code"] = json!(true);
    }
    // Only issues waiting for an agent have a place in its queue.
    if let Some(position) = s.queue.get(&issue.id) {
        summary["queue_position"] = json!(position);
    }
    summary
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchIssues {
    /// Words to find in titles and descriptions, or an identifier like FOR-12.
    pub query: Option<String>,
    /// Team key, e.g. FOR.
    pub team: Option<String>,
    /// Status name, e.g. "In Progress".
    pub status: Option<String>,
    /// Name or email of the assignee; "me" for yourself. The results then
    /// come in queue order: the issues waiting in the assignee's queue
    /// first, the one to take up next at the top.
    pub assignee: Option<String>,
    /// Project id or name.
    pub project: Option<String>,
    /// Service id or name.
    pub service: Option<String>,
    /// Include completed and canceled issues. Default false.
    pub include_closed: Option<bool>,
    /// Only issues in Triage that nobody has suggested triage for yet
    /// (see suggest_triage).
    pub needs_triage: Option<bool>,
    /// At most this many results. Default 25.
    pub limit: Option<usize>,
}

pub async fn search_issues(pool: &PgPool, actor: Actor, input: SearchIssues) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let team = input.team.as_deref().map(|t| s.team(t)).transpose()?;
    let assignee = input.assignee.as_deref().map(|a| s.user(a)).transpose()?;
    let project = input.project.as_deref().map(|p| s.project(p)).transpose()?;
    let service = input
        .service
        .as_deref()
        .map(|sv| s.service(sv))
        .transpose()?;
    let query = input.query.as_deref().map(str::to_lowercase);
    let include_closed = input.include_closed.unwrap_or(false);
    let triaged = if input.needs_triage.unwrap_or(false) {
        Some(triage::triaged(pool, actor.workspace_id).await?)
    } else {
        None
    };

    let mut found: Vec<&Issue> =
        s.issues
            .iter()
            .filter(|i| team.is_none_or(|t| i.team_id == t.id))
            .filter(|i| assignee.is_none_or(|a| i.assignee_id == Some(a.id)))
            .filter(|i| project.is_none_or(|p| i.project_id == Some(p.id)))
            .filter(|i| service.is_none_or(|sv| i.service_id == Some(sv.id)))
            .filter(|i| {
                let state = s.state_of(i);
                let closed = state.is_some_and(|st| {
                    matches!(
                        st.kind,
                        forge_domain::StateType::Completed | forge_domain::StateType::Canceled
                    )
                });
                (include_closed || !closed)
                    && input.status.as_deref().is_none_or(|name| {
                        state.is_some_and(|st| st.name.eq_ignore_ascii_case(name))
                    })
            })
            .filter(|i| {
                triaged.as_ref().is_none_or(|triaged| {
                    s.state_of(i)
                        .is_some_and(|st| st.kind == forge_domain::StateType::Triage)
                        && !triaged.contains(&i.id)
                })
            })
            .filter(|i| {
                query.as_deref().is_none_or(|q| {
                    s.identifier(i).to_lowercase() == q
                        || i.title.to_lowercase().contains(q)
                        || i.description.to_lowercase().contains(q)
                })
            })
            .collect();
    if assignee.is_some() {
        // The order agents take work in: the assignee's queue first (by
        // priority, then age), then the rest by the same rules.
        found.sort_by(|a, b| {
            let queued = |i: &Issue| s.queue.contains_key(&i.id);
            queued(b).cmp(&queued(a)).then_with(|| queue_order(a, b))
        });
    } else {
        found.sort_by_key(|i| std::cmp::Reverse(i.updated_at));
    }
    let limit = input.limit.unwrap_or(25).min(200);
    let total = found.len();
    pretty(json!({
        "total": total,
        "issues": found.into_iter().take(limit).map(|i| issue_summary(&s, i)).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IssueRef {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
}

/// A spec or decision, and what makes it apply to an issue.
pub type Applying<'a, T> = (&'a T, &'static str);

/// The specs and decisions an issue is built on, each with what it applies
/// through: the issue's service, its project, or (decisions with neither)
/// the whole workspace. Deprecated specs and decisions no longer in force
/// are left out. The same set as `issueKnowledge()` on the issue page, so
/// agents and people work from the same basis.
fn issue_knowledge<'a>(
    s: &'a Snapshot,
    issue: &Issue,
) -> (Vec<Applying<'a, Spec>>, Vec<Applying<'a, Decision>>) {
    let projects: Vec<Uuid> = issue.project_id.into_iter().collect();
    knowledge(s, issue.service_id, &projects)
}

/// The specs and decisions that apply to work on a service and in some
/// projects, chosen and ordered as for an issue (see [`issue_knowledge`]).
pub fn knowledge<'a>(
    s: &'a Snapshot,
    service: Option<Uuid>,
    projects: &[Uuid],
) -> (Vec<Applying<'a, Spec>>, Vec<Applying<'a, Decision>>) {
    let via = |service_id: Option<Uuid>, project_id: Option<Uuid>| {
        if service_id.is_some() && service_id == service {
            Some("service")
        } else if project_id.is_some_and(|p| projects.contains(&p)) {
            Some("project")
        } else {
            None
        }
    };
    let mut specs: Vec<_> = s
        .specs
        .iter()
        .filter(|sp| sp.status != SpecStatus::Deprecated)
        .filter_map(|sp| {
            let own = sp
                .service_ids
                .iter()
                .copied()
                .find(|id| Some(*id) == service);
            Some((sp, via(own, sp.project_id)?))
        })
        .collect();
    specs.sort_by(|(a, _), (b, _)| a.title.cmp(&b.title));
    let mut decisions: Vec<_> = s
        .decisions
        .iter()
        .filter(|d| {
            matches!(
                d.status,
                DecisionStatus::Accepted | DecisionStatus::Proposed
            )
        })
        .filter_map(|d| {
            let workspace_wide = d.service_id.is_none() && d.project_id.is_none();
            let via = via(d.service_id, d.project_id).or(workspace_wide.then_some("workspace"))?;
            Some((d, via))
        })
        .collect();
    // Newest first; a proposed decision counts from when it was written.
    let decided_on = |d: &Decision| d.decided_on.unwrap_or(d.created_at.date_naive());
    decisions.sort_by(|(a, _), (b, _)| {
        decided_on(b)
            .cmp(&decided_on(a))
            .then_with(|| a.title.cmp(&b.title))
    });
    (specs, decisions)
}

/// Everything about an issue an agent needs before working on it: its
/// fields, discussion, sub-issues, relations, and the specs and decisions it
/// follows (see [`issue_knowledge`]).
pub async fn get_issue(pool: &PgPool, actor: Actor, input: IssueRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let mut summary = issue_summary(&s, issue);
    let service = issue
        .service_id
        .and_then(|id| s.services.iter().find(|sv| sv.id == id));

    let relations: Vec<Value> = s
        .relations
        .iter()
        .filter_map(|r| {
            let (label, other) = if r.issue_id == issue.id {
                (format!("{:?}", r.kind).to_lowercase(), r.related_issue_id)
            } else if r.related_issue_id == issue.id {
                let label = match r.kind {
                    forge_domain::RelationType::Blocks => "blocked_by",
                    forge_domain::RelationType::Related => "related",
                    forge_domain::RelationType::Duplicate => "duplicated_by",
                };
                (label.to_string(), r.issue_id)
            } else {
                return None;
            };
            let other = s.issues.iter().find(|i| i.id == other)?;
            Some(json!({ "type": label, "issue": s.identifier(other), "title": other.title }))
        })
        .collect();

    let mut comments: Vec<_> = s
        .comments
        .iter()
        .filter(|c| c.issue_id == issue.id)
        .collect();
    comments.sort_by_key(|c| c.created_at);
    let (specs, decisions) = issue_knowledge(&s, issue);

    let extra = json!({
        "description": issue.description,
        "team": s.team_of(issue).map(|t| &t.key),
        "labels": s.labels.iter().filter(|l| issue.label_ids.contains(&l.id)).map(|l| &l.name).collect::<Vec<_>>(),
        "project": issue.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
        "service": service.map(|sv| &sv.name),
        "due_date": issue.due_date,
        "created_by": s.user_name(Some(issue.created_by)),
        "created_at": issue.created_at,
        "updated_at": issue.updated_at,
        "parent": issue.parent_id.and_then(|id| s.issues.iter().find(|i| i.id == id)).map(|p| issue_summary(&s, p)),
        "sub_issues": s.issues.iter().filter(|i| i.parent_id == Some(issue.id)).map(|i| issue_summary(&s, i)).collect::<Vec<_>>(),
        "relations": relations,
        "comments": comments.iter().map(|c| json!({
            "author": s.user_name(Some(c.author_id)), "at": c.created_at, "body": c.body,
        })).collect::<Vec<_>>(),
        "service_context": service.map(|sv| json!({
            "description": sv.description,
            "current_state": sv.current_state,
        })),
        "specs": specs.iter().map(|(sp, via)| json!({
            "id": sp.id, "title": sp.title, "status": sp.status, "via": via,
        })).collect::<Vec<_>>(),
        "decisions": decisions.iter().map(|(d, via)| json!({
            "id": d.id, "title": d.title, "status": d.status, "via": via,
        })).collect::<Vec<_>>(),
        "spec_change": spec_change(&s, issue),
    });
    if let (Value::Object(base), Value::Object(more)) = (&mut summary, extra) {
        base.extend(more);
    }
    if summary["spec_change"].is_object() {
        let standing = forge_sync::spec_change_standing(pool, actor.workspace_id, issue.id)
            .await
            .map_err(|e| e.to_string())?;
        // What keeps it from starting, and who else wants what it changes.
        summary["spec_change"]["held_back"] = json!(standing.held_back);
        summary["spec_change"]["also_wanted"] = json!(standing.also_wanted);
    }
    let history = forge_sync::issue_events(pool, actor, issue.id)
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    // The latest changes, enough to see how the issue got here.
    let start = history.len().saturating_sub(30);
    summary["history"] = history[start..]
        .iter()
        .map(|event| {
            json!({
                "at": event.at,
                "who": s.user_name(Some(event.actor_id)),
                "change": describe_change(&s, &event.change),
            })
        })
        .collect();
    pretty(summary)
}

/// The issue's spec change, as `get_issue` shows it; null when there's none.
fn spec_change(s: &Snapshot, issue: &Issue) -> Value {
    let Some(change) = s.spec_changes.iter().find(|c| c.issue_id == Some(issue.id)) else {
        return Value::Null;
    };
    let specs: Vec<Value> = change
        .specs
        .iter()
        .map(|d| {
            let spec = d.spec_id.and_then(|id| s.specs.iter().find(|sp| sp.id == id));
            let mut entry = match spec {
                Some(sp) => json!({ "spec": sp.title, "spec_id": sp.id }),
                None => json!({
                    "new_spec": d.new_title,
                    "service": d.new_service_id.and_then(|id| s.services.iter().find(|sv| sv.id == id)).map(|sv| &sv.name),
                }),
            };
            entry[if d.rewrite { "body" } else { "requirements" }] = json!(d.text);
            entry
        })
        .collect();
    json!({
        // Work can start only once it is: specs, or why none change.
        "written": change.is_written(),
        "specs": specs,
        "replaces": change.replaces_decision_ids.iter().filter_map(|id| s.decisions.iter().find(|d| d.id == *id)).map(|d| &d.title).collect::<Vec<_>>(),
        "no_spec_change": change.no_change_reason,
        // It says something else than when work on the issue started.
        "changed_after_start": change.changed_after_start(),
        // When the issue was done, and the specs that no longer fitted then.
        "applied_at": change.applied_at,
        "not_applied": change.not_applied.iter().map(|n| json!({ "spec": n.title, "why": n.reason })).collect::<Vec<_>>(),
    })
}

/// The spec changes applied to a spec, newest first: when, through which
/// issue, and what changed, in words.
fn spec_history(s: &Snapshot, spec: &Spec) -> Vec<Value> {
    let mut applied: Vec<&SpecChange> = s
        .spec_changes
        .iter()
        .filter(|c| c.applied_at.is_some() && c.applied_spec_ids.contains(&spec.id))
        .collect();
    applied.sort_by_key(|c| std::cmp::Reverse(c.applied_at));
    applied
        .into_iter()
        .map(|c| {
            let issue = s.issues.iter().find(|i| Some(i.id) == c.issue_id);
            let delta = c.specs.iter().find(|d| d.spec_id == Some(spec.id));
            json!({
                "applied_at": c.applied_at,
                "issue": issue.map(|i| s.identifier(i)),
                "title": issue.map(|i| &i.title),
                "change": delta.map_or_else(|| "created it".to_string(), describe_delta),
            })
        })
        .collect()
}

/// What a spec change did to one spec, e.g. "changed \"Review bar\"".
fn describe_delta(delta: &SpecDelta) -> String {
    if delta.rewrite {
        return "rewrote it".into();
    }
    let Ok(parsed) = parse_delta(&delta.text) else {
        return "changed it".into();
    };
    let names = |verb: &str, names: Vec<&str>| {
        (!names.is_empty()).then(|| {
            let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
            format!("{verb} {}", quoted.join(", "))
        })
    };
    let renamed: Vec<String> = parsed
        .renamed
        .iter()
        .map(|(from, to)| format!("renamed {from:?} to {to:?}"))
        .collect();
    [
        names(
            "added",
            parsed.added.iter().map(|r| r.name.as_str()).collect(),
        ),
        names(
            "changed",
            parsed.modified.iter().map(|r| r.name.as_str()).collect(),
        ),
        names(
            "removed",
            parsed.removed.iter().map(String::as_str).collect(),
        ),
        (!renamed.is_empty()).then(|| renamed.join(", ")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("; ")
}

/// A change to an issue in words, e.g. "moved it from Todo to In Progress".
fn describe_change(s: &Snapshot, change: &IssueChange) -> String {
    let state = |id: &uuid::Uuid| {
        s.states
            .iter()
            .find(|st| st.id == *id)
            .map(|st| st.name.clone())
    };
    let name = |found: Option<String>| found.unwrap_or_else(|| "something deleted".into());
    let user = |id: &Option<uuid::Uuid>| s.user_name(*id).unwrap_or_else(|| "no one".into());
    let label = |id: &uuid::Uuid| {
        name(
            s.labels
                .iter()
                .find(|l| l.id == *id)
                .map(|l| l.name.clone()),
        )
    };
    let project = |id: &Option<uuid::Uuid>| {
        id.and_then(|id| s.projects.iter().find(|p| p.id == id))
            .map_or_else(|| "none".into(), |p| p.name.clone())
    };
    let service = |id: &Option<uuid::Uuid>| {
        id.and_then(|id| s.services.iter().find(|sv| sv.id == id))
            .map_or_else(|| "none".into(), |sv| sv.name.clone())
    };
    let issue = |id: &Option<uuid::Uuid>| {
        id.and_then(|id| s.issues.iter().find(|i| i.id == id))
            .map_or_else(|| "none".into(), |i| s.identifier(i))
    };
    let team = |id: &uuid::Uuid| name(s.teams.iter().find(|t| t.id == *id).map(|t| t.key.clone()));
    match change {
        IssueChange::Created => "created it".into(),
        IssueChange::Title { to, .. } => format!("renamed it to {to:?}"),
        IssueChange::Description => "updated the description".into(),
        IssueChange::State { from, to } => {
            format!("moved it from {} to {}", name(state(from)), name(state(to)))
        }
        IssueChange::Priority { to, .. } => {
            format!(
                "set the priority to {}",
                PRIORITIES[(*to).clamp(0, 4) as usize]
            )
        }
        IssueChange::Assignee { to, .. } => format!("assigned it to {}", user(to)),
        IssueChange::Reviewer { to: None, .. } => "removed the reviewer".into(),
        IssueChange::Reviewer { to, .. } => format!("asked {} to review it", user(to)),
        IssueChange::NoCode { to: true } => "marked it as needing no code".into(),
        IssueChange::NoCode { to: false } => "removed the No code mark".into(),
        IssueChange::Labels { added, removed } => {
            let mut parts = Vec::new();
            if !added.is_empty() {
                parts.push(format!(
                    "added {}",
                    added.iter().map(label).collect::<Vec<_>>().join(", ")
                ));
            }
            if !removed.is_empty() {
                parts.push(format!(
                    "removed {}",
                    removed.iter().map(label).collect::<Vec<_>>().join(", ")
                ));
            }
            format!("labels: {}", parts.join("; "))
        }
        IssueChange::Project { to, .. } => format!("set the project to {}", project(to)),
        IssueChange::Parent { to, .. } => format!("set the parent to {}", issue(to)),
        IssueChange::Service { to, .. } => format!("set the service to {}", service(to)),
        IssueChange::DueDate { to, .. } => format!(
            "set the due date to {}",
            to.map_or_else(|| "none".into(), |d| d.to_string())
        ),
        IssueChange::Team { from, to } => format!("moved it from {} to {}", team(from), team(to)),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIssue {
    /// Team key, e.g. FOR.
    pub team: String,
    pub title: String,
    /// Markdown.
    pub description: Option<String>,
    /// Status name; defaults to the team's backlog.
    pub status: Option<String>,
    /// none, urgent, high, medium or low.
    pub priority: Option<String>,
    /// Name or email; "me" for yourself.
    pub assignee: Option<String>,
    /// The person asked to review it (never an agent): name or email.
    pub reviewer: Option<String>,
    /// Label names.
    pub labels: Option<Vec<String>>,
    /// Project id or name.
    pub project: Option<String>,
    /// Service id or name.
    pub service: Option<String>,
    /// Identifier of the parent issue, making this a sub-issue.
    pub parent: Option<String>,
    /// YYYY-MM-DD.
    pub due_date: Option<String>,
    /// true once you've looked at the similar open issues a first try
    /// showed you and none of them covers this one.
    pub confirm_new: Option<bool>,
}

/// Words that say little about what an issue is about.
const COMMON_WORDS: [&str; 52] = [
    "a", "about", "after", "all", "an", "and", "are", "as", "at", "be", "before", "but", "by",
    "can", "does", "doesn", "don", "for", "from", "has", "have", "how", "in", "into", "is", "it",
    "its", "no", "not", "now", "of", "on", "or", "so", "than", "that", "the", "their", "then",
    "them", "there", "they", "this", "to", "up", "was", "we", "what", "when", "which", "with",
    "without",
];

/// The words of a title that say what it's about, with plural and verb
/// endings dropped, so "fails" and "fail" or "requests" and "request" meet.
fn title_words(title: &str) -> Vec<String> {
    let mut words: Vec<String> = title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !COMMON_WORDS.contains(w))
        .map(|w| {
            for (ending, shortest) in [("ing", 6), ("ed", 5), ("s", 4)] {
                if w.len() >= shortest && w.ends_with(ending) && !w.ends_with("ss") {
                    return w[..w.len() - ending.len()].to_string();
                }
            }
            w.to_string()
        })
        .collect();
    words.sort();
    words.dedup();
    words
}

/// How alike two titles are, from 0 to 1: the share of the shorter title's
/// words that the other has too. Titles sharing fewer than two words count
/// as different, unless they're the same.
fn title_similarity(a: &[String], b: &[String]) -> f64 {
    if !a.is_empty() && a == b {
        return 1.0;
    }
    let shared = a.iter().filter(|w| b.contains(w)).count();
    if shared < 2 {
        return 0.0;
    }
    shared as f64 / a.len().min(b.len()) as f64
}

/// How alike titles must be before an agent is asked to check.
const SIMILAR_TITLES: f64 = 0.5;

/// Open issues whose titles are like `title`, most alike first (at most 5).
fn similar_open_issues<'a>(s: &'a Snapshot, title: &str) -> Vec<&'a Issue> {
    let words = title_words(title);
    let mut found: Vec<(f64, &Issue)> = s
        .issues
        .iter()
        .filter(|i| {
            !s.state_of(i).is_some_and(|st| {
                matches!(
                    st.kind,
                    forge_domain::StateType::Completed | forge_domain::StateType::Canceled
                )
            })
        })
        .map(|i| (title_similarity(&words, &title_words(&i.title)), i))
        .filter(|(score, _)| *score >= SIMILAR_TITLES)
        .collect();
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.updated_at.cmp(&a.1.updated_at))
    });
    found.into_iter().take(5).map(|(_, i)| i).collect()
}

/// Creates an issue. Agents file duplicates easily (FOR-44), so when open
/// issues have similar titles nothing is created yet: the agent gets them,
/// and has to say it's really new with `confirm_new`.
pub async fn create_issue(pool: &PgPool, actor: Actor, input: CreateIssue) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let team = s.team(&input.team)?;
    if input.confirm_new != Some(true) {
        let similar = similar_open_issues(&s, &input.title);
        if !similar.is_empty() {
            let list: Vec<String> = similar
                .iter()
                .map(|i| {
                    let status = s.state_of(i).map_or("", |st| st.name.as_str());
                    let assignee = s
                        .user_name(i.assignee_id)
                        .unwrap_or_else(|| "unassigned".into());
                    // Only issues waiting for an agent have a place in its queue.
                    let queue = s.queue.get(&i.id).map_or(String::new(), |position| {
                        format!(", queue_position {position}")
                    });
                    format!(
                        "- {}: {} ({status}, {assignee}{queue})",
                        s.identifier(i),
                        i.title
                    )
                })
                .collect();
            return Err(format!(
                "Not created yet: these open issues have similar titles.\n{}\n\
                 If one of them covers it, use that one (comment on it, or propose a change \
                 to its description) instead of filing a new issue. If it's really new, call \
                 create_issue again with confirm_new: true.",
                list.join("\n")
            ));
        }
    }
    let mut data = object(json!({ "team_id": team.id, "title": input.title }));
    if let Some(description) = input.description {
        data.insert("description".into(), json!(description));
    }
    if let Some(status) = &input.status {
        data.insert("state_id".into(), json!(s.state(team, status)?.id));
    }
    if let Some(priority) = &input.priority {
        data.insert("priority".into(), json!(priority_number(priority)?));
    }
    if let Some(assignee) = input.assignee.as_deref().filter(|a| !is_none(a)) {
        data.insert("assignee_id".into(), json!(s.user(assignee)?.id));
    }
    if let Some(reviewer) = input.reviewer.as_deref().filter(|r| !is_none(r)) {
        data.insert("reviewer_id".into(), json!(s.user(reviewer)?.id));
    }
    if let Some(labels) = &input.labels {
        let ids = labels
            .iter()
            .map(|l| s.label(team, l).map(|l| l.id))
            .collect::<ToolResult<Vec<_>>>()?;
        data.insert("label_ids".into(), json!(ids));
    }
    if let Some(project) = input.project.as_deref().filter(|p| !is_none(p)) {
        data.insert("project_id".into(), json!(s.project(project)?.id));
    }
    if let Some(service) = input.service.as_deref().filter(|sv| !is_none(sv)) {
        data.insert("service_id".into(), json!(s.service(service)?.id));
    }
    if let Some(parent) = input.parent.as_deref().filter(|p| !is_none(p)) {
        data.insert("parent_id".into(), json!(s.issue(parent)?.id));
    }
    if let Some(due) = input.due_date.as_deref().filter(|d| !is_none(d)) {
        data.insert("due_date".into(), json!(due));
    }

    let id = Uuid::now_v7();
    apply(
        pool,
        actor,
        vec![Operation::Create {
            model: ModelKind::Issue,
            id,
            data,
        }],
    )
    .await?;
    let s = Snapshot::load(pool, actor).await?;
    let issue = s
        .issues
        .iter()
        .find(|i| i.id == id)
        .ok_or("the issue vanished")?;
    pretty(json!({ "created": issue_summary(&s, issue) }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateIssue {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    pub title: Option<String>,
    /// Markdown. Replaces the whole description.
    pub description: Option<String>,
    /// Status name, e.g. "In Progress".
    pub status: Option<String>,
    /// none, urgent, high, medium or low.
    pub priority: Option<String>,
    /// Name or email; "me" for yourself; "none" to unassign.
    pub assignee: Option<String>,
    /// The person asked to review it (never an agent): name or email; "none" to remove.
    pub reviewer: Option<String>,
    /// Label names; replaces the issue's labels.
    pub labels: Option<Vec<String>>,
    /// Project id or name; "none" to remove.
    pub project: Option<String>,
    /// Service id or name; "none" to remove.
    pub service: Option<String>,
    /// Parent issue identifier; "none" to remove.
    pub parent: Option<String>,
    /// YYYY-MM-DD; "none" to clear.
    pub due_date: Option<String>,
}

pub async fn update_issue(pool: &PgPool, actor: Actor, input: UpdateIssue) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let team = s.team_of(issue).ok_or("the issue has no team")?;
    let mut data = Map::new();
    if let Some(title) = input.title {
        data.insert("title".into(), json!(title));
    }
    if let Some(description) = input.description {
        data.insert("description".into(), json!(description));
    }
    if let Some(status) = &input.status {
        data.insert("state_id".into(), json!(s.state(team, status)?.id));
    }
    if let Some(priority) = &input.priority {
        data.insert("priority".into(), json!(priority_number(priority)?));
    }
    if let Some(assignee) = &input.assignee {
        let id = if is_none(assignee) {
            None
        } else {
            Some(s.user(assignee)?.id)
        };
        data.insert("assignee_id".into(), json!(id));
    }
    if let Some(reviewer) = &input.reviewer {
        let id = if is_none(reviewer) {
            None
        } else {
            Some(s.user(reviewer)?.id)
        };
        data.insert("reviewer_id".into(), json!(id));
    }
    if let Some(labels) = &input.labels {
        let ids = labels
            .iter()
            .map(|l| s.label(team, l).map(|l| l.id))
            .collect::<ToolResult<Vec<_>>>()?;
        data.insert("label_ids".into(), json!(ids));
    }
    if let Some(project) = &input.project {
        let id = if is_none(project) {
            None
        } else {
            Some(s.project(project)?.id)
        };
        data.insert("project_id".into(), json!(id));
    }
    if let Some(service) = &input.service {
        let id = if is_none(service) {
            None
        } else {
            Some(s.service(service)?.id)
        };
        data.insert("service_id".into(), json!(id));
    }
    if let Some(parent) = &input.parent {
        let id = if is_none(parent) {
            None
        } else {
            Some(s.issue(parent)?.id)
        };
        data.insert("parent_id".into(), json!(id));
    }
    if let Some(due) = &input.due_date {
        data.insert(
            "due_date".into(),
            if is_none(due) {
                Value::Null
            } else {
                json!(due)
            },
        );
    }
    if data.is_empty() {
        return Err("nothing to change".into());
    }

    let id = issue.id;
    apply(
        pool,
        actor,
        vec![Operation::Update {
            model: ModelKind::Issue,
            id,
            data,
        }],
    )
    .await?;
    let s = Snapshot::load(pool, actor).await?;
    let issue = s
        .issues
        .iter()
        .find(|i| i.id == id)
        .ok_or("the issue vanished")?;
    pretty(json!({ "updated": issue_summary(&s, issue) }))
}

pub async fn delete_issue(pool: &PgPool, actor: Actor, input: IssueRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let identifier = s.identifier(issue);
    apply(
        pool,
        actor,
        vec![Operation::Delete {
            model: ModelKind::Issue,
            id: issue.id,
        }],
    )
    .await?;
    Ok(format!("Deleted {identifier}."))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommentOnIssue {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    /// Markdown.
    pub body: String,
}

pub async fn comment_on_issue(
    pool: &PgPool,
    actor: Actor,
    input: CommentOnIssue,
) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let data = object(json!({ "issue_id": issue.id, "body": input.body }));
    apply(
        pool,
        actor,
        vec![Operation::Create {
            model: ModelKind::Comment,
            id: Uuid::now_v7(),
            data,
        }],
    )
    .await?;
    Ok(format!("Commented on {}.", s.identifier(issue)))
}

/// Shows the team that the agent is working on the issue (FOR-24). Refused
/// while someone else is on it, or the agent is still on another (FOR-42).
pub async fn start_work(pool: &PgPool, actor: Actor, input: IssueRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    forge_sync::start_work(pool, actor, issue.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "You're working on {}. Call finish_work when you're done with it.",
        s.identifier(issue)
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinishWork {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    /// True when the work changes no code (a spec or decision only), so no
    /// pull request is coming: the issue gets the "No code" mark, and its
    /// reviewer can approve it without one.
    #[serde(default)]
    pub no_code: Option<bool>,
}

pub async fn finish_work(pool: &PgPool, actor: Actor, input: FinishWork) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let identifier = s.identifier(issue);
    let marked = input.no_code == Some(true) && !issue.no_code;
    if marked {
        apply(
            pool,
            actor,
            vec![Operation::Update {
                model: ModelKind::Issue,
                id: issue.id,
                data: object(json!({ "no_code": true })),
            }],
        )
        .await?;
    }
    let finished = forge_sync::finish_work(pool, actor, issue.id)
        .await
        .map_err(|e| e.to_string())?;
    let mark = if marked {
        " It's marked as needing no code."
    } else {
        ""
    };
    Ok(if finished {
        format!("Finished working on {identifier}.{mark}")
    } else {
        format!("You weren't working on {identifier}; nothing changed.{mark}")
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeDescription {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    /// The whole new description (Markdown), not just what changes.
    pub description: String,
    /// Why it should change (Markdown): what you found, who asked for it.
    pub why: String,
}

/// Proposes a new description for people to accept or dismiss on the issue.
/// The agent's earlier open proposal for the issue, if any, gives way to it.
pub async fn propose_description(
    pool: &PgPool,
    actor: Actor,
    input: ProposeDescription,
) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let identifier = s.identifier(issue);
    let description = input.description.trim();
    if description == issue.description.trim() {
        return Err(format!("that is already {identifier}'s description"));
    }
    if input.why.trim().is_empty() {
        return Err("say why the description should change".into());
    }
    IssuePatch {
        description: Some(description.to_string()),
        ..IssuePatch::default()
    }
    .validate()
    .map_err(|e| e.to_string())?;

    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    let earlier: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM suggestions WHERE workspace_id = $1 AND issue_id = $2 \
         AND kind = 'description' AND status = 'open' AND created_by = $3",
    )
    .bind(actor.workspace_id)
    .bind(issue.id)
    .bind(actor.user_id)
    .fetch_all(&mut *db)
    .await
    .map_err(|e| e.to_string())?;
    let mut operations: Vec<Operation> = earlier
        .into_iter()
        .map(|id| Operation::Update {
            model: ModelKind::Suggestion,
            id,
            data: object(json!({
                "status": "dismissed",
                "resolved_by": actor.user_id,
                "resolved_at": Utc::now(),
            })),
        })
        .collect();
    operations.push(Operation::Create {
        model: ModelKind::Suggestion,
        id: Uuid::now_v7(),
        data: object(json!({
            "kind": "description",
            "title": format!("New description for {identifier}"),
            "body": input.why.trim(),
            "changes": ["Replace the description"],
            "operations": [Operation::Update {
                model: ModelKind::Issue,
                id: issue.id,
                data: object(json!({ "description": description })),
            }],
            "issue_id": issue.id,
            "service_id": issue.service_id,
        })),
    });
    // Suggestions are the server's to write; accepting one applies its
    // change as whoever accepts, through the same rules as their own edits.
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    forge_sync::apply_in(&mut db, actor, &transaction, forge_sync::Origin::Server)
        .await
        .map_err(|e| e.to_string())?;
    db.commit().await.map_err(|e| e.to_string())?;
    Ok(format!(
        "Proposed a new description for {identifier}. It changes once someone accepts it on the issue."
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SuggestTriage {
    /// The issue's identifier, e.g. FOR-12. It must be in Triage.
    pub identifier: String,
    /// Why (Markdown, a few sentences): why these properties, and the
    /// evidence for any duplicate. If nothing should change, say why.
    pub why: String,
    /// urgent, high, medium or low.
    pub priority: Option<String>,
    /// Label names to add.
    pub labels: Option<Vec<String>>,
    /// Service id or name.
    pub service: Option<String>,
    /// Project id or name.
    pub project: Option<String>,
    /// A person's or agent's name or email; "me" for yourself.
    pub assignee: Option<String>,
    /// Identifier of the issue this one duplicates.
    pub duplicate_of: Option<String>,
    /// Identifiers of closely related issues.
    pub related: Option<Vec<String>>,
}

/// Suggests triage for an issue in Triage, the way Forge AI does: people
/// apply or dismiss it next to the issue. One waits per issue at a time.
pub async fn suggest_triage(
    pool: &PgPool,
    actor: Actor,
    input: SuggestTriage,
) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let identifier = s.identifier(issue);
    let state = s.state_of(issue);
    if !state.is_some_and(|st| st.kind == forge_domain::StateType::Triage) {
        return Err(format!(
            "{identifier} isn't in triage (it's in {}); only issues in triage get triage suggestions",
            state.map_or("an unknown status", |st| st.name.as_str())
        ));
    }
    let why = input.why.trim();
    if why.is_empty() {
        return Err("say why: what the suggestion is based on".into());
    }
    let suggest = triage::Suggest {
        explanation: why.to_string(),
        priority: input.priority,
        labels: input.labels.unwrap_or_default(),
        service: input.service,
        project: input.project,
        assignee: input.assignee,
        duplicate_of: input.duplicate_of,
        related: input.related.unwrap_or_default(),
    };
    let found = triage::changes(&s, issue, &suggest)?;
    let nothing = found.changes.is_empty();
    triage::save(pool, actor, &s, issue, why, found, None).await?;
    Ok(if nothing {
        format!(
            "Suggested leaving {identifier} as it is. People see why next to it in Triage, and it no longer needs triage."
        )
    } else {
        format!(
            "Suggested triage for {identifier}. People apply or dismiss it next to it in Triage."
        )
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelateIssues {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    /// blocks, blocked_by, related or duplicate_of.
    pub relation: String,
    /// The other issue's identifier.
    pub other: String,
}

pub async fn relate_issues(pool: &PgPool, actor: Actor, input: RelateIssues) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let other = s.issue(&input.other)?;
    let (from, to, kind) = match input.relation.trim().to_lowercase().as_str() {
        "blocks" => (issue, other, "blocks"),
        "blocked_by" => (other, issue, "blocks"),
        "related" => (issue, other, "related"),
        "duplicate_of" | "duplicate" => (issue, other, "duplicate"),
        unknown => {
            return Err(format!(
                "unknown relation {unknown:?}; use blocks, blocked_by, related or duplicate_of"
            ));
        }
    };
    let data = object(json!({ "issue_id": from.id, "related_issue_id": to.id, "type": kind }));
    apply(
        pool,
        actor,
        vec![Operation::Create {
            model: ModelKind::IssueRelation,
            id: Uuid::now_v7(),
            data,
        }],
    )
    .await?;
    Ok(format!(
        "{} {} {}.",
        s.identifier(issue),
        input.relation.trim(),
        s.identifier(other)
    ))
}

// Projects

pub async fn list_projects(pool: &PgPool, actor: Actor) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let projects: Vec<Value> = s
        .projects
        .iter()
        .map(|p| {
            let issues: Vec<&Issue> = s.issues.iter().filter(|i| i.project_id == Some(p.id)).collect();
            let closed = issues.iter().filter(|i| i.completed_at.is_some()).count();
            json!({
                "id": p.id,
                "name": p.name,
                "status": p.status,
                "lead": s.user_name(p.lead_id),
                "initiative": p.initiative_id.and_then(|id| s.initiatives.iter().find(|i| i.id == id)).map(|i| &i.name),
                "target_date": p.target_date,
                "issues": issues.len(),
                "closed_issues": closed,
            })
        })
        .collect();
    pretty(json!({ "projects": projects }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectRef {
    /// Project id or name.
    pub project: String,
}

pub async fn get_project(pool: &PgPool, actor: Actor, input: ProjectRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let p = s.project(&input.project)?;
    pretty(json!({
        "id": p.id,
        "name": p.name,
        "description": p.description,
        "status": p.status,
        "lead": s.user_name(p.lead_id),
        "target_date": p.target_date,
        "issues": s.issues.iter().filter(|i| i.project_id == Some(p.id)).map(|i| issue_summary(&s, i)).collect::<Vec<_>>(),
        "specs": s.specs.iter().filter(|sp| sp.project_id == Some(p.id)).map(|sp| json!({ "id": sp.id, "title": sp.title })).collect::<Vec<_>>(),
        "decisions": s.decisions.iter().filter(|d| d.project_id == Some(p.id)).map(|d| json!({ "id": d.id, "title": d.title, "status": d.status })).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveProject {
    /// Project id or name, to change an existing project. Leave out to create one.
    pub project: Option<String>,
    pub name: Option<String>,
    /// Markdown.
    pub description: Option<String>,
    /// planned, started, paused, completed or canceled.
    pub status: Option<String>,
    /// Name or email of the lead; "none" to remove.
    pub lead: Option<String>,
    /// Initiative id or name; "none" to remove.
    pub initiative: Option<String>,
    /// YYYY-MM-DD; "none" to clear.
    pub target_date: Option<String>,
}

pub async fn save_project(pool: &PgPool, actor: Actor, input: SaveProject) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let mut data = Map::new();
    if let Some(name) = &input.name {
        data.insert("name".into(), json!(name));
    }
    if let Some(description) = &input.description {
        data.insert("description".into(), json!(description));
    }
    if let Some(status) = &input.status {
        data.insert("status".into(), json!(status.trim().to_lowercase()));
    }
    if let Some(lead) = &input.lead {
        data.insert(
            "lead_id".into(),
            json!(if is_none(lead) {
                None
            } else {
                Some(s.user(lead)?.id)
            }),
        );
    }
    if let Some(initiative) = &input.initiative {
        let id = if is_none(initiative) {
            None
        } else {
            Some(s.initiative(initiative)?.id)
        };
        data.insert("initiative_id".into(), json!(id));
    }
    if let Some(date) = &input.target_date {
        data.insert(
            "target_date".into(),
            if is_none(date) {
                Value::Null
            } else {
                json!(date)
            },
        );
    }
    save(
        pool,
        actor,
        &s,
        ModelKind::Project,
        input.project.as_deref(),
        |s, p| s.project(p).map(|p| p.id),
        data,
        "name",
    )
    .await
}

/// Creates a record, or updates the one `existing` names.
#[allow(clippy::too_many_arguments)]
async fn save(
    pool: &PgPool,
    actor: Actor,
    s: &Snapshot,
    model: ModelKind,
    existing: Option<&str>,
    find: impl Fn(&Snapshot, &str) -> ToolResult<Uuid>,
    mut data: Map<String, Value>,
    required: &str,
) -> ToolResult<String> {
    match existing {
        Some(existing) => {
            let id = find(s, existing)?;
            if data.is_empty() {
                return Err("nothing to change".into());
            }
            apply(pool, actor, vec![Operation::Update { model, id, data }]).await?;
            Ok(format!("Updated {} {id}.", model.name()))
        }
        None => {
            if !data.contains_key(required) {
                return Err(format!(
                    "give a {required} to create a {}",
                    model.name().to_lowercase()
                ));
            }
            // Clearing is meaningless on create.
            data.retain(|_, v| !v.is_null());
            let id = Uuid::now_v7();
            apply(pool, actor, vec![Operation::Create { model, id, data }]).await?;
            Ok(format!("Created {} {id}.", model.name()))
        }
    }
}

// Services, specs and decisions

pub async fn list_services(pool: &PgPool, actor: Actor) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    pretty(json!({
        "services": s.services.iter().map(|sv| json!({
            "id": sv.id,
            "name": sv.name,
            "repository": sv.repository,
            "paths": sv.paths,
            "team": sv.team_id.and_then(|id| s.teams.iter().find(|t| t.id == id)).map(|t| &t.key),
            "specs": s.specs.iter().filter(|sp| sp.service_ids.contains(&sv.id)).count(),
            "decisions": s.decisions.iter().filter(|d| d.service_id == Some(sv.id)).count(),
        })).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ServiceRef {
    /// Service id or name.
    pub service: String,
}

/// A service with its specs and decisions in full and its open issues: the
/// context to work on it.
pub async fn get_service(pool: &PgPool, actor: Actor, input: ServiceRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let sv = s.service(&input.service)?;
    let open: Vec<Value> = s
        .issues
        .iter()
        .filter(|i| i.service_id == Some(sv.id) && i.completed_at.is_none())
        .map(|i| issue_summary(&s, i))
        .collect();
    pretty(json!({
        "id": sv.id,
        "name": sv.name,
        "description": sv.description,
        "repository": sv.repository,
        "paths": sv.paths,
        "current_state": sv.current_state,
        "current_state_updated_at": sv.current_state_updated_at,
        "specs": s.specs.iter().filter(|sp| sp.service_ids.contains(&sv.id)).map(|sp| json!({
            "id": sp.id, "title": sp.title, "status": sp.status, "body": sp.body,
        })).collect::<Vec<_>>(),
        "decisions": s.decisions.iter().filter(|d| d.service_id == Some(sv.id)).map(|d| json!({
            "id": d.id, "title": d.title, "status": d.status, "decided_on": d.decided_on,
            "context": d.context, "decision": d.decision, "consequences": d.consequences,
        })).collect::<Vec<_>>(),
        "open_issues": open,
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveService {
    /// Service id or name, to change an existing service. Leave out to create one.
    pub service: Option<String>,
    pub name: Option<String>,
    /// Markdown: what the service is for.
    pub description: Option<String>,
    /// Team key; "none" to remove.
    pub team: Option<String>,
    /// "github:owner/repo" or a folder on the server (relative to its
    /// repositories folder); "none" to remove.
    pub repository: Option<String>,
    /// Globs of the paths in the repository that belong to the service.
    pub paths: Option<Vec<String>>,
    /// Markdown: what the code does today.
    pub current_state: Option<String>,
}

pub async fn save_service(pool: &PgPool, actor: Actor, input: SaveService) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let mut data = Map::new();
    if let Some(name) = &input.name {
        data.insert("name".into(), json!(name));
    }
    if let Some(description) = &input.description {
        data.insert("description".into(), json!(description));
    }
    if let Some(team) = &input.team {
        data.insert(
            "team_id".into(),
            json!(if is_none(team) {
                None
            } else {
                Some(s.team(team)?.id)
            }),
        );
    }
    if let Some(repository) = &input.repository {
        data.insert(
            "repository".into(),
            if is_none(repository) {
                Value::Null
            } else {
                json!(repository)
            },
        );
    }
    if let Some(paths) = &input.paths {
        data.insert("paths".into(), json!(paths));
    }
    if let Some(current) = &input.current_state {
        data.insert("current_state".into(), json!(current));
        if input.service.is_some() {
            data.insert("current_state_updated_at".into(), json!(chrono::Utc::now()));
        }
    }
    save(
        pool,
        actor,
        &s,
        ModelKind::Service,
        input.service.as_deref(),
        |s, sv| s.service(sv).map(|sv| sv.id),
        data,
        "name",
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListKnowledge {
    /// Only those for this service (id or name).
    pub service: Option<String>,
}

pub async fn list_specs(pool: &PgPool, actor: Actor, input: ListKnowledge) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let service = input
        .service
        .as_deref()
        .map(|sv| s.service(sv))
        .transpose()?;
    pretty(json!({
        "specs": s.specs.iter().filter(|sp| service.is_none_or(|sv| sp.service_ids.contains(&sv.id))).map(|sp| json!({
            "id": sp.id, "title": sp.title, "status": sp.status,
            "domain": s.domain_of(sp),
            "services": service_names(&s, sp),
            "project": sp.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
            "updated_at": sp.updated_at,
        })).collect::<Vec<_>>(),
    }))
}

/// The names of the services a spec belongs to.
fn service_names<'a>(s: &'a Snapshot, sp: &Spec) -> Vec<&'a str> {
    sp.service_ids
        .iter()
        .filter_map(|id| s.services.iter().find(|sv| sv.id == *id))
        .map(|sv| sv.name.as_str())
        .collect()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpecRef {
    /// Spec id or title.
    pub spec: String,
    /// true for only its requirements' and their scenarios' names, without
    /// its text and history.
    pub outline: Option<bool>,
}

pub async fn get_spec(pool: &PgPool, actor: Actor, input: SpecRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let sp = s.spec(&input.spec)?;
    if input.outline == Some(true) {
        return pretty(json!({
            "id": sp.id,
            "title": sp.title,
            "status": sp.status,
            "domain": s.domain_of(sp),
            "services": service_names(&s, sp),
            "project": sp.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
            "updated_at": sp.updated_at,
            "requirements": requirements(&sp.body).iter().map(|r| json!({
                "name": r.name,
                "scenarios": scenario_names(&r.text),
            })).collect::<Vec<_>>(),
        }));
    }
    let verdicts = spec_verdicts(pool, actor, sp).await?;
    pretty(json!({
        "id": sp.id,
        "title": sp.title,
        "status": sp.status,
        "domain": s.domain_of(sp),
        "services": service_names(&s, sp),
        "project": sp.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
        "body": sp.body,
        "updated_at": sp.updated_at,
        // The issues whose spec changes it took in, newest first.
        "history": spec_history(&s, sp),
        "verdicts": verdicts,
    }))
}

#[derive(sqlx::FromRow)]
struct LatestVerdict {
    requirement: String,
    verdict: String,
    evidence: String,
    requirement_text: String,
    commit_sha: Option<String>,
    checked_at: chrono::DateTime<Utc>,
}

/// Each requirement's latest verdict, in the spec's order, so an agent sees
/// what was judged, on what evidence and at which commit. A verdict on a
/// requirement the spec no longer has isn't listed.
async fn spec_verdicts(pool: &PgPool, actor: Actor, spec: &Spec) -> ToolResult<Vec<Value>> {
    let stored: Vec<LatestVerdict> = sqlx::query_as(
        "SELECT requirement, verdict, evidence, requirement_text, commit_sha, checked_at \
         FROM requirement_verdicts WHERE workspace_id = $1 AND spec_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(spec.id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(requirements(&spec.body)
        .iter()
        .filter_map(|r| {
            let v = stored.iter().find(|v| v.requirement == r.name)?;
            Some(json!({
                "requirement": r.name,
                "verdict": v.verdict,
                "evidence": v.evidence,
                "commit": v.commit_sha,
                "checked_at": v.checked_at,
                // As the spec page decides it.
                "requirement_changed_since": v.requirement_text.trim_end() != r.text,
            }))
        })
        .collect())
}

/// The names of a requirement's `#### Scenario:`s, in order.
fn scenario_names(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("#### Scenario:"))
        .map(str::trim)
        .collect()
}

/// How many of the best matches come back in full, and how many more by
/// name only.
const SEARCH_IN_FULL: usize = 10;
const SEARCH_BY_NAME: usize = 30;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSpecs {
    /// The word and its alternatives, in any case, e.g.
    /// `GitHub "sign in" login OAuth`; a quoted phrase counts as one word.
    pub query: String,
    /// Only the specs of this service (id or name).
    pub service: Option<String>,
}

/// A search's words: quoted phrases whole, the rest split on spaces, each
/// once.
fn search_words(query: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for (i, part) in query.split('"').enumerate() {
        let found: Vec<&str> = if i % 2 == 1 {
            vec![part.trim()]
        } else {
            part.split_whitespace().collect()
        };
        for word in found.into_iter().filter(|w| !w.is_empty()) {
            if !words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
                words.push(word.to_string());
            }
        }
    }
    words
}

/// The requirements that mention any of the words, those mentioning more of
/// them first, so one search with the alternatives finds them all and an
/// agent doesn't read whole specs. Specs without requirements are prose and
/// are only named.
pub async fn search_specs(pool: &PgPool, actor: Actor, input: SearchSpecs) -> ToolResult<String> {
    let words = search_words(&input.query);
    if words.is_empty() {
        return Err("give the words to look for".into());
    }
    let s = Snapshot::load(pool, actor).await?;
    let service = input
        .service
        .as_deref()
        .map(|sv| s.service(sv))
        .transpose()?;
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let mentioned = |text: &str| -> Vec<&str> {
        let text = text.to_lowercase();
        words
            .iter()
            .zip(&lower)
            .filter(|(_, w)| text.contains(w.as_str()))
            .map(|(word, _)| word.as_str())
            .collect()
    };
    let mut found = Vec::new();
    let mut prose = Vec::new();
    for sp in s
        .specs
        .iter()
        .filter(|sp| service.is_none_or(|sv| sp.service_ids.contains(&sv.id)))
    {
        let reqs = requirements(&sp.body);
        if reqs.is_empty() {
            let matched = mentioned(&format!("{}\n{}", sp.title, sp.body));
            if !matched.is_empty() {
                prose.push((sp, matched));
            }
            continue;
        }
        for r in reqs {
            let matched = mentioned(&r.text);
            if !matched.is_empty() {
                found.push((sp, r, matched));
            }
        }
    }
    if found.is_empty() && prose.is_empty() {
        return Ok(format!(
            "No spec mentions any of: {}. The specs don't cover it.",
            words.join(", ")
        ));
    }
    // Stable, so equal matches keep the specs' order.
    found.sort_by_key(|(_, _, matched)| std::cmp::Reverse(matched.len()));
    prose.sort_by_key(|(_, matched)| std::cmp::Reverse(matched.len()));
    let more = found.len().saturating_sub(SEARCH_IN_FULL + SEARCH_BY_NAME);
    let mut found = found.into_iter();
    let in_full: Vec<Value> = found
        .by_ref()
        .take(SEARCH_IN_FULL)
        .map(|(sp, r, matched)| {
            json!({
                "spec_id": sp.id, "spec": sp.title, "requirement": r.name,
                "matched": matched, "text": r.text,
            })
        })
        .collect();
    let by_name: Vec<Value> = found
        .take(SEARCH_BY_NAME)
        .map(|(sp, r, matched)| {
            json!({ "spec_id": sp.id, "spec": sp.title, "requirement": r.name, "matched": matched })
        })
        .collect();
    pretty(json!({
        "requirements": in_full,
        // Further matches without their text: get_spec reads them.
        "also": by_name,
        "more": more,
        // Specs without requirements that mention the words: get_spec reads them.
        "prose_specs": prose.iter().map(|(sp, matched)| json!({
            "spec_id": sp.id, "spec": sp.title, "matched": matched,
        })).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveSpec {
    /// Spec id or title, to change an existing spec. Leave out to create one.
    pub spec: Option<String>,
    pub title: Option<String>,
    /// Markdown. Replaces the whole spec.
    pub body: Option<String>,
    /// draft, active or deprecated.
    pub status: Option<String>,
    /// The services whose code it describes, by id or name; replaces its
    /// services. An empty list for a spec about the whole workspace.
    pub services: Option<Vec<String>>,
    /// The domain it sits in, the area of the product it's about (one per
    /// spec, like a folder), by id or name, such as Accounts; "none" to
    /// remove. Domains are made in the app.
    pub domain: Option<String>,
    /// Project id or name; "none" to remove.
    pub project: Option<String>,
    /// For agents: the issue whose spec change this applies, e.g. FOR-12. It
    /// must be done, and its spec change must change this spec.
    pub applying: Option<String>,
}

/// Agents don't edit specs (decision "Specs change only when the work is
/// done"): they write an issue's spec change, and only apply it here once
/// the issue is done. People edit specs in the app.
fn check_applying(s: &Snapshot, input: &SaveSpec) -> ToolResult<()> {
    let agent = s
        .users
        .iter()
        .any(|u| u.id == s.actor.user_id && u.kind == UserKind::Agent);
    if !agent {
        return Ok(());
    }
    let Some(applying) = &input.applying else {
        return Err("Agents don't edit specs directly (decision \"Specs change only when the work is done\"): \
             write the issue's spec change with save_spec_change. It takes effect when the issue is done. \
             To apply the spec change of an issue that's done, say which with `applying`."
            .into());
    };
    let issue = s.issue(applying)?;
    let identifier = s.identifier(issue);
    if !s
        .state_of(issue)
        .is_some_and(|st| st.kind == StateType::Completed)
    {
        return Err(format!(
            "{identifier} isn't done, so its spec change hasn't taken effect yet"
        ));
    }
    let deltas = s
        .spec_changes
        .iter()
        .find(|c| c.issue_id == Some(issue.id))
        .map_or(&[][..], |c| &c.specs[..]);
    let changes_it = match &input.spec {
        Some(spec) => {
            let id = s.spec(spec)?.id;
            deltas.iter().any(|d| d.spec_id == Some(id))
        }
        None => deltas.iter().any(|d| {
            d.spec_id.is_none()
                && d.new_title
                    .as_deref()
                    .zip(input.title.as_deref())
                    .is_some_and(|(a, b)| forge_domain::same_spec_name(a, b))
        }),
    };
    if changes_it {
        Ok(())
    } else {
        Err(format!(
            "{identifier}'s spec change doesn't change this spec"
        ))
    }
}

pub async fn save_spec(pool: &PgPool, actor: Actor, input: SaveSpec) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    check_applying(&s, &input)?;
    let mut data = Map::new();
    if let Some(title) = &input.title {
        data.insert("title".into(), json!(title));
    }
    if let Some(body) = &input.body {
        data.insert("body".into(), json!(body));
    }
    if let Some(status) = &input.status {
        data.insert("status".into(), json!(status.trim().to_lowercase()));
    }
    if let Some(domain) = &input.domain {
        let domain = domain.trim();
        let id = if is_none(domain) {
            None
        } else {
            Some(s.domain(domain)?.id)
        };
        data.insert("domain_id".into(), json!(id));
    }
    if let Some(services) = &input.services {
        let ids = services
            .iter()
            .map(|sv| s.service(sv).map(|found| found.id))
            .collect::<Result<Vec<_>, _>>()?;
        data.insert("service_ids".into(), json!(ids));
    }
    if let Some(project) = &input.project {
        data.insert(
            "project_id".into(),
            json!(if is_none(project) {
                None
            } else {
                Some(s.project(project)?.id)
            }),
        );
    }
    // The body as saved, to check it against the convention.
    let body = match (&input.body, &input.spec) {
        (Some(body), _) => body.clone(),
        (None, Some(existing)) => s.spec(existing)?.body.clone(),
        (None, None) => String::new(),
    };
    let saved = save(
        pool,
        actor,
        &s,
        ModelKind::Spec,
        input.spec.as_deref(),
        |s, sp| s.spec(sp).map(|sp| sp.id),
        data,
        "title",
    )
    .await?;
    let warnings = convention_warnings(&body);
    if warnings.is_empty() {
        return Ok(saved);
    }
    let list: Vec<String> = warnings.iter().map(|w| format!("- {w}")).collect();
    Ok(format!(
        "{saved}\n\nIt doesn't follow OpenSpec's convention for behaviour specs yet:\n{}",
        list.join("\n")
    ))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveSpecChange {
    /// The issue's identifier, e.g. FOR-12.
    pub identifier: String,
    /// The spec it changes: id or title.
    pub spec: Option<String>,
    /// A new spec's title, instead of `spec`. A new spec is written whole, as `body`.
    pub new_spec: Option<String>,
    /// A new spec's service: id or name.
    pub service: Option<String>,
    /// What changes in the spec's requirements, in OpenSpec's delta format:
    /// `## ADDED Requirements`, `## MODIFIED Requirements`, `## REMOVED
    /// Requirements`, `## RENAMED Requirements`.
    pub requirements: Option<String>,
    /// The spec's whole new text, instead of `requirements`: a rewrite, or a new spec.
    pub body: Option<String>,
    /// true takes the spec out of the issue's spec change.
    pub drop: Option<bool>,
    /// Ids or titles of the decisions the issue replaces. Replaces the list; [] clears it.
    pub replaces: Option<Vec<String>>,
    /// Why no spec changes, when none does; "none" clears it.
    pub no_spec_change: Option<String>,
}

/// Writes part of an issue's spec change: one spec's, the decisions it
/// replaces, or why no spec changes. The rest of it stays as it is.
pub async fn save_spec_change(
    pool: &PgPool,
    actor: Actor,
    input: SaveSpecChange,
) -> ToolResult<String> {
    // Writes to the workspace wait from here, so who's working on the issue,
    // the other spec changes and the time each part is stamped with stay as
    // read until this one is saved.
    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    forge_sync::lock_workspace(&mut db, actor.workspace_id)
        .await
        .map_err(|e| e.to_string())?;
    let s = Snapshot::load(pool, actor).await?;
    let issue = s.issue(&input.identifier)?;
    let identifier = s.identifier(issue);
    if s.state_of(issue)
        .is_some_and(|st| matches!(st.kind, StateType::Completed | StateType::Canceled))
    {
        return Err(format!(
            "{identifier} is closed, so its spec change can't change any more; a further change goes on a new issue"
        ));
    }
    let lease =
        chrono::Utc::now() - chrono::TimeDelta::minutes(forge_sync::WORK_LEASE_MINUTES.into());
    if let Some(worker) = s.users.iter().find(|u| {
        u.id != actor.user_id
            && u.working_on_id == Some(issue.id)
            && u.working_seen_at.is_some_and(|seen| seen > lease)
    }) {
        return Err(format!(
            "{} is working on {identifier}, so its spec change is theirs to write",
            worker.name
        ));
    }
    let existing = s.spec_changes.iter().find(|c| c.issue_id == Some(issue.id));
    if existing.is_some_and(|c| c.applied_at.is_some()) {
        return Err(format!(
            "{identifier}'s spec change has already taken effect; a further change goes on a new issue"
        ));
    }
    let mut specs: Vec<SpecDelta> = existing.map(|c| c.specs.clone()).unwrap_or_default();
    let mut replaces: Vec<Uuid> = existing
        .map(|c| c.replaces_decision_ids.clone())
        .unwrap_or_default();
    let mut reason = existing.and_then(|c| c.no_change_reason.clone());
    let mut done: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let names_spec = input.spec.is_some() || input.new_spec.is_some();
    if !names_spec
        && (input.requirements.is_some()
            || input.body.is_some()
            || input.drop.is_some()
            || input.service.is_some())
    {
        return Err("say which spec: `spec` for an existing one, `new_spec` for a new one".into());
    }
    if names_spec {
        let (delta, name) = spec_delta(&s, &input, &mut warnings)?;
        // Two open issues writing a new spec of the same name would collide
        // only when the second is done; one writes it at a time.
        // Saving its own again keeps its place, first or not.
        let has_it = |title: &str| {
            existing.is_some_and(|c| {
                c.specs.iter().any(|d| {
                    d.spec_id.is_none()
                        && d.new_title
                            .as_deref()
                            .is_some_and(|t| forge_domain::same_spec_name(t, title))
                })
            })
        };
        if let Some(title) = delta
            .new_title
            .as_deref()
            .filter(|title| input.drop != Some(true) && !has_it(title))
            && let Some(other) = open_spec_changes(&s).find(|c| {
                c.issue_id != Some(issue.id)
                    && c.specs.iter().any(|d| {
                        d.spec_id.is_none()
                            && d.new_title
                                .as_deref()
                                .is_some_and(|t| forge_domain::same_spec_name(t, title))
                    })
            })
        {
            let other = s
                .issues
                .iter()
                .find(|i| Some(i.id) == other.issue_id)
                .map(|i| s.identifier(i))
                .unwrap_or_default();
            return Err(format!(
                "{other}'s spec change writes a new spec {title:?} too, and only one issue writes a spec at a time: settle it with them, or change that spec with `spec` once {other} is done"
            ));
        }
        let same = |d: &SpecDelta| match (delta.spec_id, d.spec_id) {
            (Some(a), Some(b)) => a == b,
            (None, None) => d
                .new_title
                .as_deref()
                .zip(delta.new_title.as_deref())
                .is_some_and(|(a, b)| forge_domain::same_spec_name(a, b)),
            _ => false,
        };
        if input.drop == Some(true) {
            let before = specs.len();
            specs.retain(|d| !same(d));
            if specs.len() == before {
                return Err(format!("{identifier}'s spec change doesn't change {name}"));
            }
            done.push(format!("took out {name}"));
        } else {
            match specs.iter_mut().find(|d| same(d)) {
                Some(d) => {
                    // Its place among changes to the same requirements stays;
                    // a requirement it touches only now gets its place now.
                    // Becoming a rewrite (or no longer one) touches what it
                    // didn't before: its place for that starts now.
                    // A part saved before Forge kept these counts from when
                    // its spec change was written, as conflicts read it.
                    let first = d.written_at.or(existing.map(|c| c.created_at));
                    let written_at = if d.rewrite == delta.rewrite {
                        first.or(delta.written_at)
                    } else {
                        delta.written_at
                    };
                    let before = touched(d).unwrap_or_default();
                    let touched_at = delta
                        .touched_at
                        .iter()
                        .map(|(name, now)| {
                            let since = d
                                .touched_at
                                .get(name)
                                .copied()
                                .or_else(|| before.contains(name).then_some(first).flatten());
                            (name.clone(), since.unwrap_or(*now))
                        })
                        .collect();
                    *d = SpecDelta {
                        written_at,
                        touched_at,
                        ..delta
                    };
                    done.push(format!("replaced the change to {name}"));
                }
                None => {
                    specs.push(delta);
                    done.push(format!("changes {name}"));
                }
            }
            if reason.take().is_some() {
                done.push("no longer says no spec changes".into());
            }
        }
    }
    if let Some(list) = &input.replaces {
        replaces.clear();
        for wanted in list.iter().filter(|d| !is_none(d)) {
            let decision = s.decision(wanted)?;
            if !replaces.contains(&decision.id) {
                replaces.push(decision.id);
            }
        }
        done.push(if replaces.is_empty() {
            "replaces no decision".into()
        } else {
            format!("replaces {} decision(s)", replaces.len())
        });
    }
    if let Some(why) = &input.no_spec_change {
        if is_none(why) {
            reason = None;
            done.push("no longer says no spec changes".into());
        } else if !specs.is_empty() {
            return Err(format!(
                "{identifier}'s spec change changes specs; `drop` them first if none changes"
            ));
        } else {
            reason = Some(why.trim().to_string());
            done.push("says no spec changes".into());
        }
    }
    if done.is_empty() {
        return Err("nothing to change: give a spec, `replaces` or `no_spec_change`".into());
    }

    // Coming second to an issue this one already goes before, directly or
    // through others, would leave both waiting for good.
    let id = existing.map_or_else(Uuid::now_v7, |c| c.id);
    let now = chrono::Utc::now();
    let mine = SpecChange {
        id,
        specs: specs.clone(),
        ..existing.cloned().unwrap_or(SpecChange {
            id,
            workspace_id: actor.workspace_id,
            issue_id: Some(issue.id),
            specs: vec![],
            replaces_decision_ids: vec![],
            no_change_reason: None,
            applied_at: None,
            applied_spec_ids: vec![],
            not_applied: vec![],
            as_started: None,
            created_by: actor.user_id,
            created_at: now,
            updated_at: now,
        })
    };
    let others: Vec<SpecChange> = open_spec_changes(&s)
        .filter(|c| c.id != id)
        .cloned()
        .collect();
    let looped_with = |mine: Option<SpecChange>| {
        let open: Vec<SpecChange> = others.iter().cloned().chain(mine).collect();
        forge_domain::held_in_a_loop(&forge_domain::holds(&open), id)
    };
    let before = looped_with(existing.cloned());
    let looped: Vec<String> = looped_with(Some(mine))
        .into_iter()
        .filter(|change| !before.contains(change))
        .filter_map(|change| s.spec_changes.iter().find(|c| c.id == change)?.issue_id)
        .filter_map(|issue| s.issues.iter().find(|i| i.id == issue))
        .map(|i| s.identifier(i))
        .collect();
    if !looped.is_empty() {
        return Err(format!(
            "{identifier} would come second to {} on a requirement or a new spec, while {identifier} already goes first for them (directly or through other issues), so neither could ever start. Settle it with them first: wait until it's done, leave this part out, combine the issues, or split the requirement.",
            looped.join(", ")
        ));
    }

    let data = object(json!({
        "specs": specs,
        "replaces_decision_ids": replaces,
        "no_change_reason": reason,
    }));
    let operation = match existing {
        Some(change) => Operation::Update {
            model: ModelKind::SpecChange,
            id: change.id,
            data,
        },
        None => {
            let mut data = data;
            data.insert("issue_id".into(), json!(issue.id));
            Operation::Create {
                model: ModelKind::SpecChange,
                id,
                data,
            }
        }
    };
    // Spec changes are written through these tools only, for now.
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations: vec![operation],
    };
    forge_sync::apply_in(&mut db, actor, &transaction, forge_sync::Origin::Server)
        .await
        .map_err(|e| e.to_string())?;
    db.commit().await.map_err(|e| e.to_string())?;

    let mut saved = format!(
        "Saved {identifier}'s spec change: it {}. It takes effect when {identifier} is done.",
        done.join("; ")
    );
    if s.state_of(issue)
        .is_some_and(|st| st.kind == StateType::Started)
    {
        saved.push_str(&format!(
            "\n\nWork on {identifier} has started, so its page shows the spec change was changed \
             after that, next to what it said then. Say why in a comment on {identifier}."
        ));
    }
    let standing = forge_sync::spec_change_standing(pool, actor.workspace_id, issue.id)
        .await
        .map_err(|e| e.to_string())?;
    if !standing.held_back.is_empty() {
        saved.push_str(&format!(
            "\n\n{identifier} can't start yet: {}",
            standing.held_back.join(" ")
        ));
    }
    if warnings.is_empty() {
        return Ok(saved);
    }
    let list: Vec<String> = warnings.iter().map(|w| format!("- {w}")).collect();
    Ok(format!(
        "{saved}\n\nIt doesn't follow OpenSpec's convention for behaviour specs yet:\n{}",
        list.join("\n")
    ))
}

/// The spec changes of open issues (not done or canceled) that haven't taken
/// effect.
fn open_spec_changes(s: &Snapshot) -> impl Iterator<Item = &SpecChange> {
    s.spec_changes.iter().filter(|c| {
        c.applied_at.is_none()
            && c.issue_id
                .and_then(|id| s.issues.iter().find(|i| i.id == id))
                .and_then(|i| s.state_of(i))
                .is_some_and(|st| !matches!(st.kind, StateType::Completed | StateType::Canceled))
    })
}

/// One spec's part of a spec change, checked against the spec as it is now,
/// and the spec's name in words. Convention problems go into `warnings`.
fn spec_delta(
    s: &Snapshot,
    input: &SaveSpecChange,
    warnings: &mut Vec<String>,
) -> ToolResult<(SpecDelta, String)> {
    let spec = match (&input.spec, &input.new_spec) {
        (Some(_), Some(_)) => return Err("give `spec` or `new_spec`, not both".into()),
        (Some(spec), None) => Some(s.spec(spec)?),
        _ => None,
    };
    let new_title = input.new_spec.as_deref().map(str::trim);
    if let Some(title) = new_title
        && let Some(taken) = s
            .specs
            .iter()
            .find(|sp| forge_domain::same_spec_name(&sp.title, title))
    {
        return Err(format!(
            "there's already a spec {:?}: change it with `spec`",
            taken.title
        ));
    }
    let name = match (spec, new_title) {
        (Some(sp), _) => format!("{:?}", sp.title),
        (None, title) => format!("the new spec {:?}", title.unwrap_or_default()),
    };
    let (text, rewrite) = match (&input.requirements, &input.body, input.drop) {
        (None, None, Some(true)) => (String::new(), false),
        (_, _, Some(true)) => {
            return Err("`drop` takes the spec out; leave out `requirements` and `body`".into());
        }
        (Some(requirements), None, _) => (requirements.trim().to_string(), false),
        (None, Some(body), _) => (body.trim().to_string(), true),
        (Some(_), Some(_), _) => return Err("give `requirements` or `body`, not both".into()),
        (None, None, _) => {
            return Err("give `requirements` (what changes, in OpenSpec's delta format) or `body` (the whole new spec)".into());
        }
    };
    let new_service_id = match (&input.service, spec) {
        (Some(_), Some(_)) => return Err("`service` is only for a new spec".into()),
        (Some(service), None) if !is_none(service) => Some(s.service(service)?.id),
        _ => None,
    };
    let delta = SpecDelta {
        spec_id: spec.map(|sp| sp.id),
        new_title: new_title.map(Into::into),
        new_service_id,
        text,
        rewrite,
        written_at: Some(chrono::Utc::now()),
        touched_at: Default::default(),
        based_on: spec.map(|sp| sp.body.clone()),
    };
    let mut delta = delta;
    delta.touched_at = touched(&delta)
        .unwrap_or_default()
        .into_iter()
        .map(|name| (name, chrono::Utc::now()))
        .collect();
    if input.drop == Some(true) {
        return Ok((delta, name));
    }
    delta.validate().map_err(|e| e.to_string())?;
    match spec {
        _ if rewrite => warnings.extend(convention_warnings(&delta.text)),
        None => {}
        Some(sp) => {
            let parsed = parse_delta(&delta.text).map_err(|problems| problems.join(" "))?;
            // Prose: no `## Requirements` section (which would warn), and no requirements.
            if convention_warnings(&sp.body).is_empty() && requirements(&sp.body).is_empty() {
                return Err(format!(
                    "{name} isn't written in OpenSpec's convention yet, so it has no requirements to change: rewrite it whole, as `body`"
                ));
            }
            let current: Vec<String> = requirements(&sp.body).into_iter().map(|r| r.name).collect();
            let has = |wanted: &str| current.iter().any(|c| c == wanted);
            let mut problems: Vec<String> = Vec::new();
            for wanted in parsed.needs().filter(|n| !has(n)) {
                problems.push(format!("{name} has no requirement \"{wanted}\" to change."));
            }
            for wanted in parsed.brings().filter(|n| has(n)) {
                problems.push(format!(
                    "{name} already has a requirement \"{wanted}\": change it under \"## MODIFIED Requirements\"."
                ));
            }
            if !problems.is_empty() {
                return Err(problems.join(" "));
            }
            for requirement in parsed.added.iter().chain(&parsed.modified) {
                warnings.extend(requirement_warnings(requirement));
            }
        }
    }
    Ok((delta, name))
}

pub async fn list_decisions(
    pool: &PgPool,
    actor: Actor,
    input: ListKnowledge,
) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let service = input
        .service
        .as_deref()
        .map(|sv| s.service(sv))
        .transpose()?;
    pretty(json!({
        "decisions": s.decisions.iter().filter(|d| service.is_none_or(|sv| d.service_id == Some(sv.id))).map(|d| json!({
            "id": d.id, "title": d.title, "status": d.status, "decided_on": d.decided_on,
            "service": d.service_id.and_then(|id| s.services.iter().find(|sv| sv.id == id)).map(|sv| &sv.name),
            "project": d.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
        })).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DecisionRef {
    /// Decision id or title.
    pub decision: String,
}

pub async fn get_decision(pool: &PgPool, actor: Actor, input: DecisionRef) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let d = s.decision(&input.decision)?;
    pretty(json!({
        "id": d.id,
        "title": d.title,
        "status": d.status,
        "decided_on": d.decided_on,
        "service": d.service_id.and_then(|id| s.services.iter().find(|sv| sv.id == id)).map(|sv| &sv.name),
        "project": d.project_id.and_then(|id| s.projects.iter().find(|p| p.id == id)).map(|p| &p.name),
        "superseded_by": d.superseded_by_id.and_then(|id| s.decisions.iter().find(|x| x.id == id)).map(|x| &x.title),
        "context": d.context,
        "decision": d.decision,
        "consequences": d.consequences,
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveDecision {
    /// Decision id or title, to change an existing decision. Leave out to record one.
    pub decision_ref: Option<String>,
    pub title: Option<String>,
    /// Markdown: the situation that called for a decision.
    pub context: Option<String>,
    /// Markdown: what was decided.
    pub decision: Option<String>,
    /// Markdown: what follows from it.
    pub consequences: Option<String>,
    /// proposed, accepted, superseded or overridden.
    pub status: Option<String>,
    /// Service id or name; "none" to remove.
    pub service: Option<String>,
    /// Project id or name; "none" to remove.
    pub project: Option<String>,
    /// Id or title of the decision that replaces this one; "none" to clear.
    pub superseded_by: Option<String>,
    /// YYYY-MM-DD.
    pub decided_on: Option<String>,
}

pub async fn save_decision(pool: &PgPool, actor: Actor, input: SaveDecision) -> ToolResult<String> {
    let s = Snapshot::load(pool, actor).await?;
    let mut data = Map::new();
    for (field, value) in [
        ("title", &input.title),
        ("context", &input.context),
        ("decision", &input.decision),
        ("consequences", &input.consequences),
    ] {
        if let Some(value) = value {
            data.insert(field.into(), json!(value));
        }
    }
    if let Some(status) = &input.status {
        data.insert("status".into(), json!(status.trim().to_lowercase()));
    }
    if let Some(service) = &input.service {
        data.insert(
            "service_id".into(),
            json!(if is_none(service) {
                None
            } else {
                Some(s.service(service)?.id)
            }),
        );
    }
    if let Some(project) = &input.project {
        data.insert(
            "project_id".into(),
            json!(if is_none(project) {
                None
            } else {
                Some(s.project(project)?.id)
            }),
        );
    }
    if let Some(later) = &input.superseded_by {
        let replaced = !is_none(later);
        data.insert(
            "superseded_by_id".into(),
            json!(if replaced {
                Some(s.decision(later)?.id)
            } else {
                None
            }),
        );
        // As in the app: naming what replaces it makes it superseded, unless
        // the call says another status.
        if replaced && input.status.is_none() {
            data.insert("status".into(), json!("superseded"));
        }
    }
    if let Some(date) = &input.decided_on {
        data.insert(
            "decided_on".into(),
            if is_none(date) {
                Value::Null
            } else {
                json!(date)
            },
        );
    }
    save(
        pool,
        actor,
        &s,
        ModelKind::Decision,
        input.decision_ref.as_deref(),
        |s, d| s.decision(d).map(|d| d.id),
        data,
        "title",
    )
    .await
}
