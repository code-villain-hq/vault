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

use crate::error::ErrorNote;
use crate::state::AppState;

/// Raven's own key among the apps.
pub const RAVEN: &str = "raven";
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
    pub app: String,
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

/// Writes what the recorder hears until the server stops.
pub async fn write(pool: PgPool, mut rx: mpsc::Receiver<Batch>) {
    while let Some(batch) = rx.recv().await {
        if let Err(e) = insert(&pool, &batch).await {
            // This target is left out of the record, so failing here can't loop.
            tracing::warn!(target: "raven_server::telemetry", error = %e, "couldn't record a batch");
        }
    }
}

/// Stores a batch in one transaction.
pub async fn insert(pool: &PgPool, batch: &Batch) -> Result<(), sqlx::Error> {
    let mut db = pool.begin().await?;
    for span in &batch.spans {
        sqlx::query(
            "INSERT INTO spans (app, trace_id, span_id, parent_span_id, name, kind, started_at, \
             duration_ms, is_error, error_message, http_status, version, attributes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(&batch.app)
        .bind(&span.trace_id)
        .bind(&span.span_id)
        .bind(&span.parent_span_id)
        .bind(&span.name)
        .bind(&span.kind)
        .bind(span.started_at)
        .bind(span.duration_ms)
        .bind(span.is_error)
        .bind(&span.error_message)
        .bind(span.http_status)
        .bind(&span.version)
        .bind(&span.attributes)
        .execute(&mut *db)
        .await?;
    }
    for log in &batch.logs {
        sqlx::query(
            "INSERT INTO logs (app, at, level, message, target, trace_id, span_id, attributes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&batch.app)
        .bind(log.at)
        .bind(&log.level)
        .bind(&log.message)
        .bind(&log.target)
        .bind(&log.trace_id)
        .bind(&log.span_id)
        .bind(&log.attributes)
        .execute(&mut *db)
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
        app: RAVEN.into(),
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
                app: RAVEN.into(),
                spans: Vec::new(),
                logs: vec![log],
            });
        }
    }
}
