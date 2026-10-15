//! Agent runners: a URL an agent's owner registers, where Forge POSTs a
//! signed wake-up when there's work for the agent: an issue assigned to it,
//! a comment on one of its issues, one sent back to it, one accepted out of
//! Triage, a mention. Forge only notifies; the runner (hosted by the agent's
//! owner) decides what to do and reads the rest through the MCP server.
//!
//! The sync engine queues wake-ups as it applies changes (`agent_wakeups`);
//! [`worker`] delivers them, retrying with backoff. While the agent is busy
//! they wait; when it's free, it gets one, in the order of the agents' work
//! queue: the one about the highest-priority issue, then the oldest wake-up. The rest wait until it has taken that one up
//! (started work) and is free again.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use forge_domain::{RunnerInfo, RunnerSecret, SetRunnerRequest, WakeupInfo};
use forge_sync::WORK_LEASE_MINUTES;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use crate::agents::ensure_agent;
use crate::auth::{Member, new_token};
use crate::error::AppError;
use crate::state::AppState;

/// The header carrying `sha256=<hex HMAC-SHA256 of the body>`, keyed with
/// the runner's secret.
pub const SIGNATURE_HEADER: &str = "x-forge-signature";

/// Runner secrets start with this, so they're easy to recognise.
const SECRET_PREFIX: &str = "forge_runner_";

/// How many of an agent's latest wake-ups the Agents page shows.
const SHOWN_WAKEUPS: i64 = 10;

/// Attempts before Forge gives up on a wake-up.
const MAX_ATTEMPTS: i32 = 6;

/// How long a runner has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Delivered and failed wake-ups are kept this many days.
const KEEP_DAYS: i32 = 30;

/// How long a woken agent has to start work before Forge counts it as free
/// again: a run that finds nothing to do never says so.
const TAKE_UP_MINUTES: i32 = 10;

/// SQL: whether the agent of membership `m` and runner `r` can be woken now.
/// Not while it's working on something (unless its run stopped), nor while
/// it hasn't taken up the last wake-up yet.
fn agent_is_free() -> String {
    format!(
        "NOT (m.working_on_id IS NOT NULL \
              AND m.working_seen_at > now() - make_interval(mins => {WORK_LEASE_MINUTES})) \
         AND (r.woken_at IS NULL OR r.woken_at <= now() - make_interval(mins => {TAKE_UP_MINUTES}))"
    )
}

/// `PUT /agents/{agent}/runner`: sets where the agent is woken. Admins only:
/// Forge will send requests to that URL. A new runner gets a secret, returned
/// once; changing the URL keeps it.
pub async fn set(
    State(state): State<AppState>,
    member: Member,
    Path((_, agent)): Path<(Uuid, Uuid)>,
    Json(request): Json<SetRunnerRequest>,
) -> Result<Json<Option<RunnerSecret>>, AppError> {
    member.require_admin()?;
    let url = clean_url(&request.url)?;
    let mut db = state.pool.begin().await?;
    ensure_agent(&mut db, member.actor.workspace_id, agent).await?;
    let secret = new_secret();
    let created: bool = sqlx::query_scalar(
        "INSERT INTO agent_runners (workspace_id, user_id, url, secret) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (workspace_id, user_id) DO UPDATE SET url = EXCLUDED.url, updated_at = now() \
         RETURNING xmax = 0",
    )
    .bind(member.actor.workspace_id)
    .bind(agent)
    .bind(&url)
    .bind(&secret)
    .fetch_one(&mut *db)
    .await?;
    db.commit().await?;
    Ok(Json(created.then_some(RunnerSecret { secret })))
}

/// `POST /agents/{agent}/runner/secret`: replaces the runner's secret, for
/// when the old one may have leaked. Admins only.
pub async fn new_runner_secret(
    State(state): State<AppState>,
    member: Member,
    Path((_, agent)): Path<(Uuid, Uuid)>,
) -> Result<Json<RunnerSecret>, AppError> {
    member.require_admin()?;
    let secret = new_secret();
    let updated = sqlx::query(
        "UPDATE agent_runners SET secret = $3, updated_at = now() \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(member.actor.workspace_id)
    .bind(agent)
    .bind(&secret)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::not_found("this agent has no runner"));
    }
    Ok(Json(RunnerSecret { secret }))
}

