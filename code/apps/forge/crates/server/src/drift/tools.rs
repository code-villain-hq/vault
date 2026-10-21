//! The tools Claude uses during a drift check: read the service's code and
//! history and the specs and decisions in full, propose changes for people to review, give each spec
//! requirement a verdict, and keep the service's current state up to date.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use forge_ai::{Repo, ToolHandler, ToolOutput};
use forge_domain::{Actor, ModelKind, Operation, Service, Spec, Verdict, requirements};
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{object, write};
use crate::mcp::snapshot::Snapshot;
use crate::mcp::tools::{self as mcp, DecisionRef, SpecRef};

/// At most this many suggestions per check, to keep them worth reading.
const MAX_SUGGESTIONS: i32 = 10;

pub struct DriftTools {
    pool: PgPool,
    agent: Actor,
    snapshot: Snapshot,
    service: Service,
    repo: Repo,
    run_id: Uuid,
    /// When the check started: verdicts are about the code as it was then.
    started_at: DateTime<Utc>,
    /// The latest commit touching the service's code, when it started.
    commit: Option<String>,
    /// The requirements given a verdict so far, by spec.
    judged: BTreeSet<(Uuid, String)>,
    pub suggestions: i32,
    pub summary: Option<String>,
    finished: bool,
}

impl DriftTools {
    pub fn new(
        pool: PgPool,
        agent: Actor,
        snapshot: Snapshot,
        service: Service,
        repo: Repo,
        run_id: Uuid,
        commit: Option<String>,
    ) -> Self {
        DriftTools {
            pool,
            agent,
            snapshot,
            service,
            repo,
            run_id,
            started_at: Utc::now(),
            commit,
            judged: BTreeSet::new(),
            suggestions: 0,
            summary: None,
            finished: false,
        }
    }
}

pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_files",
            "description": "List the service's tracked files, optionally only those matching a glob such as `src/**/*.rs`.",
            "input_schema": { "type": "object", "properties": {
                "glob": { "type": "string", "description": "Optional glob to narrow the list." },
            }},
        }),
        json!({
            "name": "read_file",
            "description": "Read a file's lines, numbered. Reads up to 400 lines at a time; give a range for longer files.",
            "input_schema": { "type": "object", "required": ["path"], "properties": {
                "path": { "type": "string", "description": "Path relative to the repository root, as list_files shows it." },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
            }},
        }),
        json!({
            "name": "search_code",
            "description": "Find lines matching a regular expression (case-insensitive) in the service's files.",
            "input_schema": { "type": "object", "required": ["pattern"], "properties": {
                "pattern": { "type": "string", "description": "A regular expression." },
                "glob": { "type": "string", "description": "Optional glob to limit which files are searched." },
            }},
        }),
        json!({
            "name": "git_log",
            "description": "Recent commits touching the service, or one file, newest first, with the files each changed.",
            "input_schema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Optional: only commits touching this file." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
            }},
        }),
        json!({
            "name": "git_show",
            "description": "One commit's message and its diff within the service.",
            "input_schema": { "type": "object", "required": ["commit"], "properties": {
                "commit": { "type": "string", "description": "A commit hash from git_log." },
            }},
        }),
        json!({
            "name": "read_spec",
            "description": "Read one spec in full: the service's own, or one of the project and workspace-wide ones listed.",
            "input_schema": { "type": "object", "required": ["spec"], "properties": {
                "spec": { "type": "string", "description": "The spec's id or title." },
            }},
        }),
        json!({
            "name": "read_decision",
            "description": "Read one decision in full: its context, the decision, its consequences and status.",
            "input_schema": { "type": "object", "required": ["decision"], "properties": {
                "decision": { "type": "string", "description": "The decision's id or title." },
            }},
        }),
        json!({
            "name": "propose_change",
            "description": "Suggest a fix for one finding where the tracker and the code disagree. People review it before anything changes. Explain what drifted and cite the evidence (files and lines, commits).",
            "input_schema": { "type": "object", "required": ["title", "explanation", "changes"], "properties": {
                "title": { "type": "string", "description": "One line, e.g. \"FOR-12 is done in the code but still open\"." },
                "explanation": { "type": "string", "description": "Markdown: what the tracker says, what the code does, and the evidence." },
                "issue": { "type": "string", "description": "The issue this is mainly about, e.g. FOR-12, if any." },
                "decision": { "type": "string", "description": "The id or title of the decision this is mainly about, if any." },
                "spec": { "type": "string", "description": "The id or title of the spec this is mainly about, if any." },
                "requirement": { "type": "string", "description": "The requirement of that spec this comes from, if any: its name, exactly as after `### Requirement:`." },
                "changes": {
                    "type": "array",
                    "description": "The changes to make if someone accepts. Can be empty when the finding needs a person to decide.",
                    "items": { "type": "object", "required": ["action"], "properties": {
                        "action": { "type": "string", "enum": [
                            "set_issue_status", "comment_on_issue", "update_issue_description", "create_issue",
                            "set_decision_status", "update_spec", "update_service_description",
                        ]},
                        "issue": { "type": "string", "description": "Issue identifier, for issue actions." },
                        "status": { "type": "string", "description": "For set_issue_status: a status name of the issue's team. For set_decision_status: proposed, accepted, superseded or overridden." },
                        "body": { "type": "string", "description": "Markdown: the comment, the new description, or the new spec text." },
                        "title": { "type": "string", "description": "For create_issue." },
                        "decision": { "type": "string", "description": "Decision id or title, for set_decision_status." },
                        "spec": { "type": "string", "description": "Spec id or title, for update_spec." },
                    }},
                },
            }},
        }),
        json!({
            "name": "report_verdicts",
            "description": "Record whether the code meets requirements of the service's specs (those written as `### Requirement: <name>`). Give a verdict on every requirement listed in the context; you can call this as often as you like, and a later verdict on a requirement replaces an earlier one.",
            "input_schema": { "type": "object", "required": ["verdicts"], "properties": {
                "verdicts": { "type": "array", "items": {
                    "type": "object", "required": ["spec", "requirement", "verdict", "evidence"], "properties": {
                        "spec": { "type": "string", "description": "The spec's id or title." },
                        "requirement": { "type": "string", "description": "The requirement's name, exactly as after `### Requirement:`." },
                        "verdict": { "type": "string", "enum": ["met", "not_met", "unclear"] },
                        "evidence": { "type": "string", "description": "Markdown. For met and not_met: the files and lines that show it. For unclear: why, e.g. the code is outside the service's paths." },
                    },
                }},
            }},
        }),
        json!({
            "name": "update_current_state",
            "description": "Record what the service's code does today, replacing the previous current state. Concise Markdown, at most about 400 words.",
            "input_schema": { "type": "object", "required": ["current_state"], "properties": {
                "current_state": { "type": "string" },
            }},
        }),
        json!({
            "name": "finish",
            "description": "End the check, with a short summary of what you checked and found.",
            "input_schema": { "type": "object", "required": ["summary"], "properties": {
                "summary": { "type": "string", "description": "Markdown, a few sentences." },
            }},
        }),
    ]
}

#[derive(Deserialize)]
struct ListFiles {
    glob: Option<String>,
}

#[derive(Deserialize)]
struct ReadFile {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Deserialize)]
struct SearchCode {
    pattern: String,
    glob: Option<String>,
}

#[derive(Deserialize)]
struct GitLog {
    path: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct GitShow {
    commit: String,
}

#[derive(Deserialize)]
struct Proposal {
    title: String,
    explanation: String,
    issue: Option<String>,
    decision: Option<String>,
    spec: Option<String>,
    requirement: Option<String>,
    changes: Vec<Change>,
}

#[derive(Deserialize)]
struct Change {
    action: String,
    issue: Option<String>,
    status: Option<String>,
    body: Option<String>,
    title: Option<String>,
    decision: Option<String>,
    spec: Option<String>,
}

#[derive(Deserialize)]
struct Verdicts {
    verdicts: Vec<VerdictInput>,
}

#[derive(Deserialize)]
struct VerdictInput {
    spec: String,
    requirement: String,
    verdict: Verdict,
    evidence: String,
}

#[derive(Deserialize)]
struct CurrentState {
    current_state: String,
}

#[derive(Deserialize)]
struct Finish {
    summary: String,
}

fn parse<T: serde::de::DeserializeOwned>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("invalid input: {e}"))
}

