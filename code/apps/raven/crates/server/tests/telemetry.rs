//! What apps send is stored whole, a batch at a time, and the Technical
//! page names when each error and version began, however long ago.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// Adds an app the way Settings → Apps does and gives it a key; returns
/// its id and key.
async fn app_with_key(app: &App, session: &str, workspace: &str, key: &str) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let created = app
        .sync(
            session,
            workspace,
            json!([{ "action": "create", "model": "App", "id": id, "data": {
                "key": key, "name": key, "health_url": Value::Null,
            }}]),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let made = app
        .post(
            &format!("/api/w/{workspace}/apps/{key}/key"),
            Some(session),
            json!({}),
        )
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.body);
    (id, made.body["key"].as_str().unwrap().to_string())
}

fn server_span(trace: &str, span: &str, status: Value) -> Value {
    json!({
        "traceId": trace,
        "spanId": span,
        "name": "GET /api/specs/42",
        "kind": 2,
        "startTimeUnixNano": "1760000000000000000",
        "endTimeUnixNano": "1760000000180000000",
        "attributes": [
            { "key": "http.request.method", "value": { "stringValue": "GET" } },
            { "key": "http.route", "value": { "stringValue": "/api/specs/{id}" } },
            { "key": "http.response.status_code", "value": { "intValue": "200" } }
        ],
        "status": status,
    })
}

