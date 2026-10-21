//! What the web app shows: each app's health (level 1) and its actions,
//! errors, slow calls and logs (level 2). Every query here is timed as part
//! of the call that runs it, so Raven's own slow pages show where the time
//! went.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::health::{self, Status};
use crate::state::AppState;
use crate::telemetry::timed;

/// The windows the pages offer, in seconds: 15 minutes, an hour, a day, a week.
const WINDOWS: [f64; 4] = [900.0, 3600.0, 86_400.0, 604_800.0];

#[derive(Deserialize)]
pub struct WindowQuery {
    window: Option<f64>,
}

impl WindowQuery {
    fn secs(&self) -> f64 {
        self.window
            .filter(|w| WINDOWS.contains(w))
            .unwrap_or(3600.0)
    }
}

#[derive(Serialize)]
pub struct AppSummary {
    key: String,
    name: String,
    health_url: Option<String>,
    /// Whether the app has a key to send its data with.
    sending: bool,
    status: Status,
}

/// `GET /api/apps`: every watched app and how it is.
pub async fn apps(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<AppSummary>>, AppError> {
    const SQL: &str =
        "SELECT key, name, health_url, ingest_key_hash IS NOT NULL FROM apps ORDER BY position";
    let rows: Vec<(String, String, Option<String>, bool)> =
        timed("list apps", SQL, sqlx::query_as(SQL).fetch_all(&state.pool)).await?;
    let mut apps = Vec::with_capacity(rows.len());
    for (key, name, health_url, sending) in rows {
        let status = timed(
            "app status",
            "health::status",
            health::status(&state.pool, &key),
        )
        .await?;
        apps.push(AppSummary {
            key,
            name,
            health_url,
            sending,
            status,
        });
    }
    Ok(Json(apps))
}

async fn known_app(pool: &PgPool, app: &str) -> Result<String, AppError> {
    const SQL: &str = "SELECT name FROM apps WHERE key = $1";
    timed(
        "find app",
        SQL,
        sqlx::query_scalar(SQL).bind(app).fetch_optional(pool),
    )
    .await?
    .ok_or_else(|| AppError::not_found("Raven doesn't watch that app"))
}

#[derive(Serialize)]
pub struct Slot {
    start: DateTime<Utc>,
    /// ok, warn, down, or none (nothing heard).
    state: &'static str,
    checks: i64,
    failed: i64,
    errors: i64,
}

#[derive(Serialize, FromRow)]
pub struct CheckRow {
    checked_at: DateTime<Utc>,
    ok: bool,
    status_code: Option<i32>,
    duration_ms: f64,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct HealthPage {
    name: String,
    health_url: Option<String>,
    /// Whether the app has a key to send its calls with.
    sending: bool,
    status: Status,
    /// The share of the last day's checks that passed.
    uptime: Option<f64>,
    /// How long a passing check took, typically, over the last day.
    typical_check_ms: Option<f64>,
    /// The last day in half hours, oldest first.
    slots: Vec<Slot>,
    recent_checks: Vec<CheckRow>,
}

const SLOT_SECS: i64 = 30 * 60;
const SLOTS: i64 = 48;

/// `GET /api/apps/{app}/health`
pub async fn health_page(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(app): Path<String>,
) -> Result<Json<HealthPage>, AppError> {
    let pool = &state.pool;
    let name = known_app(pool, &app).await?;
    const URL: &str = "SELECT health_url, ingest_key_hash IS NOT NULL FROM apps WHERE key = $1";
    let (health_url, sending): (Option<String>, bool) = timed(
        "health url",
        URL,
        sqlx::query_as(URL).bind(&app).fetch_one(pool),
    )
    .await?;
    let status = timed("app status", "health::status", health::status(pool, &app)).await?;

    const CHECKS: &str = "SELECT floor(extract(epoch FROM checked_at) / $2)::bigint, count(*), \
        count(*) FILTER (WHERE NOT ok) FROM health_checks \
        WHERE app = $1 AND checked_at > now() - interval '24 hours' GROUP BY 1";
    let checks: Vec<(i64, i64, i64)> = timed(
        "checks per half hour",
        CHECKS,
        sqlx::query_as(CHECKS)
            .bind(&app)
            .bind(SLOT_SECS as f64)
            .fetch_all(pool),
    )
    .await?;
    const CALLS: &str = "SELECT floor(extract(epoch FROM started_at) / $2)::bigint, count(*), \
        count(*) FILTER (WHERE is_error) FROM spans \
        WHERE app = $1 AND kind = 'server' AND started_at > now() - interval '24 hours' GROUP BY 1";
    let calls: HashMap<i64, (i64, i64)> = timed(
        "calls per half hour",
        CALLS,
        sqlx::query_as::<_, (i64, i64, i64)>(CALLS)
            .bind(&app)
            .bind(SLOT_SECS as f64)
            .fetch_all(pool),
    )
    .await?
    .into_iter()
    .map(|(b, n, e)| (b, (n, e)))
    .collect();
    let checks: HashMap<i64, (i64, i64)> =
        checks.into_iter().map(|(b, n, f)| (b, (n, f))).collect();
    let now_slot = Utc::now().timestamp().div_euclid(SLOT_SECS);
    let slots = (now_slot - SLOTS + 1..=now_slot)
        .map(|slot| {
            let (n, failed) = checks.get(&slot).copied().unwrap_or_default();
            let (heard, errors) = calls.get(&slot).copied().unwrap_or_default();
            let state = if n == 0 && heard == 0 {
                "none"
            } else if failed * 2 > n {
                "down"
            } else if failed > 0 || errors > 0 {
                "warn"
            } else {
                "ok"
            };
            Slot {
                start: DateTime::from_timestamp(slot * SLOT_SECS, 0).unwrap_or_default(),
                state,
                checks: n,
                failed,
                errors,
            }
        })
        .collect();

    const DAY: &str = "SELECT count(*), count(*) FILTER (WHERE ok), \
        percentile_cont(0.5) WITHIN GROUP (ORDER BY duration_ms) FILTER (WHERE ok) \
        FROM health_checks WHERE app = $1 AND checked_at > now() - interval '24 hours'";
    let (total, passed, typical): (i64, i64, Option<f64>) = timed(
        "uptime",
        DAY,
        sqlx::query_as(DAY).bind(&app).fetch_one(pool),
    )
    .await?;
    const RECENT: &str = "SELECT checked_at, ok, status_code, duration_ms, error FROM health_checks \
        WHERE app = $1 ORDER BY checked_at DESC LIMIT 6";
    let recent_checks = timed(
        "recent checks",
        RECENT,
        sqlx::query_as(RECENT).bind(&app).fetch_all(pool),
    )
    .await?;

    Ok(Json(HealthPage {
        name,
        health_url,
        sending,
        status,
        uptime: (total > 0).then(|| passed as f64 / total as f64),
        typical_check_ms: typical,
        slots,
        recent_checks,
    }))
}

/// One action over a window, next to its own last week.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ActionStats {
    pub name: String,
    pub calls: i64,
    pub errors: i64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub normal_p95_ms: Option<f64>,
    pub normal_calls: Option<i64>,
}

const ACTIONS: &str = "WITH recent AS ( \
        SELECT name, count(*) AS calls, count(*) FILTER (WHERE is_error) AS errors, \
          percentile_cont(0.5) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms, \
          percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms \
        FROM spans WHERE app = $1 AND kind = 'server' \
          AND started_at > now() - make_interval(secs => $2) \
        GROUP BY name), \
      normal AS ( \
        SELECT name, count(*) AS normal_calls, \
          percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS normal_p95_ms \
        FROM spans WHERE app = $1 AND kind = 'server' \
          AND started_at > now() - interval '7 days' \
        GROUP BY name) \
    SELECT r.name, r.calls, r.errors, r.p50_ms, r.p95_ms, n.normal_p95_ms, n.normal_calls \
    FROM recent r LEFT JOIN normal n USING (name)";

/// Each action's calls over the last `secs` seconds.
pub async fn action_stats(
    pool: &PgPool,
    app: &str,
    secs: f64,
) -> Result<Vec<ActionStats>, sqlx::Error> {
    timed(
        "action stats",
        ACTIONS,
        sqlx::query_as(ACTIONS).bind(app).bind(secs).fetch_all(pool),
    )
    .await
}

#[derive(Serialize)]
pub struct ActionRow {
    #[serde(flatten)]
    stats: ActionStats,
    calls_per_min: f64,
    error_rate: f64,
    slower_than_usual: bool,
}

impl ActionRow {
    fn new(stats: ActionStats, secs: f64) -> Self {
        ActionRow {
            calls_per_min: stats.calls as f64 / (secs / 60.0),
            error_rate: stats.errors as f64 / stats.calls.max(1) as f64,
            slower_than_usual: stats.slower_than_usual(),
            stats,
        }
    }
}

#[derive(Serialize, FromRow)]
pub struct ErrorGroup {
    action: String,
    message: String,
    /// Within the window.
    count: i64,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    first_version: Option<String>,
    last_trace_id: String,
}

const ERROR_GROUPS: &str = "SELECT name AS action, \
      coalesce(error_message, 'HTTP ' || http_status::text, 'failed') AS message, \
      count(*) FILTER (WHERE started_at > now() - make_interval(secs => $2)) AS count, \
      min(started_at) AS first_seen, max(started_at) AS last_seen, \
      (array_agg(version ORDER BY started_at))[1] AS first_version, \
      (array_agg(trace_id ORDER BY started_at DESC))[1] AS last_trace_id \
    FROM spans WHERE app = $1 AND kind = 'server' AND is_error AND ($3::text IS NULL OR name = $3) \
    GROUP BY 1, 2 \
    HAVING count(*) FILTER (WHERE started_at > now() - make_interval(secs => $2)) > 0 \
    ORDER BY last_seen DESC LIMIT 20";

async fn error_groups(
    pool: &PgPool,
    app: &str,
    secs: f64,
    action: Option<&str>,
) -> Result<Vec<ErrorGroup>, sqlx::Error> {
    timed(
        "error groups",
        ERROR_GROUPS,
        sqlx::query_as(ERROR_GROUPS)
            .bind(app)
            .bind(secs)
            .bind(action)
            .fetch_all(pool),
    )
    .await
}

#[derive(Serialize, FromRow)]
pub struct VersionMark {
    version: String,
    first_seen: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct TechnicalPage {
    name: String,
    window_secs: f64,
    /// Worst first: failing, then slower than usual, then the slowest.
    actions: Vec<ActionRow>,
    errors: Vec<ErrorGroup>,
    versions: Vec<VersionMark>,
}

/// `GET /api/apps/{app}/technical?window=<secs>`
pub async fn technical_page(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(app): Path<String>,
    Query(query): Query<WindowQuery>,
) -> Result<Json<TechnicalPage>, AppError> {
    let pool = &state.pool;
    let name = known_app(pool, &app).await?;
    let secs = query.secs();
    let mut actions: Vec<ActionRow> = action_stats(pool, &app, secs)
        .await?
        .into_iter()
        .map(|stats| ActionRow::new(stats, secs))
        .collect();
    actions.sort_by(|a, b| {
        (b.stats.errors > 0)
            .cmp(&(a.stats.errors > 0))
            .then(b.error_rate.total_cmp(&a.error_rate))
            .then(b.slower_than_usual.cmp(&a.slower_than_usual))
            .then(b.stats.p95_ms.total_cmp(&a.stats.p95_ms))
    });
    let errors = error_groups(pool, &app, secs, None).await?;
    const VERSIONS: &str = "SELECT version, min(started_at) AS first_seen FROM spans \
        WHERE app = $1 AND kind = 'server' AND version IS NOT NULL \
        GROUP BY version ORDER BY first_seen DESC LIMIT 5";
    let versions = timed(
        "versions",
        VERSIONS,
        sqlx::query_as(VERSIONS).bind(&app).fetch_all(pool),
    )
    .await?;
    Ok(Json(TechnicalPage {
        name,
        window_secs: secs,
        actions,
        errors,
        versions,
    }))
}

#[derive(Deserialize)]
pub struct ActionQuery {
    name: String,
    window: Option<f64>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Point {
    at: DateTime<Utc>,
    calls: i64,
    errors: i64,
    p50_ms: f64,
    p95_ms: f64,
}

/// Every bucket of the window, the quiet ones as zero calls, so a chart
/// spans the whole window rather than only the moments with calls.
fn whole_window(points: Vec<Point>, secs: f64, bucket_secs: f64, now: DateTime<Utc>) -> Vec<Point> {
    let bucket = bucket_secs as i64;
    let last = now.timestamp().div_euclid(bucket) * bucket;
    let count = (secs / bucket_secs).ceil() as i64;
    let mut by_start: HashMap<i64, Point> =
        points.into_iter().map(|p| (p.at.timestamp(), p)).collect();
    (0..count)
        .rev()
        .filter_map(|i| {
            let start = last - i * bucket;
            by_start.remove(&start).or_else(|| {
                Some(Point {
                    at: DateTime::from_timestamp(start, 0)?,
                    calls: 0,
                    errors: 0,
                    p50_ms: 0.0,
                    p95_ms: 0.0,
                })
            })
        })
        .collect()
}

#[derive(Serialize, FromRow)]
pub struct CallSummary {
    trace_id: String,
    started_at: DateTime<Utc>,
    duration_ms: f64,
    http_status: Option<i32>,
    is_error: bool,
}

#[derive(Serialize)]
pub struct ActionPage {
    app_name: String,
    name: String,
    window_secs: f64,
    bucket_secs: f64,
    stats: Option<ActionRow>,
    series: Vec<Point>,
    slowest: Vec<CallSummary>,
    errors: Vec<ErrorGroup>,
}

/// `GET /api/apps/{app}/action?name=<action>&window=<secs>`
pub async fn action_page(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(app): Path<String>,
    Query(query): Query<ActionQuery>,
) -> Result<Json<ActionPage>, AppError> {
    let pool = &state.pool;
    let app_name = known_app(pool, &app).await?;
    let secs = WindowQuery {
        window: query.window,
    }
    .secs();
    let stats = action_stats(pool, &app, secs)
        .await?
        .into_iter()
        .find(|a| a.name == query.name)
        .map(|stats| ActionRow::new(stats, secs));
    let bucket_secs = match secs {
        s if s <= 3600.0 => 60.0,
        s if s <= 86_400.0 => 900.0,
        _ => 3600.0,
    };
    const SERIES: &str = "SELECT to_timestamp(floor(extract(epoch FROM started_at) / $3) * $3) AS at, \
          count(*) AS calls, count(*) FILTER (WHERE is_error) AS errors, \
          percentile_cont(0.5) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms, \
          percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms \
        FROM spans WHERE app = $1 AND kind = 'server' AND name = $2 \
          AND started_at > now() - make_interval(secs => $4) \
        GROUP BY 1 ORDER BY 1";
    let filled: Vec<Point> = timed(
        "calls over time",
        SERIES,
        sqlx::query_as(SERIES)
            .bind(&app)
            .bind(&query.name)
            .bind(bucket_secs)
            .bind(secs)
            .fetch_all(pool),
    )
    .await?;
    let series = whole_window(filled, secs, bucket_secs, Utc::now());
    const SLOWEST: &str = "SELECT trace_id, started_at, duration_ms, http_status, is_error FROM spans \
        WHERE app = $1 AND kind = 'server' AND name = $2 \
          AND started_at > now() - make_interval(secs => $3) \
        ORDER BY duration_ms DESC LIMIT 8";
    let slowest = timed(
        "slowest calls",
        SLOWEST,
        sqlx::query_as(SLOWEST)
            .bind(&app)
            .bind(&query.name)
            .bind(secs)
            .fetch_all(pool),
    )
    .await?;
    let errors = error_groups(pool, &app, secs, Some(&query.name)).await?;
    Ok(Json(ActionPage {
        app_name,
        name: query.name,
        window_secs: secs,
        bucket_secs,
        stats,
        series,
        slowest,
        errors,
    }))
}

#[derive(Serialize, FromRow)]
pub struct SpanRow {
    span_id: String,
    parent_span_id: Option<String>,
    name: String,
    kind: String,
    started_at: DateTime<Utc>,
    duration_ms: f64,
    is_error: bool,
    error_message: Option<String>,
    http_status: Option<i32>,
    version: Option<String>,
    attributes: Value,
}

#[derive(Serialize, FromRow)]
pub struct LogRow {
    at: DateTime<Utc>,
    level: String,
    message: String,
    target: Option<String>,
    span_id: Option<String>,
    attributes: Value,
}

#[derive(Serialize)]
pub struct CallPage {
    app_name: String,
    trace_id: String,
    /// In the order they started; the call itself first.
    spans: Vec<SpanRow>,
    logs: Vec<LogRow>,
    /// The log lines just before the call, for context when it logged nothing.
    logs_before: Vec<LogRow>,
}

/// `GET /api/apps/{app}/calls/{trace_id}`: one call with what happened inside it.
pub async fn call_page(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path((app, trace_id)): Path<(String, String)>,
) -> Result<Json<CallPage>, AppError> {
    let pool = &state.pool;
    let app_name = known_app(pool, &app).await?;
    const SPANS: &str = "SELECT span_id, parent_span_id, name, kind, started_at, duration_ms, is_error, \
        error_message, http_status, version, attributes FROM spans \
        WHERE app = $1 AND trace_id = $2 ORDER BY started_at, (parent_span_id IS NOT NULL)";
    let spans: Vec<SpanRow> = timed(
        "spans of a call",
        SPANS,
        sqlx::query_as(SPANS)
            .bind(&app)
            .bind(&trace_id)
            .fetch_all(pool),
    )
    .await?;
    let Some(first) = spans.first() else {
        return Err(AppError::not_found(
            "that call isn't here: Raven keeps calls for 30 days",
        ));
    };
    let started = first.started_at;
    const LOGS: &str = "SELECT at, level, message, target, span_id, attributes FROM logs \
        WHERE app = $1 AND trace_id = $2 ORDER BY at, id";
    let logs = timed(
        "logs of a call",
        LOGS,
        sqlx::query_as(LOGS)
            .bind(&app)
            .bind(&trace_id)
            .fetch_all(pool),
    )
    .await?;
    const BEFORE: &str = "SELECT at, level, message, target, span_id, attributes FROM logs \
        WHERE app = $1 AND at BETWEEN $2 AND $3 AND trace_id IS DISTINCT FROM $4 \
        ORDER BY at DESC, id DESC LIMIT 5";
    let mut logs_before: Vec<LogRow> = timed(
        "logs before a call",
        BEFORE,
        sqlx::query_as(BEFORE)
            .bind(&app)
            .bind(started - Duration::minutes(5))
            .bind(started)
            .bind(&trace_id)
            .fetch_all(pool),
    )
    .await?;
    logs_before.reverse();
    Ok(Json(CallPage {
        app_name,
        trace_id,
        spans,
        logs,
        logs_before,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn whole_window_fills_quiet_buckets() {
        let busy = Point {
            at: at(1_200),
            calls: 3,
            errors: 1,
            p50_ms: 4.0,
            p95_ms: 9.0,
        };
        let series = whole_window(vec![busy], 300.0, 60.0, at(1_330));
        let starts: Vec<i64> = series.iter().map(|p| p.at.timestamp()).collect();
        assert_eq!(starts, [1_080, 1_140, 1_200, 1_260, 1_320]);
        let calls: Vec<i64> = series.iter().map(|p| p.calls).collect();
        assert_eq!(calls, [0, 0, 3, 0, 0]);
    }
}
