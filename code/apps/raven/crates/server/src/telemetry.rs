//! Raven watches itself the way it watches Forge and Glyph: every API call
//! is a server span, the database queries inside it are child spans, and
//! what Raven logs during a call is kept with it. Records go to a writer in
//! the background, so recording never slows a call down; when the writer
//! falls far behind, records are dropped rather than queued without end.

use std::fmt::Write as _;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use uuid::Uuid;

use crate::error::ErrorNote;
use crate::state::AppState;

/// Each response names its call, so a problem someone reports can be found.
pub const TRACE_HEADER: &str = "x-raven-trace";

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: f64,
    pub is_error: bool,
    pub error_message: Option<String>,
    pub http_status: Option<i32>,
    pub version: Option<String>,
    pub attributes: Value,
}

#[derive(Debug, Clone)]
pub struct LogRecord {
    pub at: DateTime<Utc>,
    pub level: String,
    pub message: String,
    pub target: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Value,
}

/// What one app said at once.
#[derive(Debug, Default)]
pub struct Batch {
    /// The app, or `None` for Raven itself.
    pub app: Option<Uuid>,
    pub spans: Vec<SpanRecord>,
    pub logs: Vec<LogRecord>,
}

/// Hands records to the writer.
#[derive(Clone)]
pub struct Recorder {
    tx: mpsc::Sender<Batch>,
    version: Arc<str>,
}

impl Recorder {
    /// The recorder and the end the writer reads from. `version` marks
    /// Raven's own spans.
    pub fn new(version: &str) -> (Recorder, mpsc::Receiver<Batch>) {
        let (tx, rx) = mpsc::channel(10_000);
        let recorder = Recorder {
            tx,
            version: version.into(),
        };
        (recorder, rx)
    }