/// `DELETE /agents/{agent}/runner`: stops waking the agent. Wake-ups not
/// delivered yet are dropped; the record of earlier ones goes too.
pub async fn remove(
    State(state): State<AppState>,
    member: Member,
    Path((_, agent)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let mut db = state.pool.begin().await?;
    let removed = sqlx::query("DELETE FROM agent_runners WHERE workspace_id = $1 AND user_id = $2")
        .bind(member.actor.workspace_id)
        .bind(agent)
        .execute(&mut *db)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(AppError::not_found("this agent has no runner"));
    }
    sqlx::query("DELETE FROM agent_wakeups WHERE workspace_id = $1 AND user_id = $2")
        .bind(member.actor.workspace_id)
        .bind(agent)
        .execute(&mut *db)
        .await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The runners of a workspace's agents, with their latest wake-ups.
pub async fn of_workspace(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<Vec<(Uuid, RunnerInfo)>, AppError> {
    let runners: Vec<(Uuid, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT user_id, url, created_at FROM agent_runners WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    type WakeupRow = (
        Uuid,
        Uuid,
        String,
        String,
        String,
        i32,
        Option<String>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
    );
    // Pending ones wait while the agent isn't free.
    let wakeups: Vec<WakeupRow> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT w.user_id, w.id, w.event, t.key || '-' || i.number, \
                CASE WHEN w.status = 'pending' AND NOT ({free}) THEN 'waiting' ELSE w.status END, \
                w.attempts, w.last_error, w.created_at, w.delivered_at \
         FROM (SELECT *, row_number() OVER (PARTITION BY user_id ORDER BY created_at DESC, id DESC) AS n \
               FROM agent_wakeups WHERE workspace_id = $1) w \
         JOIN issues i ON i.id = w.issue_id JOIN teams t ON t.id = i.team_id \
         JOIN agent_runners r ON r.workspace_id = w.workspace_id AND r.user_id = w.user_id \
         JOIN workspace_members m ON m.workspace_id = w.workspace_id AND m.user_id = w.user_id \
         WHERE w.n <= $2 ORDER BY w.created_at DESC, w.id DESC",
        free = agent_is_free()
    )))
    .bind(workspace_id)
    .bind(SHOWN_WAKEUPS)
    .fetch_all(pool)
    .await?;

    Ok(runners
        .into_iter()
        .map(|(agent, url, created_at)| {
            let wakeups = wakeups
                .iter()
                .filter(|w| w.0 == agent)
                .filter_map(|w| {
                    Some(WakeupInfo {
                        id: w.1,
                        event: from_text(&w.2)?,
                        issue: w.3.clone(),
                        status: from_text(&w.4)?,
                        attempts: w.5,
                        last_error: w.6.clone(),
                        created_at: w.7,
                        delivered_at: w.8,
                    })
                })
                .collect();
            (
                agent,
                RunnerInfo {
                    url,
                    created_at,
                    wakeups,
                },
            )
        })
        .collect())
}

/// Delivers wake-ups for the life of the server: whenever a workspace
/// changes (that's when they're queued), and every few seconds for retries.
pub async fn worker(state: AppState) {
    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "couldn't start delivering wake-ups to agent runners");
            return;
        }
    };
    let mut changes = state.wake.subscribe();
    let mut last_prune = std::time::Instant::now() - Duration::from_secs(3600);
    loop {
        if last_prune.elapsed() > Duration::from_secs(3600) {
            last_prune = std::time::Instant::now();
            if let Err(e) = prune(&state.pool).await {
                tracing::error!(error = ?e, "couldn't prune old wake-ups");
            }
        }
        loop {
            match deliver_next(&state.pool, &client).await {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    tracing::error!(error = ?e, "couldn't deliver a wake-up");
                    break;
                }
            }
        }
        // Any change may have queued one; lagging behind just means many did.
        let _ = tokio::time::timeout(Duration::from_secs(5), changes.recv()).await;
    }
}

/// Delivers every wake-up that's due. Returns how many it tried.
pub async fn deliver_due(pool: &PgPool, client: &reqwest::Client) -> Result<usize, AppError> {
    let mut tried = 0;
    while deliver_next(pool, client).await? {
        tried += 1;
    }
    Ok(tried)
}

