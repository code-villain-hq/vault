//! Drift checks agents run outside Forge and report here (APP-10, spec
//! "Drift checks from agents"). A check is a run with trigger `agent`: the
//! agent starts it with a key of its own, reports verdicts as it goes, and
//! finishes it. Forge keeps what the agent reports apart from what it sees
//! itself, the issues the agent files and comments on meanwhile.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use forge_domain::{
    Actor, CheckVerdict, ModelKind, Operation, Spec, Transaction, UsagePart, VerdictTally,
    requirements,
};
use forge_sync::{Origin, apply_in};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::snapshot::{Snapshot, ToolResult};

/// A check without a start or report for this long has lost contact.
pub const LOST_CONTACT_MINUTES: i64 = 30;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannedRequirement {
    /// The spec: id or title.
    pub spec: String,
    /// The requirement's name, as in `### Requirement: <name>`.
    pub requirement: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartDriftCheck {
    /// Your own key for the check, unique in the workspace, e.g. its commit range. Starting again with
    /// the same key answers with that check, so a retry never makes a second one.
    pub key: String,
    /// The commit the range starts after, as in git's `from..to`. Leave out for a full audit at one commit.
    pub commit_from: Option<String>,
    /// The last commit checked.
    pub commit_to: String,
    /// The services the check covers: names or ids.
    pub services: Vec<String>,
    /// The checker's name and version, e.g. "vault-drift-check v2".
    pub checker: String,
    /// true for a full audit of every requirement, false for an incremental check of what changed.
    pub full_audit: bool,
    /// The requirements you plan to judge. A check that ends with some of them unjudged shows as incomplete.
    pub plan: Option<Vec<PlannedRequirement>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportedVerdict {
    /// The spec: id or title.
    pub spec: String,
    /// The requirement's name, as in `### Requirement: <name>`.
    pub requirement: String,
    /// met, not_met, unclear, skipped (not judged on purpose), or covered (open work covers it).
    pub verdict: String,
    /// Markdown: the evidence (files and lines), or why it's unclear, skipped or covered.
    pub why: String,
    /// For covered: the open issue that covers it, e.g. FOR-12.
    pub issue: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportDriftCheck {
    /// The check's key, as started.
    pub key: String,
    /// Verdicts so far. A requirement reported again replaces its earlier verdict.
    pub verdicts: Vec<ReportedVerdict>,
    /// More requirements you plan to judge.
    pub plan: Option<Vec<PlannedRequirement>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportedUsage {
    /// What the part was, e.g. "Orchestrator" or "Worker: Accounts".
    pub name: String,
    pub calls: Option<i64>,
    /// Input tokens read without the cache.
    pub input: Option<i64>,
    pub cache_writes: Option<i64>,
    pub cache_reads: Option<i64>,
    pub output: Option<i64>,
    /// true when the output is an estimate, not a measurement.
    pub output_estimated: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinishDriftCheck {
    /// The check's key, as started.
    pub key: String,
    /// finished, or failed.
    pub outcome: String,
    /// Markdown: what the check found and did.
    pub summary: String,
    /// For failed: why.
    pub error: Option<String>,
    /// What the check used, one part each for the orchestrator and every worker. Leave out what you
    /// don't know: it shows as unavailable, never as 0.
    pub usage: Option<Vec<ReportedUsage>>,
}

/// The check behind a key, as far as the tools need it.
#[derive(sqlx::FromRow)]
struct Check {
    id: Uuid,
    created_by: Uuid,
    /// running, succeeded or failed.
    status: String,
    started_at: DateTime<Utc>,
    commit_to: Option<String>,
}

async fn find(db: &mut PgConnection, actor: Actor, key: &str) -> ToolResult<Option<Check>> {
    sqlx::query_as(
        "SELECT id, created_by, status, coalesce(started_at, created_at) AS started_at, commit_to \
         FROM ai_runs WHERE workspace_id = $1 AND run_key = $2",
    )
    .bind(actor.workspace_id)
    .bind(key.trim())
    .fetch_optional(&mut *db)
    .await
    .map_err(|e| e.to_string())
}

/// The agent's own check behind a key, still running.
async fn own_running(db: &mut PgConnection, actor: Actor, key: &str) -> ToolResult<Check> {
    let check = find(db, actor, key).await?.ok_or_else(|| {
        format!(
            "there's no check with the key {:?}; start it with start_drift_check",
            key.trim()
        )
    })?;
    if check.created_by != actor.user_id {
        return Err("only the agent that started this check can report into it".into());
    }
    Ok(check)
}

async fn write(db: &mut PgConnection, actor: Actor, operations: Vec<Operation>) -> ToolResult<()> {
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    apply_in(db, actor, &transaction, Origin::Server)
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

/// The requirement of `spec` that `name` names, with its text.
fn requirement_of(spec: &Spec, name: &str) -> ToolResult<(String, String)> {
    let all = requirements(&spec.body);
    let name = name.trim();
    let name = name.strip_prefix("Requirement:").unwrap_or(name).trim();
    if let Some(found) = all.iter().find(|r| r.name == name) {
        return Ok((found.name.clone(), found.text.clone()));
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

/// A requirement as reported: which spec, at which version, and its text.
struct Resolved<'a> {
    spec: &'a Spec,
    name: String,
    text: String,
}

fn resolve<'a>(s: &'a Snapshot, spec: &str, requirement: &str) -> ToolResult<Resolved<'a>> {
    let spec = s.spec(spec)?;
    let (name, text) = requirement_of(spec, requirement)?;
    Ok(Resolved { spec, name, text })
}

/// Adds planned requirements that aren't in the check yet.
async fn plan(
    db: &mut PgConnection,
    actor: Actor,
    run_id: Uuid,
    planned: &[Resolved<'_>],
) -> ToolResult<()> {
    for r in planned {
        sqlx::query(
            "INSERT INTO drift_check_results \
             (id, workspace_id, run_id, spec_id, spec_title, spec_updated_at, requirement, requirement_text) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (run_id, spec_id, requirement) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(actor.workspace_id)
        .bind(run_id)
        .bind(r.spec.id)
        .bind(&r.spec.title)
        .bind(r.spec.updated_at)
        .bind(&r.name)
        .bind(&r.text)
        .execute(&mut *db)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The check's verdicts so far, counted from what it recorded.
async fn tally(db: &mut PgConnection, run_id: Uuid) -> ToolResult<VerdictTally> {
    let counts: Vec<(Option<String>, i64)> = sqlx::query_as(
        "SELECT verdict, count(*) FROM drift_check_results WHERE run_id = $1 GROUP BY verdict",
    )
    .bind(run_id)
    .fetch_all(&mut *db)
    .await
    .map_err(|e| e.to_string())?;
    let mut tally = VerdictTally::default();
    for (verdict, count) in counts {
        let count = count as i32;
        match verdict.as_deref() {
            Some("met") => tally.met = count,
            Some("not_met") => tally.not_met = count,
            Some("unclear") => tally.unclear = count,
            Some("skipped") => tally.skipped = count,
            Some("covered") => tally.covered = count,
            _ => tally.unjudged = count,
        }
    }
    Ok(tally)
}

fn resolve_plan<'a>(
    s: &'a Snapshot,
    planned: &[PlannedRequirement],
    problems: &mut Vec<String>,
) -> Vec<Resolved<'a>> {
    planned
        .iter()
        .filter_map(|p| {
            resolve(s, &p.spec, &p.requirement)
                .map_err(|e| problems.push(e))
                .ok()
        })
        .collect()
}

pub async fn start_drift_check(
    pool: &PgPool,
    actor: Actor,
    input: StartDriftCheck,
) -> ToolResult<String> {
    let key = input.key.trim();
    if key.is_empty() {
        return Err("give the check a key".into());
    }
    if input.services.is_empty() {
        return Err("name the services the check covers".into());
    }
    let s = Snapshot::load(pool, actor).await?;
    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    // Two starts with the same key wait for each other here, so one makes it.
    forge_sync::lock_workspace(&mut db, actor.workspace_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(check) = find(&mut db, actor, key).await? {
        if check.created_by != actor.user_id {
            return Err(format!(
                "another agent's check already has the key {key:?}; choose another"
            ));
        }
        return Ok(format!(
            "The check {key:?} had already started; nothing changed. It's {}.",
            match check.status.as_str() {
                "succeeded" => "finished",
                "failed" => "failed",
                _ => "running: report into it, then finish it",
            }
        ));
    }

    let mut problems = Vec::new();
    let mut service_ids = Vec::new();
    for name in &input.services {
        match s.service(name) {
            Ok(service) if !service_ids.contains(&service.id) => service_ids.push(service.id),
            Ok(_) => {}
            Err(_) => problems.push(format!("no service {:?}", name.trim())),
        }
    }
    if !problems.is_empty() {
        let names: Vec<&str> = s.services.iter().map(|s| s.name.as_str()).collect();
        return Err(format!(
            "nothing started: {}. The services are {}.",
            problems.join("; "),
            names.join(", ")
        ));
    }
    let planned = resolve_plan(&s, input.plan.as_deref().unwrap_or_default(), &mut problems);
    if !problems.is_empty() {
        return Err(format!("nothing started: {}.", problems.join("; ")));
    }

    let id = Uuid::now_v7();
    let now = Utc::now();
    let data = object(json!({
        "kind": "drift",
        "trigger": "agent",
        "status": "running",
        "service_ids": service_ids,
        "commit_from": input.commit_from.as_deref().map(str::trim).filter(|c| !c.is_empty()),
        "commit_to": input.commit_to.trim(),
        "checker": input.checker.trim(),
        "full_audit": input.full_audit,
        "run_key": key,
        "started_at": now,
        "last_report_at": now,
        "tally": VerdictTally::default(),
    }));
    write(
        &mut db,
        actor,
        vec![Operation::Create {
            model: ModelKind::AiRun,
            id,
            data,
        }],
    )
    .await?;
    if !planned.is_empty() {
        plan(&mut db, actor, id, &planned).await?;
        let tally = tally(&mut db, id).await?;
        write(
            &mut db,
            actor,
            vec![Operation::Update {
                model: ModelKind::AiRun,
                id,
                data: object(json!({ "tally": tally })),
            }],
        )
        .await?;
    }
    db.commit().await.map_err(|e| e.to_string())?;
    Ok(format!(
        "Started the check {key:?}{}. Report verdicts with report_drift_check as you go, and \
         finish it with finish_drift_check. Without a report for {LOST_CONTACT_MINUTES} minutes it \
         shows as having lost contact.",
        if planned.is_empty() {
            String::new()
        } else {
            format!(", planning {} requirements", planned.len())
        }
    ))
}

fn verdict_of(name: &str) -> ToolResult<CheckVerdict> {
    match name.trim() {
        "met" => Ok(CheckVerdict::Met),
        "not_met" => Ok(CheckVerdict::NotMet),
        "unclear" => Ok(CheckVerdict::Unclear),
        "skipped" => Ok(CheckVerdict::Skipped),
        "covered" => Ok(CheckVerdict::Covered),
        other => Err(format!(
            "{other:?} isn't a verdict; use met, not_met, unclear, skipped or covered"
        )),
    }
}

/// A verdict as reported, resolved against the specs.
struct Judged<'a> {
    resolved: Resolved<'a>,
    verdict: CheckVerdict,
    why: &'a str,
    issue: Option<Uuid>,
}

pub async fn report_drift_check(
    pool: &PgPool,
    actor: Actor,
    input: ReportDriftCheck,
) -> ToolResult<String> {
    if input.verdicts.is_empty() && input.plan.as_ref().is_none_or(Vec::is_empty) {
        return Err("give at least one verdict".into());
    }
    let s = Snapshot::load(pool, actor).await?;
    let mut problems = Vec::new();
    // The last verdict on a requirement counts.
    let mut verdicts: BTreeMap<(Uuid, String), Judged> = BTreeMap::new();
    for reported in &input.verdicts {
        let resolved = match resolve(&s, &reported.spec, &reported.requirement) {
            Ok(r) => r,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        let verdict = match verdict_of(&reported.verdict) {
            Ok(v) => v,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        let issue = match (&reported.issue, verdict) {
            (Some(identifier), _) if !identifier.trim().is_empty() => match s.issue(identifier) {
                Ok(issue) => Some(issue.id),
                Err(e) => {
                    problems.push(e);
                    continue;
                }
            },
            (_, CheckVerdict::Covered) => {
                problems.push(format!(
                    "\"{}\" is covered: name the issue that covers it",
                    resolved.name
                ));
                continue;
            }
            _ => None,
        };
        verdicts.insert(
            (resolved.spec.id, resolved.name.clone()),
            Judged {
                resolved,
                verdict,
                why: &reported.why,
                issue,
            },
        );
    }
    let planned = resolve_plan(&s, input.plan.as_deref().unwrap_or_default(), &mut problems);
    if !problems.is_empty() {
        return Err(format!(
            "nothing recorded: {}. Fix those and send all of them again.",
            problems.join("; ")
        ));
    }

    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    forge_sync::lock_workspace(&mut db, actor.workspace_id)
        .await
        .map_err(|e| e.to_string())?;
    let check = own_running(&mut db, actor, &input.key).await?;
    if check.status != "running" {
        return Err("this check has ended; it takes no more reports".into());
    }
    plan(&mut db, actor, check.id, &planned).await?;

    let mut operations = Vec::new();
    for Judged {
        resolved,
        verdict,
        why,
        issue,
    } in verdicts.values()
    {
        sqlx::query(
            "INSERT INTO drift_check_results \
             (id, workspace_id, run_id, spec_id, spec_title, spec_updated_at, requirement, \
              requirement_text, verdict, why, issue_id, reported_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now()) \
             ON CONFLICT (run_id, spec_id, requirement) DO UPDATE SET \
              spec_title = EXCLUDED.spec_title, spec_updated_at = EXCLUDED.spec_updated_at, \
              requirement_text = EXCLUDED.requirement_text, verdict = EXCLUDED.verdict, \
              why = EXCLUDED.why, issue_id = EXCLUDED.issue_id, reported_at = now()",
        )
        .bind(Uuid::now_v7())
        .bind(actor.workspace_id)
        .bind(check.id)
        .bind(resolved.spec.id)
        .bind(&resolved.spec.title)
        .bind(resolved.spec.updated_at)
        .bind(&resolved.name)
        .bind(&resolved.text)
        .bind(
            serde_json::to_value(verdict)
                .ok()
                .and_then(|v| v.as_str().map(String::from)),
        )
        .bind(why)
        .bind(issue)
        .execute(&mut *db)
        .await
        .map_err(|e| e.to_string())?;

        // Met, not met and unclear become the requirement's latest verdict,
        // as a Forge AI check's do.
        let latest = match verdict {
            CheckVerdict::Met => "met",
            CheckVerdict::NotMet => "not_met",
            CheckVerdict::Unclear => "unclear",
            CheckVerdict::Skipped | CheckVerdict::Covered => continue,
        };
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM requirement_verdicts WHERE spec_id = $1 AND requirement = $2",
        )
        .bind(resolved.spec.id)
        .bind(&resolved.name)
        .fetch_optional(&mut *db)
        .await
        .map_err(|e| e.to_string())?;
        let fields = json!({
            "verdict": latest,
            "evidence": why,
            "requirement_text": resolved.text,
            "spec_updated_at": resolved.spec.updated_at,
            "commit_sha": check.commit_to,
            "run_id": check.id,
            "checked_at": check.started_at,
        });
        operations.push(match existing {
            Some(id) => Operation::Update {
                model: ModelKind::RequirementVerdict,
                id,
                data: object(fields),
            },
            None => {
                let mut data = object(fields);
                data.insert("spec_id".into(), json!(resolved.spec.id));
                data.insert("requirement".into(), json!(resolved.name));
                Operation::Create {
                    model: ModelKind::RequirementVerdict,
                    id: Uuid::now_v7(),
                    data,
                }
            }
        });
    }
    let tally = tally(&mut db, check.id).await?;
    operations.push(Operation::Update {
        model: ModelKind::AiRun,
        id: check.id,
        data: object(json!({ "tally": tally, "last_report_at": Utc::now() })),
    });
    write(&mut db, actor, operations).await?;
    db.commit().await.map_err(|e| e.to_string())?;
    Ok(format!(
        "Recorded {} {}. So far: {}.",
        verdicts.len(),
        if verdicts.len() == 1 {
            "verdict"
        } else {
            "verdicts"
        },
        describe(&tally)
    ))
}

/// The tally in words, e.g. "52 met, 1 not met, 8 planned without a verdict".
fn describe(tally: &VerdictTally) -> String {
    let parts: Vec<String> = [
        (tally.met, "met"),
        (tally.not_met, "not met"),
        (tally.unclear, "unclear"),
        (tally.skipped, "skipped"),
        (tally.covered, "covered by open work"),
        (tally.unjudged, "planned without a verdict"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    if parts.is_empty() {
        "no verdicts".into()
    } else {
        parts.join(", ")
    }
}

/// The issues an agent filed and commented on from `from` to `to`, as
/// Forge saw them, oldest first.
pub async fn seen(
    db: &mut PgConnection,
    workspace_id: Uuid,
    agent: Uuid,
    from: DateTime<Utc>,
    to: Option<DateTime<Utc>>,
) -> Result<(Vec<Uuid>, Vec<Uuid>), sqlx::Error> {
    let filed = sqlx::query_scalar(
        "SELECT id FROM issues WHERE workspace_id = $1 AND created_by = $2 \
         AND created_at >= $3 AND ($4::timestamptz IS NULL OR created_at <= $4) ORDER BY created_at",
    )
    .bind(workspace_id)
    .bind(agent)
    .bind(from)
    .bind(to)
    .fetch_all(&mut *db)
    .await?;
    let commented = sqlx::query_scalar(
        "SELECT issue_id FROM comments WHERE workspace_id = $1 AND author_id = $2 \
         AND created_at >= $3 AND ($4::timestamptz IS NULL OR created_at <= $4) \
         GROUP BY issue_id ORDER BY min(created_at)",
    )
    .bind(workspace_id)
    .bind(agent)
    .bind(from)
    .bind(to)
    .fetch_all(&mut *db)
    .await?;
    Ok((filed, commented))
}

pub async fn finish_drift_check(
    pool: &PgPool,
    actor: Actor,
    input: FinishDriftCheck,
) -> ToolResult<String> {
    let failed = match input.outcome.trim() {
        "finished" => false,
        "failed" => true,
        other => {
            return Err(format!(
                "{other:?} isn't an outcome; use finished or failed"
            ));
        }
    };
    let error = input
        .error
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    if failed && error.is_none() {
        return Err("say why the check failed, in error".into());
    }
    let usage: Option<Vec<UsagePart>> = input.usage.map(|parts| {
        parts
            .into_iter()
            .map(|p| UsagePart {
                name: p.name,
                calls: p.calls,
                input: p.input,
                cache_writes: p.cache_writes,
                cache_reads: p.cache_reads,
                output: p.output,
                output_estimated: p.output_estimated.unwrap_or(false),
            })
            .collect()
    });

    let mut db = pool.begin().await.map_err(|e| e.to_string())?;
    forge_sync::lock_workspace(&mut db, actor.workspace_id)
        .await
        .map_err(|e| e.to_string())?;
    let check = own_running(&mut db, actor, &input.key).await?;
    if check.status != "running" {
        return Ok("This check had already ended; nothing changed.".into());
    }
    let tally = tally(&mut db, check.id).await?;
    let now = Utc::now();
    write(
        &mut db,
        actor,
        vec![Operation::Update {
            model: ModelKind::AiRun,
            id: check.id,
            data: object(json!({
                "status": if failed { "failed" } else { "succeeded" },
                "summary": input.summary,
                "error": if failed { error } else { None },
                "usage": usage,
                "tally": tally,
                "last_report_at": now,
                "finished_at": now,
            })),
        }],
    )
    .await?;

    let (filed, commented) = seen(
        &mut db,
        actor.workspace_id,
        actor.user_id,
        check.started_at,
        Some(now),
    )
    .await
    .map_err(|e| e.to_string())?;
    db.commit().await.map_err(|e| e.to_string())?;
    let s = Snapshot::load(pool, actor).await?;
    let identifiers = |ids: Vec<Uuid>| -> Vec<String> {
        ids.iter()
            .filter_map(|id| s.issues.iter().find(|i| i.id == *id))
            .map(|i| s.identifier(i))
            .collect()
    };
    let (filed, commented) = (identifiers(filed), identifiers(commented));
    let outcome = if failed {
        "Marked the check failed".to_string()
    } else if tally.unjudged > 0 {
        format!(
            "Finished the check; it shows as incomplete: {} planned requirements have no verdict",
            tally.unjudged
        )
    } else {
        "Finished the check".to_string()
    };
    let list = |ids: &[String]| {
        if ids.is_empty() {
            "nothing".to_string()
        } else {
            ids.join(", ")
        }
    };
    Ok(format!(
        "{outcome}. Verdicts: {}. Forge saw you file {} and comment on {}.",
        describe(&tally),
        list(&filed),
        list(&commented)
    ))
}