    fn send(&self, batch: Batch) {
        // Full means the database is far behind; dropping is kinder than waiting.
        let _ = self.tx.try_send(batch);
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Writes what the recorder hears until the server stops. Raven's own
/// records go to the app that is Raven itself; until a workspace watches
/// it, they're dropped.
pub async fn write(pool: PgPool, mut rx: mpsc::Receiver<Batch>) {
    let mut raven: Option<Uuid> = None;
    while let Some(mut batch) = rx.recv().await {
        if batch.app.is_none() {
            if raven.is_none() {
                raven = sqlx::query_scalar("SELECT id FROM apps WHERE is_self")
                    .fetch_optional(&pool)
                    .await
                    .ok()
                    .flatten();
            }
            let Some(raven) = raven else { continue };
            batch.app = Some(raven);
        }
        if let Err(e) = insert(&pool, &batch).await {
            // This target is left out of the record, so failing here can't loop.
            tracing::warn!(target: "raven_server::telemetry", error = %e, "couldn't record a batch");
        }
    }
}

/// A batch's spans, one array a column, in one statement.
const INSERT_SPANS: &str = "INSERT INTO spans (app_id, trace_id, span_id, parent_span_id, name, \
    kind, started_at, duration_ms, is_error, error_message, http_status, version, attributes) \
    SELECT $1, trace_id, span_id, parent_span_id, name, kind, started_at, duration_ms, is_error, \
      error_message, http_status, version, attributes \
    FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::timestamptz[], \
      $8::float8[], $9::bool[], $10::text[], $11::int4[], $12::text[], $13::jsonb[]) \
      WITH ORDINALITY AS t(trace_id, span_id, parent_span_id, name, kind, started_at, duration_ms, \
        is_error, error_message, http_status, version, attributes, n) \
    ORDER BY n";

/// A batch's log lines in one statement, in the order they came, so lines
/// with the same time keep it.
const INSERT_LOGS: &str = "INSERT INTO logs (app_id, at, level, message, target, trace_id, \
    span_id, attributes) \
    SELECT $1, at, level, message, target, trace_id, span_id, attributes \
    FROM unnest($2::timestamptz[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[], \
      $8::jsonb[]) WITH ORDINALITY AS t(at, level, message, target, trace_id, span_id, attributes, n) \
    ORDER BY n";

/// Stores a batch in one transaction: one statement for its spans and one
/// for its logs. One record Postgres won't take turns the whole batch away.
pub async fn insert(pool: &PgPool, batch: &Batch) -> Result<(), sqlx::Error> {
    let Some(app) = batch.app else { return Ok(()) };
    if batch.spans.is_empty() && batch.logs.is_empty() {
        return Ok(());
    }
    let mut db = pool.begin().await?;
    if !batch.spans.is_empty() {
        let spans = &batch.spans;
        let column = |f: fn(&SpanRecord) -> &str| spans.iter().map(f).collect::<Vec<_>>();
        let optional = |f: fn(&SpanRecord) -> Option<&str>| spans.iter().map(f).collect::<Vec<_>>();
        timed(
            "store spans",
            INSERT_SPANS,
            sqlx::query(INSERT_SPANS)
                .bind(app)
                .bind(column(|s| &s.trace_id))
                .bind(column(|s| &s.span_id))
                .bind(optional(|s| s.parent_span_id.as_deref()))
                .bind(column(|s| &s.name))
                .bind(column(|s| &s.kind))
                .bind(spans.iter().map(|s| s.started_at).collect::<Vec<_>>())
                .bind(spans.iter().map(|s| s.duration_ms).collect::<Vec<_>>())
                .bind(spans.iter().map(|s| s.is_error).collect::<Vec<_>>())
                .bind(optional(|s| s.error_message.as_deref()))
                .bind(spans.iter().map(|s| s.http_status).collect::<Vec<_>>())
                .bind(optional(|s| s.version.as_deref()))
                .bind(spans.iter().map(|s| &s.attributes).collect::<Vec<_>>())
                .execute(&mut *db),
        )
        .await?;
    }
    if !batch.logs.is_empty() {
        let logs = &batch.logs;
        let optional = |f: fn(&LogRecord) -> Option<&str>| logs.iter().map(f).collect::<Vec<_>>();
        timed(
            "store logs",
            INSERT_LOGS,
            sqlx::query(INSERT_LOGS)
                .bind(app)
                .bind(logs.iter().map(|l| l.at).collect::<Vec<_>>())
                .bind(logs.iter().map(|l| l.level.as_str()).collect::<Vec<_>>())
                .bind(logs.iter().map(|l| l.message.as_str()).collect::<Vec<_>>())
                .bind(optional(|l| l.target.as_deref()))
                .bind(optional(|l| l.trace_id.as_deref()))
                .bind(optional(|l| l.span_id.as_deref()))
                .bind(logs.iter().map(|l| &l.attributes).collect::<Vec<_>>())
                .execute(&mut *db),
        )
        .await?;
    }
    db.commit().await
}

/// A call in progress: what's been timed and logged inside it so far.
struct Call {
    trace_id: String,
    span_id: String,
    children: Vec<SpanRecord>,
    logs: Vec<LogRecord>,
}

tokio::task_local! {
    static CALL: Arc<Mutex<Call>>;
}

fn random_hex<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    hex::encode(bytes)
}

/// Middleware: records each API call as a server span named after its
/// route, with the child spans and logs made while it ran.
pub async fn record_calls(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string());
    let path = request.uri().path().to_string();
    // The built web app's files aren't actions.
    if route.is_none() && !path.starts_with("/api/") {
        return next.run(request).await;
    }
    let method = request.method().clone();
    let name = format!("{method} {}", route.as_deref().unwrap_or("(no such route)"));
    let call = Arc::new(Mutex::new(Call {
        trace_id: random_hex::<16>(),
        span_id: random_hex::<8>(),
        children: Vec::new(),
        logs: Vec::new(),
    }));
    let started_at = Utc::now();
    let clock = Instant::now();
    let mut response = CALL.scope(call.clone(), next.run(request)).await;
    let duration_ms = clock.elapsed().as_secs_f64() * 1000.0;

