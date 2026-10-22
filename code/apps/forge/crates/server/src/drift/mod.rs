//! Drift checks: Forge's AI reads a service's code and git history, compares
//! them with the service's issues, specs, decisions and recorded current
//! state, and suggests fixes for people to accept or dismiss.
//!
//! Checks run as `ai_runs`, queued by people (manual) or on a schedule, and
//! executed one at a time by a background worker, which also triages new
//! issues (see `triage.rs`).

mod check;
pub mod suggestions;
mod tools;
pub mod triage;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use chrono::{DateTime, Utc};
use forge_ai::ModelProvider;
use forge_domain::{
    Actor, AiRunStatus, CheckResult, DriftCheckDetails, ModelKind, Operation, Transaction,
};
use forge_sync::{Origin, apply_in};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::Member;
use crate::config::{Error as SettingsError, Settings};
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct DriftSettings {
    /// Services may point at repositories on the server's disk only below
    /// this directory. `None` turns local repositories off.
    pub repos_root: Option<PathBuf>,
    /// How often each service is checked on its own. `None`: only on request.
    pub check_every: Option<chrono::Duration>,
    pub max_turns: usize,
    /// Where the GitHub connection keeps its clones, when it's set up.
    pub github_clones: Option<PathBuf>,
    /// Whether issues arriving in triage get triaged by Forge's AI on their own.
    pub triage_new_issues: bool,
}

impl Default for DriftSettings {
    fn default() -> Self {
        DriftSettings {
            repos_root: None,
            check_every: None,
            max_turns: 40,
            github_clones: None,
            triage_new_issues: false,
        }
    }
}

impl DriftSettings {
    /// `local_repos_root`, `ai.check_hours` (default 24; 0 turns scheduled
    /// checks off), `ai.max_turns` and `ai.triage` (false turns triaging new
    /// issues off).
    pub fn from_settings(settings: &Settings) -> Result<Self, SettingsError> {
        let repos_root = settings
            .get("local_repos_root")
            .and_then(|p| std::fs::canonicalize(p).ok());
        let number = |key: &str, default: i64| match settings.get(key) {
            Some(value) => value
                .parse::<i64>()
                .ok()
                .filter(|n| *n >= 0)
                .ok_or_else(|| settings.invalid(key, "is a whole number")),
            None => Ok(default),
        };
        let hours = number("ai.check_hours", 24)?;
        let max_turns = number("ai.max_turns", 40)?;
        Ok(DriftSettings {
            repos_root,
            check_every: (hours > 0).then(|| chrono::Duration::hours(hours)),
            max_turns: max_turns as usize,
            github_clones: None,
            triage_new_issues: settings.flag("ai.triage", true)?,
        })
    }

    /// Where a service's repository is on disk.
    pub fn resolve_repository(&self, repository: &str) -> Result<PathBuf, String> {
        let repository = repository.trim();
        if let Some(name) = repository.strip_prefix("github:") {
            let clones = self
                .github_clones
                .as_ref()
                .ok_or("this server isn't connected to GitHub")?;
            if !crate::github::app::valid_full_name(name) {
                return Err(format!(
                    "{repository} isn't a GitHub repository name like github:owner/name"
                ));
            }
            let path = clones.join("repos").join(name);
            if !path.join(".git").exists() {
                return Err(format!(
                    "the server hasn't cloned {name} yet; it does when GitHub is connected and on every push"
                ));
            }
            return Ok(path);
        }
        let root = self.repos_root.as_ref().ok_or(
            "this server doesn't allow repositories on its disk; set FORGE_LOCAL_REPOS_ROOT to allow those under it",
        )?;
        // Relative paths are under the root, so the same service works on a
        // laptop and in Docker, where the root is mounted somewhere else.
        let path = std::fs::canonicalize(root.join(repository))
            .map_err(|_| format!("{repository} doesn't exist on the server"))?;
        if !path.starts_with(root) {
            return Err(format!(
                "{repository} is outside {}, where repositories may live",
                root.display()
            ));
        }
        if !path.join(".git").exists() {
            return Err(format!("{repository} isn't a git repository"));
        }
        Ok(path)
    }
}

/// The workspace's own AI agent, "Forge AI", created on first use. Drift
/// checks act as it, so everything they do is attributed to it.
pub async fn ai_agent(db: &mut PgConnection, workspace_id: Uuid) -> Result<Actor, AppError> {
    crate::agents::forge_agent(db, workspace_id, "forge-ai", "Forge AI").await
}