impl ToolHandler for DriftTools {
    fn finished(&self) -> bool {
        self.finished
    }

    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let result = match name {
                "list_files" => parse::<ListFiles>(input)
                    .and_then(|i| self.repo.list(i.glob.as_deref()).map_err(|e| e.0)),
                "read_file" => match parse::<ReadFile>(input) {
                    Ok(i) => self
                        .repo
                        .read(&i.path, i.start_line, i.end_line)
                        .await
                        .map_err(|e| e.0),
                    Err(e) => Err(e),
                },
                "search_code" => match parse::<SearchCode>(input) {
                    Ok(i) => self
                        .repo
                        .search(&i.pattern, i.glob.as_deref())
                        .await
                        .map_err(|e| e.0),
                    Err(e) => Err(e),
                },
                "git_log" => match parse::<GitLog>(input) {
                    Ok(i) => self
                        .repo
                        .log(i.path.as_deref(), i.limit.unwrap_or(20))
                        .await
                        .map_err(|e| e.0),
                    Err(e) => Err(e),
                },
                "git_show" => match parse::<GitShow>(input) {
                    Ok(i) => self.repo.show(&i.commit).await.map_err(|e| e.0),
                    Err(e) => Err(e),
                },
                "read_spec" => match parse::<SpecRef>(input) {
                    Ok(i) => mcp::get_spec(&self.pool, self.agent, i).await,
                    Err(e) => Err(e),
                },
                "read_decision" => match parse::<DecisionRef>(input) {
                    Ok(i) => mcp::get_decision(&self.pool, self.agent, i).await,
                    Err(e) => Err(e),
                },
                "propose_change" => match parse::<Proposal>(input) {
                    Ok(p) => self.propose(p).await,
                    Err(e) => Err(e),
                },
                "report_verdicts" => match parse::<Verdicts>(input) {
                    Ok(i) => self.report_verdicts(i.verdicts).await,
                    Err(e) => Err(e),
                },
                "update_current_state" => match parse::<CurrentState>(input) {
                    Ok(i) => self.update_current_state(i.current_state).await,
                    Err(e) => Err(e),
                },
                "finish" => match parse::<Finish>(input) {
                    Ok(i) => {
                        self.summary = Some(i.summary);
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

impl DriftTools {
    async fn propose(&mut self, proposal: Proposal) -> Result<String, String> {
        if self.suggestions >= MAX_SUGGESTIONS {
            return Err(format!(
                "that's {MAX_SUGGESTIONS} suggestions already; keep the most important ones and finish"
            ));
        }
        let s = &self.snapshot;
        let issue = proposal.issue.as_deref().map(|i| s.issue(i)).transpose()?;
        let decision = proposal
            .decision
            .as_deref()
            .map(|d| s.decision(d))
            .transpose()?;
        let spec = proposal
            .spec
            .as_deref()
            .filter(|spec| !spec.trim().is_empty())
            .map(|spec| s.spec(spec))
            .transpose()?;
        let requirement = match proposal
            .requirement
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            Some(name) => {
                let spec = spec.ok_or("give the spec the requirement belongs to")?;
                Some(requirement_of(spec, name)?)
            }
            None => None,
        };

        let mut changes = Vec::new();
        let mut operations = Vec::new();
        for change in &proposal.changes {
            let (text, operation) = self.change(change)?;
            changes.push(text);
            operations.push(operation);
        }

        let id = Uuid::now_v7();
        let data = object(json!({
            "title": proposal.title,
            "body": proposal.explanation,
            "changes": changes,
            "operations": operations,
            "service_id": self.service.id,
            "issue_id": issue.map(|i| i.id),
            "decision_id": decision.map(|d| d.id),
            "spec_id": spec.map(|sp| sp.id),
            "requirement": requirement,
            "run_id": self.run_id,
        }));
        let mut db = self.pool.begin().await.map_err(|e| e.to_string())?;
        write(
            &mut db,
            self.agent,
            vec![Operation::Create {
                model: ModelKind::Suggestion,
                id,
                data,
            }],
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        db.commit().await.map_err(|e| e.to_string())?;
        self.suggestions += 1;
        Ok(format!(
            "Suggested ({} change{}). People will review it.",
            changes.len(),
            if changes.len() == 1 { "" } else { "s" }
        ))
    }

    /// A proposed change in words, and as the operation to apply on accept.
    fn change(&self, change: &Change) -> Result<(String, Value), String> {
        let s = &self.snapshot;
        let need = |value: &Option<String>, what: &str| {
            value
                .clone()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| format!("{} needs `{what}`", change.action))
        };
        let operation = |op: Operation| serde_json::to_value(op).map_err(|e| e.to_string());
        match change.action.as_str() {
            "set_issue_status" => {
                let issue = s.issue(&need(&change.issue, "issue")?)?;
                let team = s.team_of(issue).ok_or("the issue has no team")?;
                let state = s.state(team, &need(&change.status, "status")?)?;
                Ok((
                    format!("Move {} to {}", s.identifier(issue), state.name),
                    operation(Operation::Update {
                        model: ModelKind::Issue,
                        id: issue.id,
                        data: object(json!({ "state_id": state.id })),
                    })?,
                ))
            }
            "comment_on_issue" => {
                let issue = s.issue(&need(&change.issue, "issue")?)?;
                let body = need(&change.body, "body")?;
                Ok((
                    format!("Comment on {}: {}", s.identifier(issue), first_line(&body)),
                    operation(Operation::Create {
                        model: ModelKind::Comment,
                        id: Uuid::now_v7(),
                        data: object(json!({ "issue_id": issue.id, "body": body })),
                    })?,
                ))
            }
            "update_issue_description" => {
                let issue = s.issue(&need(&change.issue, "issue")?)?;
                Ok((
                    format!("Rewrite the description of {}", s.identifier(issue)),
                    operation(Operation::Update {
                        model: ModelKind::Issue,
                        id: issue.id,
                        data: object(json!({ "description": need(&change.body, "body")? })),
                    })?,
                ))
            }
            "create_issue" => {
                let title = need(&change.title, "title")?;
                let team = self
                    .service
                    .team_id
                    .and_then(|id| s.teams.iter().find(|t| t.id == id))
                    .or_else(|| s.teams.first())
                    .ok_or("the workspace has no team")?;
                Ok((
                    format!("Create an issue in {}: {title}", team.key),
                    operation(Operation::Create {
                        model: ModelKind::Issue,
                        id: Uuid::now_v7(),
                        data: object(json!({
                            "team_id": team.id,
                            "title": title,
                            "description": change.body.clone().unwrap_or_default(),
                            "service_id": self.service.id,
                        })),
                    })?,
                ))
            }
            "set_decision_status" => {
                let decision = s.decision(&need(&change.decision, "decision")?)?;
                let status = need(&change.status, "status")?.trim().to_lowercase();
                if !["proposed", "accepted", "superseded", "overridden"].contains(&status.as_str())
                {
                    return Err(
                        "decision status must be proposed, accepted, superseded or overridden"
                            .into(),
                    );
                }
                Ok((
                    format!("Mark the decision \"{}\" {status}", decision.title),
                    operation(Operation::Update {
                        model: ModelKind::Decision,
                        id: decision.id,
                        data: object(json!({ "status": status })),
                    })?,
                ))
            }
            "update_spec" => {
                let spec = s.spec(&need(&change.spec, "spec")?)?;
                Ok((
                    format!("Rewrite the spec \"{}\"", spec.title),
                    operation(Operation::Update {
                        model: ModelKind::Spec,
                        id: spec.id,
                        data: object(json!({ "body": need(&change.body, "body")? })),
                    })?,
                ))
            }
            "update_service_description" => Ok((
                format!("Rewrite what {} is for", self.service.name),
                operation(Operation::Update {
                    model: ModelKind::Service,
                    id: self.service.id,
                    data: object(json!({ "description": need(&change.body, "body")? })),
                })?,
            )),
            other => Err(format!("unknown action {other:?}")),
        }
    }

    async fn report_verdicts(&mut self, verdicts: Vec<VerdictInput>) -> Result<String, String> {
        if verdicts.is_empty() {
            return Err("give at least one verdict".into());
        }
        // The last verdict on a requirement counts.
        let mut checked: BTreeMap<(Uuid, String), (&Spec, VerdictInput)> = BTreeMap::new();
        let mut problems = Vec::new();
        for verdict in verdicts {
            let spec = match self.snapshot.spec(&verdict.spec) {
                Ok(spec) if spec.service_ids.contains(&self.service.id) => spec,
                Ok(spec) => {
                    problems.push(format!(
                        "\"{}\" isn't one of this service's specs",
                        spec.title
                    ));
                    continue;
                }
                Err(e) => {
                    problems.push(e);
                    continue;
                }
            };
            match requirement_of(spec, &verdict.requirement) {
                Ok(name) => {
                    checked.insert((spec.id, name), (spec, verdict));
                }
                Err(e) => problems.push(e),
            }
        }
        if !problems.is_empty() {
            return Err(format!(
                "nothing recorded: {}. Fix those and send all of them again.",
                problems.join("; ")
            ));
        }

        let mut db = self.pool.begin().await.map_err(|e| e.to_string())?;
        let mut operations = Vec::new();
        for ((spec_id, name), (spec, verdict)) in &checked {
            let text = requirements(&spec.body)
                .into_iter()
                .find(|r| &r.name == name)
                .map(|r| r.text)
                .unwrap_or_default();
            let existing: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM requirement_verdicts WHERE spec_id = $1 AND requirement = $2",
            )
            .bind(spec_id)
            .bind(name)
            .fetch_optional(&mut *db)
            .await
            .map_err(|e| e.to_string())?;
            let fields = json!({
                "verdict": verdict.verdict,
                "evidence": verdict.evidence,
                "requirement_text": text,
                "spec_updated_at": spec.updated_at,
                "commit_sha": self.commit,
                "run_id": self.run_id,
                "checked_at": self.started_at,
            });
            operations.push(match existing {
                Some(id) => Operation::Update {
                    model: ModelKind::RequirementVerdict,
                    id,
                    data: object(fields),
                },
                None => {
                    let mut data = object(fields);
                    data.insert("spec_id".into(), json!(spec_id));
                    data.insert("requirement".into(), json!(name));
                    Operation::Create {
                        model: ModelKind::RequirementVerdict,
                        id: Uuid::now_v7(),
                        data,
                    }
                }
            });
        }
        write(&mut db, self.agent, operations)
            .await
            .map_err(|e| format!("{e:?}"))?;
        db.commit().await.map_err(|e| e.to_string())?;
        self.judged.extend(checked.into_keys());

        let missing: Vec<String> = self
            .snapshot
            .specs
            .iter()
            .filter(|spec| spec.service_ids.contains(&self.service.id))
            .flat_map(|spec| {
                requirements(&spec.body)
                    .into_iter()
                    .filter(|r| !self.judged.contains(&(spec.id, r.name.clone())))
                    .map(|r| format!("\"{}\" ({})", r.name, spec.title))
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok(if missing.is_empty() {
            "Recorded. Every requirement has a verdict now.".into()
        } else {
            format!("Recorded. Still without a verdict: {}.", missing.join(", "))
        })
    }

    async fn update_current_state(&mut self, current_state: String) -> Result<String, String> {
        let data = object(
            json!({ "current_state": current_state, "current_state_updated_at": chrono::Utc::now() }),
        );
        let mut db = self.pool.begin().await.map_err(|e| e.to_string())?;
        write(
            &mut db,
            self.agent,
            vec![Operation::Update {
                model: ModelKind::Service,
                id: self.service.id,
                data,
            }],
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        db.commit().await.map_err(|e| e.to_string())?;
        Ok("Current state updated.".into())
    }
}

/// The name of one of the spec's requirements, as written in the spec.
fn requirement_of(spec: &Spec, name: &str) -> Result<String, String> {
    let all = requirements(&spec.body);
    let name = name.trim();
    let name = name.strip_prefix("Requirement:").unwrap_or(name).trim();
    if let Some(found) = all.iter().find(|r| r.name == name) {
        return Ok(found.name.clone());
    }
    if all.is_empty() {
        return Err(format!(
            "the spec \"{}\" has no `### Requirement:` sections",
            spec.title
        ));
    }
    let names: Vec<String> = all.iter().map(|r| format!("\"{}\"", r.name)).collect();
    Err(format!(
        "the spec \"{}\" has no requirement \"{name}\"; its requirements are {}",
        spec.title,
        names.join(", ")
    ))
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(80).collect::<String>())
    } else {
        line.to_string()
    }
}