/// A stored log line: message, level, target, trace and attributes.
type LogLine = (String, String, Option<String>, Option<String>, Value);

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_is_stored_whole(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let (forge, key) = app_with_key(&app, &ada, &workspace, "forge").await;

    let traces = json!({ "resourceSpans": [
        {
            "resource": { "attributes": [
                { "key": "service.version", "value": { "stringValue": "abc123" } }
            ]},
            "scopeSpans": [{ "spans": [
                server_span("AA", "01", json!({})),
                {
                    "traceId": "AA", "spanId": "02", "parentSpanId": "01",
                    "name": "SELECT specs", "kind": 3,
                    "startTimeUnixNano": "1760000000010000000",
                    "endTimeUnixNano": "1760000000020000000",
                    "attributes": [{ "key": "db.system", "value": { "stringValue": "postgresql" } }]
                }
            ]}]
        },
        { "scopeSpans": [{ "spans": [
            server_span("BB", "03", json!({ "code": 2, "message": "spec store unavailable" }))
        ]}]}
    ]});
    assert_eq!(app.send("/v1/traces", &key, traces).await, StatusCode::OK);

    type Row = (
        Uuid,
        String,
        Option<String>,
        String,
        String,
        f64,
        bool,
        Option<String>,
        Option<i32>,
        Option<String>,
        Value,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT app_id, span_id, parent_span_id, name, kind, duration_ms, is_error, \
         error_message, http_status, version, attributes FROM spans ORDER BY span_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.0 == forge));
    let (_, span, parent, name, kind, duration, is_error, message, status, version, attributes) =
        &rows[0];
    assert_eq!(
        (span.as_str(), parent, name.as_str(), kind.as_str()),
        ("01", &None, "GET /api/specs/{id}", "server")
    );
    assert_eq!(
        (*duration, *is_error, message, *status),
        (180.0, false, &None, Some(200))
    );
    assert_eq!(version.as_deref(), Some("abc123"));
    assert_eq!(attributes["http.route"], "/api/specs/{id}");
    let child = &rows[1];
    assert_eq!((child.2.as_deref(), child.4.as_str()), (Some("01"), "db"));
    assert_eq!(child.5, 10.0);
    let failed = &rows[2];
    assert!(failed.6);
    assert_eq!(failed.7.as_deref(), Some("spec store unavailable"));
    assert_eq!(failed.9, None);

    let logs = json!({ "resourceLogs": [{ "scopeLogs": [{
        "scope": { "name": "forge_server::specs" },
        "logRecords": (0..5).map(|i| json!({
            "timeUnixNano": "1760000000000000000",
            "severityNumber": 9,
            "body": { "stringValue": format!("line {i}") },
            "traceId": "AA",
            "attributes": [{ "key": "n", "value": { "intValue": i.to_string() } }]
        })).collect::<Vec<_>>()
    }]}]});
    assert_eq!(app.send("/v1/logs", &key, logs).await, StatusCode::OK);
    // Lines with the same time keep the order they came in.
    let lines: Vec<LogLine> = sqlx::query_as(
        "SELECT message, level, target, trace_id, attributes FROM logs WHERE app_id = $1 \
         ORDER BY at, id",
    )
    .bind(forge)
    .fetch_all(&pool)
    .await
    .unwrap();
    let messages: Vec<&str> = lines.iter().map(|l| l.0.as_str()).collect();
    assert_eq!(messages, ["line 0", "line 1", "line 2", "line 3", "line 4"]);
    assert_eq!(lines[3].1, "info");
    assert_eq!(lines[3].2.as_deref(), Some("forge_server::specs"));
    assert_eq!(lines[3].3.as_deref(), Some("aa"));
    assert_eq!(lines[3].4, json!({ "n": 3 }));
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_bad_record_turns_the_whole_batch_away(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let (_, key) = app_with_key(&app, &ada, &workspace, "forge").await;

    // Postgres takes no NUL in text.
    let traces = json!({ "resourceSpans": [{ "scopeSpans": [{ "spans": [
        server_span("AA", "01", json!({})),
        server_span("BB", "02", json!({ "code": 2, "message": "bad \u{0} byte" })),
        server_span("CC", "03", json!({})),
    ]}]}]});
    assert_eq!(
        app.send("/v1/traces", &key, traces).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let logs = json!({ "resourceLogs": [{ "scopeLogs": [{ "logRecords": [
        { "timeUnixNano": "1760000000000000000", "body": { "stringValue": "fine" } },
        { "timeUnixNano": "1760000000000000000", "body": { "stringValue": "bad \u{0} byte" } },
    ]}]}]});
    assert_eq!(
        app.send("/v1/logs", &key, logs).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let stored: (i64, i64) =
        sqlx::query_as("SELECT (SELECT count(*) FROM spans), (SELECT count(*) FROM logs)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, (0, 0));

    // A record that can't be read at all is left out; the rest are kept.
    let traces = json!({ "resourceSpans": [{ "scopeSpans": [{ "spans": [
        server_span("AA", "01", json!({})),
        { "spanId": "02", "name": "no trace", "startTimeUnixNano": "1" },
    ]}]}]});
    assert_eq!(app.send("/v1/traces", &key, traces).await, StatusCode::OK);
    let spans: i64 = sqlx::query_scalar("SELECT count(*) FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(spans, 1);
}

/// A server span `ago` before now.
#[allow(clippy::too_many_arguments)]
async fn span_at(
    pool: &PgPool,
    app: Uuid,
    trace: &str,
    name: &str,
    ago: Duration,
    error: Option<&str>,
    http_status: i32,
    version: &str,
) -> DateTime<Utc> {
    let at = DateTime::from_timestamp((Utc::now() - ago).timestamp(), 0).unwrap();
    sqlx::query(
        "INSERT INTO spans (app_id, trace_id, span_id, name, kind, started_at, duration_ms, \
         is_error, error_message, http_status, version) \
         VALUES ($1, $2, $2, $3, 'server', $4, 5, $5, $6, $7, $8)",
    )
    .bind(app)
    .bind(trace)
    .bind(name)
    .bind(at)
    .bind(error.is_some() || http_status >= 500)
    .bind(error)
    .bind(http_status)
    .bind(version)
    .execute(pool)
    .await
    .unwrap();
    at
}

fn time(value: &Value) -> DateTime<Utc> {
    value.as_str().unwrap().parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn technical_names_when_errors_and_versions_began(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let (forge, _) = app_with_key(&app, &ada, &workspace, "forge").await;
    let (a, b) = ("GET /a", "GET /b");
    let day = Duration::days(1);
    let minute = Duration::minutes(1);

    let v1_began = span_at(&pool, forge, "t0", a, day * 12, None, 200, "v1").await;
    let boom_began = span_at(&pool, forge, "t1", a, day * 10, Some("boom"), 500, "v1").await;
    let v2_began = span_at(&pool, forge, "t2", a, day * 3, None, 200, "v2").await;
    // A failure outside the window doesn't show.
    span_at(&pool, forge, "t3", a, day * 2, None, 503, "v2").await;
    span_at(&pool, forge, "t4", a, minute * 20, Some("boom"), 500, "v2").await;
    let boom_last = span_at(&pool, forge, "t5", a, minute * 10, Some("boom"), 500, "v2").await;
    let v3_began = span_at(&pool, forge, "t6", b, minute * 30, None, 200, "v3").await;
    let nope = span_at(&pool, forge, "t7", b, minute * 5, Some("nope"), 500, "v3").await;
    // Another app's calls stay its own.
    let (glyph, _) = app_with_key(&app, &ada, &workspace, "glyph").await;
    span_at(&pool, glyph, "g1", a, day * 20, Some("boom"), 500, "v0").await;
    span_at(&pool, glyph, "g2", a, minute, None, 200, "v9").await;

    let page = app
        .get(
            &format!("/api/w/{workspace}/apps/forge/technical?window=3600"),
            &ada,
        )
        .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let errors = page.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2, "{}", page.body);
    assert_eq!(errors[0]["action"], b);
    assert_eq!(errors[0]["message"], "nope");
    assert_eq!(errors[0]["count"], 1);
    assert_eq!(time(&errors[0]["first_seen"]), nope);
    assert_eq!(errors[0]["first_version"], "v3");
    assert_eq!(errors[0]["last_trace_id"], "t7");
    assert_eq!(errors[1]["action"], a);
    assert_eq!(errors[1]["message"], "boom");
    assert_eq!(errors[1]["count"], 2);
    assert_eq!(time(&errors[1]["first_seen"]), boom_began);
    assert_eq!(time(&errors[1]["last_seen"]), boom_last);
    assert_eq!(errors[1]["first_version"], "v1");
    assert_eq!(errors[1]["last_trace_id"], "t5");

    let versions: Vec<(String, DateTime<Utc>)> = page.body["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["version"].as_str().unwrap().to_string(),
                time(&v["first_seen"]),
            )
        })
        .collect();
    assert_eq!(
        versions,
        [
            ("v3".to_string(), v3_began),
            ("v2".to_string(), v2_began),
            ("v1".to_string(), v1_began),
        ]
    );

    // A week's window takes in the failure two days ago, named by its status.
    let week = app
        .get(
            &format!("/api/w/{workspace}/apps/forge/technical?window=604800"),
            &ada,
        )
        .await;
    let errors = week.body["errors"].as_array().unwrap();
    let messages: Vec<&str> = errors
        .iter()
        .map(|e| e["message"].as_str().unwrap())
        .collect();
    assert_eq!(messages, ["nope", "boom", "HTTP 503"]);

    // One action's page shows only its errors.
    let action = app
        .get(
            &format!("/api/w/{workspace}/apps/forge/action?name=GET%20%2Fa&window=3600"),
            &ada,
        )
        .await;
    let errors = action.body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(time(&errors[0]["first_seen"]), boom_began);
    assert_eq!(errors[0]["first_version"], "v1");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_big_batch_goes_in_at_once(pool: PgPool) {
    use raven_server::telemetry::{Batch, LogRecord, SpanRecord, insert};

    let app = App::new(pool.clone());
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let (forge, _) = app_with_key(&app, &ada, &workspace, "forge").await;
    // More rows than bind parameters would allow one VALUES row each.
    let started_at = Utc::now();
    let batch = Batch {
        app: Some(forge),
        spans: (0..10_000)
            .map(|i| SpanRecord {
                trace_id: format!("t{i}"),
                span_id: format!("s{i}"),
                parent_span_id: (i % 2 == 1).then(|| format!("s{}", i - 1)),
                name: "GET /".into(),
                kind: "server".into(),
                started_at,
                duration_ms: f64::from(i),
                is_error: i % 7 == 0,
                error_message: (i % 7 == 0).then(|| "boom".into()),
                http_status: (i % 3 == 0).then_some(200),
                version: None,
                attributes: json!({ "i": i }),
            })
            .collect(),
        logs: (0..10_000)
            .map(|i| LogRecord {
                at: started_at,
                level: "info".into(),
                message: format!("line {i}"),
                target: None,
                trace_id: Some(format!("t{i}")),
                span_id: None,
                attributes: json!({}),
            })
            .collect(),
    };
    insert(&pool, &batch).await.unwrap();
    let spans: (i64, i64, i64, f64, i64) = sqlx::query_as(
        "SELECT count(*), count(parent_span_id), count(*) FILTER (WHERE is_error), \
         sum(duration_ms), count(http_status) FROM spans WHERE app_id = $1",
    )
    .bind(forge)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(spans, (10_000, 5_000, 1_429, 49_995_000.0, 3_334));
    let in_order: Vec<String> =
        sqlx::query_scalar("SELECT message FROM logs WHERE app_id = $1 ORDER BY id")
            .bind(forge)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(in_order.len(), 10_000);
    assert!(
        in_order
            .iter()
            .enumerate()
            .all(|(i, m)| *m == format!("line {i}"))
    );
    // Nothing to store is no work.
    insert(
        &pool,
        &Batch {
            app: Some(forge),
            ..Batch::default()
        },
    )
    .await
    .unwrap();
}