#[derive(Serialize)]
pub struct QueuedRun {
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct AiStatus {
    /// Whether the server has a model, so Forge AI can check and triage.
    pub configured: bool,
}

/// `GET /api/w/{workspace}/ai`: whether Forge AI can run here. Without it,
/// the app doesn't offer to ask it; agents can still suggest triage.
pub async fn status(State(state): State<AppState>, _member: Member) -> Json<AiStatus> {
    Json(AiStatus {
        configured: state.ai.is_some(),
    })
}

/// `POST /api/w/{workspace}/services/{service}/check`: queues a drift check.
pub async fn request_check(
    State(state): State<AppState>,
    member: Member,
    UrlPath((_, service_id)): UrlPath<(Uuid, Uuid)>,
) -> Result<Json<QueuedRun>, AppError> {
    member.require_not_guest()?;
    let provider = state.ai.as_ref().ok_or_else(|| {
        AppError::invalid("drift checks need a model: set ANTHROPIC_API_KEY on the server")
    })?;
    let repository: Option<Option<String>> =
        sqlx::query_scalar("SELECT repository FROM services WHERE id = $1 AND workspace_id = $2")
            .bind(service_id)
            .bind(member.actor.workspace_id)
            .fetch_optional(&state.pool)
            .await?;
    let repository = repository
        .ok_or_else(|| AppError::not_found("service not found"))?
        .ok_or_else(|| AppError::invalid("give the service a repository first"))?;
    state
        .drift
        .resolve_repository(&repository)
        .map_err(AppError::invalid)?;

    let mut db = state.pool.begin().await?;
    let id = queue(
        &mut db,
        member.actor,
        service_id,
        "manual",
        provider.model(),
    )
    .await?;
    db.commit().await?;
    state.drift_wake.notify_one();
    Ok(Json(QueuedRun { id }))
}

/// `GET /api/w/{workspace}/drift-checks/{run}`: what a drift check found,
/// and the issues Forge saw its agent file and comment on while it ran.
pub async fn check_details(
    State(state): State<AppState>,
    member: Member,
    UrlPath((_, run_id)): UrlPath<(Uuid, Uuid)>,
) -> Result<Json<DriftCheckDetails>, AppError> {
    member.require_not_guest()?;
    let workspace_id = member.actor.workspace_id;
    #[derive(sqlx::FromRow)]
    struct Run {
        created_by: Uuid,
        trigger: String,
        started_at: DateTime<Utc>,
        finished_at: Option<DateTime<Utc>>,
    }
    let run: Run = sqlx::query_as(
        "SELECT created_by, trigger, coalesce(started_at, created_at) AS started_at, finished_at \
         FROM ai_runs WHERE id = $1 AND workspace_id = $2 AND kind = 'drift'",
    )
    .bind(run_id)
    .bind(workspace_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("drift check not found"))?;
    // An agent reports per check; a Forge AI check's verdicts are the
    // requirements' latest ones it still holds.
    let results: Vec<Value> = if run.trigger == "agent" {
        sqlx::query_scalar(
            "SELECT to_jsonb(r) - 'id' - 'workspace_id' - 'run_id' FROM drift_check_results r \
             WHERE run_id = $1 ORDER BY spec_title, requirement",
        )
    } else {
        sqlx::query_scalar(
            "SELECT jsonb_build_object('spec_id', v.spec_id, 'spec_title', s.title, \
             'spec_updated_at', v.spec_updated_at, 'requirement', v.requirement, \
             'requirement_text', v.requirement_text, 'verdict', v.verdict, 'why', v.evidence, \
             'issue_id', NULL, 'reported_at', v.checked_at) \
             FROM requirement_verdicts v JOIN specs s ON s.id = v.spec_id \
             WHERE v.run_id = $1 ORDER BY s.title, v.requirement",
        )
    }
    .bind(run_id)
    .fetch_all(&state.pool)
    .await?;
    let results = results
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<CheckResult>, _>>()
        .map_err(AppError::internal)?;
    let mut db = state.pool.acquire().await?;
    let (filed, commented) = crate::mcp::drift_checks::seen(
        &mut db,
        workspace_id,
        run.created_by,
        run.started_at,
        run.finished_at,
    )
    .await?;
    Ok(Json(DriftCheckDetails {
        results,
        filed,
        commented,
    }))
}

/// Queues a run unless the service already has one waiting or running.
async fn queue(
    db: &mut PgConnection,
    actor: Actor,
    service_id: Uuid,
    trigger: &str,
    model: &str,
) -> Result<Uuid, AppError> {
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ai_runs WHERE service_id = $1 AND status IN ('queued', 'running'))",
    )
    .bind(service_id)
    .fetch_one(&mut *db)
    .await?;
    if busy {
        return Err(AppError::conflict(
            "a check of this service is already waiting or running",
        ));
    }
    let id = Uuid::now_v7();
    let data = object(json!({ "service_id": service_id, "trigger": trigger, "model": model }));
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

