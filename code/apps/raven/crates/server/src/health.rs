//! Level 1: is each app up and well. Raven calls every watched app's health
//! URL, in every workspace, every half minute, and its own at the address
//! it listens on. An app is down when its last two checks failed, and
//! needs a look when it missed only the last one, or when level 2 sees
//! failing calls or an action slower than its own last week. Old records
//! are cleared out here too.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::metrics::{self, ActionStats};

const EVERY: Duration = Duration::from_secs(30);
const TIMEOUT: Duration = Duration::from_secs(5);
/// What Raven keeps of calls, logs and checks.
const KEEP_DAYS: i32 = 30;

/// Checks every app with a health URL, now and every half minute. Raven
/// itself answers at `self_url`.
pub async fn watch(pool: PgPool, self_url: String) {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent("raven")
        .build()
        .expect("an HTTP client with default settings builds");
    let mut ticks = tokio::time::interval(EVERY);
    let mut last_cleanup: Option<Instant> = None;
    loop {
        ticks.tick().await;
        let apps: Vec<(Uuid, String)> = match sqlx::query_as(
            "SELECT id, CASE WHEN is_self THEN $1 ELSE health_url END FROM apps \
             WHERE health_url IS NOT NULL OR is_self",
        )
        .bind(&self_url)
        .fetch_all(&pool)
        .await
        {
            Ok(apps) => apps,
            Err(e) => {
                tracing::warn!(error = %e, "couldn't read the apps to check");
                continue;
            }
        };
        let checks = apps.into_iter().map(|(app, url)| {
            let client = client.clone();
            let pool = pool.clone();
            async move {
                let check = check(&client, &url).await;
                let saved = sqlx::query(
                    "INSERT INTO health_checks (app_id, ok, status_code, duration_ms, error) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(app)
                .bind(check.ok)
                .bind(check.status_code)
                .bind(check.duration_ms)
                .bind(&check.error)
                .execute(&pool)
                .await;
                if let Err(e) = saved {
                    tracing::warn!(%app, error = %e, "couldn't save a health check");
                }
            }
        });
        futures::future::join_all(checks).await;

        if last_cleanup.is_none_or(|t| t.elapsed() > Duration::from_secs(60 * 60)) {
            last_cleanup = Some(Instant::now());
            if let Err(e) = clean_up(&pool).await {
                tracing::warn!(error = %e, "couldn't clear out old records");
            }
        }
    }
}

struct Check {
    ok: bool,
    status_code: Option<i32>,
    duration_ms: f64,
    error: Option<String>,
}

async fn check(client: &reqwest::Client, url: &str) -> Check {
    let clock = Instant::now();
    let result = client.get(url).send().await;
    let duration_ms = clock.elapsed().as_secs_f64() * 1000.0;
    match result {
        Ok(response) => {
            let status = response.status();
            Check {
                ok: status.is_success(),
                status_code: Some(i32::from(status.as_u16())),
                duration_ms,
                error: (!status.is_success()).then(|| format!("answered {status}")),
            }
        }
        Err(e) => Check {
            ok: false,
            status_code: None,
            duration_ms,
            error: Some(if e.is_timeout() {
                format!("no answer within {} seconds", TIMEOUT.as_secs())
            } else if e.is_connect() {
                "not answering: nothing is listening".into()
            } else {
                "the call failed".into()
            }),
        },
    }
}

async fn clean_up(pool: &PgPool) -> Result<(), sqlx::Error> {
    for sql in [
        "DELETE FROM spans WHERE started_at < now() - make_interval(days => $1)",
        "DELETE FROM logs WHERE at < now() - make_interval(days => $1)",
        "DELETE FROM health_checks WHERE checked_at < now() - make_interval(days => $1)",
    ] {
        sqlx::query(sql).bind(KEEP_DAYS).execute(pool).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Up,
    /// Up, but it missed its last check, or level 2 sees errors or slowness.
    Warn,
    Down,
    /// Not checked yet, or nothing to check.
    Unknown,
}

/// How an app is, in one word and one reason, with what to do about it.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub level: Level,
    pub reason: String,
    /// The action to look into, when level 2 is why.
    pub action: Option<String>,
    pub last_check: Option<DateTime<Utc>>,
    /// The first check it missed, while it is down. A time, not words, so
    /// each reader sees it in their own time zone.
    pub down_since: Option<DateTime<Utc>>,
}

/// An error as the start of a sentence: "Not answering: …".
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The window level 1 looks at for errors and slowness.
pub const RECENT_SECS: f64 = 15.0 * 60.0;

/// The app's status now.
pub async fn status(pool: &PgPool, app: Uuid) -> Result<Status, sqlx::Error> {
    let checks: Vec<(DateTime<Utc>, bool, Option<String>)> = sqlx::query_as(
        "SELECT checked_at, ok, error FROM health_checks WHERE app_id = $1 \
         ORDER BY checked_at DESC LIMIT 2",
    )
    .bind(app)
    .fetch_all(pool)
    .await?;
    let last_check = checks.first().map(|c| c.0);
    if checks.len() == 2 && checks.iter().all(|c| !c.1) {
        let since: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT min(checked_at) FROM health_checks WHERE app_id = $1 AND checked_at > \
             coalesce((SELECT max(checked_at) FROM health_checks WHERE app_id = $1 AND ok), '-infinity')",
        )
        .bind(app)
        .fetch_one(pool)
        .await?;
        let why = checks[0].2.as_deref().unwrap_or("health check failed");
        return Ok(Status {
            level: Level::Down,
            reason: sentence(why),
            action: None,
            last_check,
            down_since: since,
        });
    }

    // One miss may be a blip; it needs a look, not yet the red.
    if let Some((_, false, error)) = checks.first() {
        let why = error.as_deref().unwrap_or("health check failed");
        return Ok(Status {
            level: Level::Warn,
            reason: format!("Missed its last check ({why})"),
            action: None,
            last_check,
            down_since: None,
        });
    }

    let actions = metrics::action_stats(pool, app, RECENT_SECS).await?;
    if let Some(worst) = actions
        .iter()
        .filter(|a| a.errors > 0)
        .max_by_key(|a| a.errors)
    {
        let calls = if worst.errors == 1 { "call" } else { "calls" };
        return Ok(Status {
            level: Level::Warn,
            reason: format!(
                "{} failed {calls} to {} in the last 15 minutes",
                worst.errors, worst.name
            ),
            action: Some(worst.name.clone()),
            last_check,
            down_since: None,
        });
    }
    if let Some(slow) = actions
        .iter()
        .filter(|a| a.slower_than_usual())
        .max_by(|a, b| a.slowdown().total_cmp(&b.slowdown()))
    {
        return Ok(Status {
            level: Level::Warn,
            reason: format!(
                "{} is slower than usual: {} now, {} normally",
                slow.name,
                ms(slow.p95_ms),
                ms(slow.normal_p95_ms.unwrap_or_default())
            ),
            action: Some(slow.name.clone()),
            last_check,
            down_since: None,
        });
    }
    if checks.is_empty() && actions.is_empty() {
        return Ok(Status {
            level: Level::Unknown,
            reason: "Not heard from yet".into(),
            action: None,
            last_check,
            down_since: None,
        });
    }
    Ok(Status {
        level: Level::Up,
        reason: "Up and well".into(),
        action: None,
        last_check,
        down_since: None,
    })
}

/// A duration as people read it: "180 ms", "1.2 s".
pub fn ms(value: f64) -> String {
    if value >= 1000.0 {
        format!("{:.1} s", value / 1000.0)
    } else if value >= 10.0 {
        format!("{value:.0} ms")
    } else {
        format!("{value:.1} ms")
    }
}

impl ActionStats {
    /// Slower than its own last week: the slow end at least twice as slow,
    /// and by enough to notice.
    pub fn slower_than_usual(&self) -> bool {
        match (self.normal_p95_ms, self.normal_calls) {
            (Some(normal), Some(history)) => {
                self.calls >= 5
                    && history >= 50
                    && self.p95_ms > normal * 2.0
                    && self.p95_ms - normal > 20.0
            }
            _ => false,
        }
    }

    pub fn slowdown(&self) -> f64 {
        self.normal_p95_ms
            .filter(|n| *n > 0.0)
            .map_or(1.0, |n| self.p95_ms / n)
    }
}