    let status = response.status();
    let call = std::mem::replace(
        &mut *call.lock().expect("not poisoned"),
        Call {
            trace_id: String::new(),
            span_id: String::new(),
            children: Vec::new(),
            logs: Vec::new(),
        },
    );
    if let Ok(value) = HeaderValue::from_str(&call.trace_id) {
        response.headers_mut().insert(TRACE_HEADER, value);
    }
    let is_error = status.is_server_error();
    let note = response
        .extensions()
        .get::<ErrorNote>()
        .map(|n| n.0.clone());
    let version = Some(state.recorder.version().to_string());
    let mut spans = vec![SpanRecord {
        trace_id: call.trace_id.clone(),
        span_id: call.span_id.clone(),
        parent_span_id: None,
        name,
        kind: "server".into(),
        started_at,
        duration_ms,
        is_error,
        error_message: if is_error { note } else { None },
        http_status: Some(i32::from(status.as_u16())),
        version: version.clone(),
        attributes: json!({ "http.request.method": method.as_str(), "url.path": path }),
    }];
    spans.extend(call.children.into_iter().map(|child| SpanRecord {
        version: version.clone(),
        ..child
    }));
    state.recorder.send(Batch {
        app: None,
        spans,
        logs: call.logs,
    });
    response
}

/// Times a database query as a child span of the call it runs in (outside
/// a call, it only runs it). `name` says what it's for; `statement` is the
/// SQL, shown with slow calls.
pub async fn timed<F: Future>(name: &str, statement: &str, query: F) -> F::Output {
    let started_at = Utc::now();
    let clock = Instant::now();
    let output = query.await;
    let duration_ms = clock.elapsed().as_secs_f64() * 1000.0;
    let _ = CALL.try_with(|call| {
        let mut call = call.lock().expect("not poisoned");
        let span = SpanRecord {
            trace_id: call.trace_id.clone(),
            span_id: random_hex::<8>(),
            parent_span_id: Some(call.span_id.clone()),
            name: name.into(),
            kind: "db".into(),
            started_at,
            duration_ms,
            is_error: false,
            error_message: None,
            http_status: None,
            version: None,
            attributes: json!({ "db.system": "postgresql", "db.statement": statement }),
        };
        call.children.push(span);
    });
    output
}

/// A tracing layer that keeps Raven's log lines: everything Raven itself
/// logs during a call, with that call, and warnings and errors from
/// anywhere.
pub struct Capture {
    recorder: Recorder,
}

impl Capture {
    pub fn new(recorder: Recorder) -> Self {
        Capture { recorder }
    }
}

#[derive(Default)]
struct Fields {
    message: String,
    rest: serde_json::Map<String, Value>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.rest.insert(field.name().into(), value.into());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let mut text = String::new();
        let _ = write!(text, "{value:?}");
        if field.name() == "message" {
            self.message = text;
        } else {
            self.rest.insert(field.name().into(), text.into());
        }
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let target = meta.target();
        if target.starts_with("raven_server::telemetry") {
            return;
        }
        let level = *meta.level();
        let ours = target.starts_with("raven_server");
        let serious = level <= Level::WARN;
        if !ours && !serious {
            return;
        }
        let in_call = CALL.try_with(|_| ()).is_ok();
        // Outside a call, the database's own complaints come from writing these.
        if !in_call && (!serious || target.starts_with("sqlx")) {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut log = LogRecord {
            at: Utc::now(),
            level: level.as_str().to_lowercase(),
            message: fields.message,
            target: Some(target.to_string()),
            trace_id: None,
            span_id: None,
            attributes: Value::Object(fields.rest),
        };
        let kept = CALL.try_with(|call| {
            let mut call = call.lock().expect("not poisoned");
            log.trace_id = Some(call.trace_id.clone());
            log.span_id = Some(call.span_id.clone());
            call.logs.push(log.clone());
        });
        if kept.is_err() {
            self.recorder.send(Batch {
                app: None,
                spans: Vec::new(),
                logs: vec![log],
            });
        }
    }
}