/// Tries the most important wake-up that's due, for an agent that's free:
/// the issue placed highest in the work queue, then the highest-priority one
/// (no priority last), then the oldest. Returns whether there was one.
async fn deliver_next(pool: &PgPool, client: &reqwest::Client) -> Result<bool, AppError> {
    // Claimed for a minute, so another worker (or this one after a crash)
    // leaves it alone while the runner is asked.
    type Claimed = (Uuid, Uuid, Uuid, String, i32, String, String, String);
    let claimed: Option<Claimed> = sqlx::query_as(AssertSqlSafe(format!(
        "UPDATE agent_wakeups w SET attempts = w.attempts + 1, \
                next_attempt_at = now() + interval '1 minute' \
         FROM agent_runners r, issues i, teams t \
         WHERE w.id = (SELECT q.id FROM agent_wakeups q \
                       JOIN agent_runners r ON r.workspace_id = q.workspace_id AND r.user_id = q.user_id \
                       JOIN workspace_members m ON m.workspace_id = q.workspace_id AND m.user_id = q.user_id \
                       JOIN issues qi ON qi.id = q.issue_id \
                       WHERE q.status = 'pending' AND q.next_attempt_at <= now() AND {free} \
                       ORDER BY CASE WHEN qi.priority = 0 THEN 5 ELSE qi.priority END, q.created_at, q.id \
                       LIMIT 1 FOR UPDATE OF q SKIP LOCKED) \
           AND r.workspace_id = w.workspace_id AND r.user_id = w.user_id \
           AND i.id = w.issue_id AND t.id = i.team_id \
         RETURNING w.id, w.workspace_id, w.user_id, w.event, w.attempts, r.url, r.secret, \
                   t.key || '-' || i.number",
        free = agent_is_free()
    )))
    .fetch_optional(pool)
    .await?;
    let Some((id, workspace_id, agent, event, attempts, url, secret, issue)) = claimed else {
        return Ok(false);
    };

    let body = json!({
        "id": id,
        "event": event,
        "issue": issue,
        "workspace_id": workspace_id,
        "agent_id": agent,
    })
    .to_string();
    let result = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "Forge")
        .header(SIGNATURE_HEADER, signature(&secret, body.as_bytes()))
        .body(body)
        .send()
        .await;
    let error = match result {
        Ok(response) if response.status().is_success() => None,
        Ok(response) => Some(format!("HTTP {}", response.status().as_u16())),
        Err(e) if e.is_timeout() => Some("timed out".to_string()),
        Err(e) if e.is_connect() => Some("couldn't connect".to_string()),
        Err(_) => Some("request failed".to_string()),
    };

    match error {
        None => {
            let mut db = pool.begin().await?;
            sqlx::query(
                "UPDATE agent_wakeups SET status = 'delivered', delivered_at = now(), last_error = NULL \
                 WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *db)
            .await?;
            // The next one waits until the agent has taken this one up.
            sqlx::query(
                "UPDATE agent_runners SET woken_at = now() WHERE workspace_id = $1 AND user_id = $2",
            )
            .bind(workspace_id)
            .bind(agent)
            .execute(&mut *db)
            .await?;
            db.commit().await?;
        }
        Some(error) => {
            tracing::warn!(%url, %error, attempts, "an agent runner didn't take a wake-up");
            sqlx::query(
                "UPDATE agent_wakeups SET last_error = $2, \
                    status = CASE WHEN $3 THEN 'failed' ELSE 'pending' END, \
                    next_attempt_at = now() + make_interval(secs => $4) \
                 WHERE id = $1",
            )
            .bind(id)
            .bind(error)
            .bind(attempts >= MAX_ATTEMPTS)
            .bind(retry_after(attempts).as_secs_f64())
            .execute(pool)
            .await?;
        }
    }
    Ok(true)
}

/// How long to wait before trying again after `attempts` tries: 30 seconds,
/// then 2, 10 and 30 minutes, then an hour.
fn retry_after(attempts: i32) -> Duration {
    Duration::from_secs(match attempts {
        ..=1 => 30,
        2 => 120,
        3 => 600,
        4 => 1800,
        _ => 3600,
    })
}

/// `sha256=` and the hex HMAC-SHA256 of `body`, keyed with the secret.
pub fn signature(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

async fn prune(pool: &PgPool) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM agent_wakeups WHERE status <> 'pending' \
         AND created_at < now() - make_interval(days => $1)",
    )
    .bind(KEEP_DAYS)
    .execute(pool)
    .await?;
    Ok(())
}

fn new_secret() -> String {
    format!("{SECRET_PREFIX}{}", new_token())
}

/// An http(s) URL, as given (trimmed).
fn clean_url(url: &str) -> Result<String, AppError> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| AppError::invalid("the runner URL isn't a valid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(AppError::invalid(
            "the runner URL must start with http:// or https://",
        ));
    }
    Ok(url.to_string())
}

/// A value stored as its snake_case name, such as an event or status.
fn from_text<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(json!(text)).ok()
}