/// Writes through the sync engine as the server.
async fn write(
    db: &mut PgConnection,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<(), AppError> {
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    apply_in(db, actor, &transaction, Origin::Server).await?;
    Ok(())
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Runs queued checks one at a time, and queues scheduled ones, for the
/// life of the server. Without a model it only clears up runs that can't go
/// on, so nothing waits for them forever.
pub async fn worker(state: AppState) {
    if let Err(e) = recover_interrupted(&state.pool, state.ai.is_some()).await {
        tracing::error!(error = ?e, "couldn't mark interrupted runs failed");
    }
    let Some(provider) = state.ai.clone() else {
        return;
    };
    let mut last_schedule = std::time::Instant::now() - Duration::from_secs(3600);
    loop {
        if let Some(every) = state.drift.check_every
            && last_schedule.elapsed() > Duration::from_secs(600)
        {
            last_schedule = std::time::Instant::now();
            if let Err(e) =
                queue_scheduled(&state.pool, &state.drift, provider.model(), every).await
            {
                tracing::error!(error = ?e, "couldn't queue scheduled drift checks");
            }
        }
        if state.drift.triage_new_issues
            && let Err(e) = triage::queue_new_issues(&state.pool, provider.model()).await
        {
            tracing::error!(error = ?e, "couldn't queue triage of new issues");
        }
        match run_next(&state.pool, provider.as_ref(), &state.drift).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => tracing::error!(error = ?e, "drift worker failed"),
        }
        let _ = tokio::time::timeout(Duration::from_secs(30), state.drift_wake.notified()).await;
    }
}

/// Runs the oldest queued check, if any. Returns whether one ran.
pub async fn run_next(
    pool: &PgPool,
    provider: &dyn ModelProvider,
    settings: &DriftSettings,
) -> Result<bool, AppError> {
    let mut db = pool.begin().await?;
    type Claimed = (Uuid, Uuid, String, Option<Uuid>, Option<Uuid>);
    let claimed: Option<Claimed> = sqlx::query_as(
        "SELECT id, workspace_id, kind, service_id, issue_id FROM ai_runs WHERE status = 'queued' \
         ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(&mut *db)
    .await?;
    let Some((run_id, workspace_id, kind, service_id, issue_id)) = claimed else {
        return Ok(false);
    };
    let agent = ai_agent(&mut db, workspace_id).await?;
    let started = object(
        json!({ "status": AiRunStatus::Running, "started_at": Utc::now(), "model": provider.model() }),
    );
    write(
        &mut db,
        agent,
        vec![Operation::Update {
            model: ModelKind::AiRun,
            id: run_id,
            data: started,
        }],
    )
    .await?;
    db.commit().await?;

    tracing::info!(%run_id, %kind, "AI run started");
    let outcome = match (kind.as_str(), service_id, issue_id) {
        ("triage", _, Some(issue_id)) => {
            triage::run(pool, provider, settings, agent, run_id, issue_id).await
        }
        ("triage", _, None) => Err("the issue was deleted".to_string()),
        (_, Some(service_id), _) => {
            check::run(pool, provider, settings, agent, run_id, service_id).await
        }
        (_, None, _) => Err("the service was deleted".to_string()),
    };

    let mut finish = object(json!({ "finished_at": Utc::now() }));
    let mut db = pool.begin().await?;
    match outcome {
        Ok(result) => {
            finish.insert(
                "status".into(),
                json!(if result.error.is_some() {
                    "failed"
                } else {
                    "succeeded"
                }),
            );
            finish.insert("summary".into(), json!(result.summary));
            finish.insert("error".into(), json!(result.error));
            finish.insert("input_tokens".into(), json!(result.input_tokens));
            finish.insert("output_tokens".into(), json!(result.output_tokens));
            finish.insert("cost_usd".into(), json!(result.cost_usd));
            finish.insert("suggestions".into(), json!(result.suggestions));
            sqlx::query("INSERT INTO ai_run_transcripts (run_id, messages) VALUES ($1, $2)")
                .bind(run_id)
                .bind(Value::Array(result.messages))
                .execute(&mut *db)
                .await?;
        }
        Err(error) => {
            tracing::warn!(%run_id, %error, "AI run failed");
            finish.insert("status".into(), json!("failed"));
            finish.insert("error".into(), json!(error));
        }
    }
    write(
        &mut db,
        agent,
        vec![Operation::Update {
            model: ModelKind::AiRun,
            id: run_id,
            data: finish,
        }],
    )
    .await?;
    db.commit().await?;
    tracing::info!(%run_id, "AI run finished");
    Ok(true)
}

/// A server that stopped mid-run leaves it "running" forever; fail those.
/// At startup: runs left running were cut off by the restart; without a
/// model, queued ones can't run either.
async fn recover_interrupted(pool: &PgPool, model: bool) -> Result<(), AppError> {
    let stuck: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT id, workspace_id, kind, status FROM ai_runs \
         WHERE trigger <> 'agent' AND (status = 'running' OR (status = 'queued' AND NOT $1))",
    )
    .bind(model)
    .fetch_all(pool)
    .await?;
    for (id, workspace_id, kind, status) in stuck {
        let mut db = pool.begin().await?;
        let agent = ai_agent(&mut db, workspace_id).await?;
        let what = if kind == "triage" {
            "triage"
        } else {
            "the check"
        };
        let error = if status == "running" {
            format!("The server restarted during {what}.")
        } else {
            format!("No model is set up to run {what}.")
        };
        let data = object(json!({
            "status": "failed", "error": error, "finished_at": Utc::now(),
        }));
        write(
            &mut db,
            agent,
            vec![Operation::Update {
                model: ModelKind::AiRun,
                id,
                data,
            }],
        )
        .await?;
        db.commit().await?;
    }
    Ok(())
}

/// Queues a check for every service with a repository that hasn't been
/// checked within `every`.
pub async fn queue_scheduled(
    pool: &PgPool,
    settings: &DriftSettings,
    model: &str,
    every: chrono::Duration,
) -> Result<usize, AppError> {
    let due: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT s.id, s.workspace_id, s.repository FROM services s WHERE s.repository IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM ai_runs r WHERE r.service_id = s.id AND r.created_at > now() - $1::interval)",
    )
    .bind(format!("{} seconds", every.num_seconds()))
    .fetch_all(pool)
    .await?;
    let mut queued = 0;
    for (service_id, workspace_id, repository) in due {
        // A repository Forge can't find would only fail, check after check.
        if settings.resolve_repository(&repository).is_err() {
            continue;
        }
        let mut db = pool.begin().await?;
        let agent = ai_agent(&mut db, workspace_id).await?;
        if queue(&mut db, agent, service_id, "schedule", model)
            .await
            .is_ok()
        {
            db.commit().await?;
            queued += 1;
        }
    }
    Ok(queued)
}

/// Queues a check after a push changed a service's code, unless one ran
/// in the last half hour (pushes come in bursts).
pub async fn queue_after_push(
    pool: &PgPool,
    model: &str,
    workspace_id: Uuid,
    service_id: Uuid,
) -> Result<(), AppError> {
    let recent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ai_runs WHERE service_id = $1 AND created_at > now() - interval '30 minutes')",
    )
    .bind(service_id)
    .fetch_one(pool)
    .await?;
    if recent {
        return Ok(());
    }
    let mut db = pool.begin().await?;
    let agent = ai_agent(&mut db, workspace_id).await?;
    if queue(&mut db, agent, service_id, "push", model)
        .await
        .is_ok()
    {
        db.commit().await?;
    }
    Ok(())
}

/// Lets `main` hand the provider over without naming its type.
pub fn shared(provider: impl ModelProvider + 'static) -> Arc<dyn ModelProvider> {
    Arc::new(provider)
}
