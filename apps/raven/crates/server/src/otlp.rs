//! How Forge and Glyph send their calls and logs: OpenTelemetry over HTTP
//! (OTLP), in its JSON form, at `/v1/traces` and `/v1/logs`. Each app sends
//! with its own key (`raven-server ingest-key <app>`), which says which app
//! it is.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use crate::auth::hash_token;
use crate::error::AppError;
use crate::state::AppState;
use crate::telemetry::{self, Batch, LogRecord, SpanRecord};

/// `POST /v1/traces`
pub async fn traces(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let app = sender(&state, &headers).await?;
    let request = json_body(&headers, &body)?;
    let batch = Batch {
        spans: spans(&request),
        app,
        logs: Vec::new(),
    };
    telemetry::insert(&state.pool, &batch).await?;
    Ok(Json(json!({})))
}

/// `POST /v1/logs`
pub async fn logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let app = sender(&state, &headers).await?;
    let request = json_body(&headers, &body)?;
    let batch = Batch {
        logs: log_records(&request),
        app,
        spans: Vec::new(),
    };
    telemetry::insert(&state.pool, &batch).await?;
    Ok(Json(json!({})))
}

/// The app whose key the request carries.
async fn sender(state: &AppState, headers: &HeaderMap) -> Result<String, AppError> {
    let key = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("send with the app's Raven key"))?;
    sqlx::query_scalar("SELECT key FROM apps WHERE ingest_key_hash = $1")
        .bind(hash_token(key.trim()))
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::unauthorized("that key isn't one of Raven's"))
}

fn json_body(headers: &HeaderMap, body: &[u8]) -> Result<Value, AppError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !content_type.starts_with("application/json") {
        return Err(AppError::unsupported(
            "Raven takes OTLP as JSON: set OTEL_EXPORTER_OTLP_PROTOCOL=http/json",
        ));
    }
    serde_json::from_slice(body).map_err(|e| AppError::invalid(format!("not OTLP JSON: {e}")))
}

fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// OTLP's `[{key, value: {stringValue: …}}]` as a plain JSON object.
fn attributes(value: &Value) -> Map<String, Value> {
    list(value, "attributes")
        .filter_map(|kv| {
            Some((
                kv.get("key")?.as_str()?.to_string(),
                any_value(kv.get("value")?),
            ))
        })
        .collect()
}

fn any_value(value: &Value) -> Value {
    if let Some(s) = value.get("stringValue") {
        return s.clone();
    }
    if let Some(b) = value.get("boolValue") {
        return b.clone();
    }
    if let Some(i) = value.get("intValue") {
        // 64-bit integers come as strings.
        return i
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .map_or_else(|| i.clone(), Value::from);
    }
    if let Some(d) = value.get("doubleValue") {
        return d.clone();
    }
    if let Some(a) = value.get("arrayValue") {
        return Value::Array(list(a, "values").map(any_value).collect());
    }
    if let Some(kv) = value.get("kvlistValue") {
        return Value::Object(
            list(kv, "values")
                .filter_map(|kv| {
                    Some((
                        kv.get("key")?.as_str()?.to_string(),
                        any_value(kv.get("value")?),
                    ))
                })
                .collect(),
        );
    }
    Value::Null
}

/// Nanoseconds since 1970, which OTLP JSON sends as a string or a number.
fn nanos(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value.as_i64())
        .or_else(|| value.as_u64().map(|n| n as i64))
}

fn time(nanos: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(nanos)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
}

fn spans(request: &Value) -> Vec<SpanRecord> {
    let mut records = Vec::new();
    for resource_spans in list(request, "resourceSpans") {
        let resource = resource_spans
            .get("resource")
            .map(attributes)
            .unwrap_or_default();
        let version = resource
            .get("service.version")
            .and_then(Value::as_str)
            .map(String::from);
        for scope in list(resource_spans, "scopeSpans") {
            for span in list(scope, "spans") {
                if let Some(record) = span_record(span, version.clone()) {
                    records.push(record);
                }
            }
        }
    }
    records
}

fn span_record(span: &Value, version: Option<String>) -> Option<SpanRecord> {
    let trace_id = text(span, "traceId")?;
    let span_id = text(span, "spanId")?;
    let start = nanos(span.get("startTimeUnixNano"))?;
    let end = nanos(span.get("endTimeUnixNano")).unwrap_or(start);
    let attributes = attributes(span);
    let str_attr = |key: &str| attributes.get(key).and_then(Value::as_str);
    let kind = match span.get("kind").and_then(Value::as_i64) {
        Some(2) => "server",
        _ if attributes.contains_key("db.system") || attributes.contains_key("db.system.name") => {
            "db"
        }
        Some(3) => "client",
        Some(4) => "producer",
        Some(5) => "consumer",
        _ => "internal",
    };
    let method = str_attr("http.request.method").or_else(|| str_attr("http.method"));
    let route = str_attr("http.route");
    let raw_name = span
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unnamed");
    // An action is its route, so calls with different ids count together.
    let name = match (kind, method, route) {
        ("server", Some(method), Some(route)) => format!("{method} {route}"),
        _ => raw_name.to_string(),
    };
    let http_status = attributes
        .get("http.response.status_code")
        .or_else(|| attributes.get("http.status_code"))
        .and_then(Value::as_i64)
        .and_then(|s| i32::try_from(s).ok());
    let status = span.get("status");
    let failed = status.and_then(|s| s.get("code")).and_then(Value::as_i64) == Some(2);
    let is_error = failed || (kind == "server" && http_status.is_some_and(|s| s >= 500));
    let error_message = status
        .and_then(|s| s.get("message"))
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(String::from)
        .or_else(|| {
            list(span, "events")
                .find(|e| e.get("name").and_then(Value::as_str) == Some("exception"))
                .and_then(|e| {
                    self::attributes(e)
                        .get("exception.message")
                        .and_then(Value::as_str)
                        .map(String::from)
                })
        })
        .filter(|_| is_error);
    Some(SpanRecord {
        trace_id,
        span_id,
        parent_span_id: text(span, "parentSpanId"),
        name,
        kind: kind.into(),
        started_at: time(start),
        duration_ms: (end - start).max(0) as f64 / 1_000_000.0,
        is_error,
        error_message,
        http_status,
        version,
        attributes: Value::Object(attributes),
    })
}

fn log_records(request: &Value) -> Vec<LogRecord> {
    let mut records = Vec::new();
    for resource_logs in list(request, "resourceLogs") {
        for scope in list(resource_logs, "scopeLogs") {
            let target = scope
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from);
            for log in list(scope, "logRecords") {
                let at = nanos(log.get("timeUnixNano"))
                    .filter(|n| *n > 0)
                    .or_else(|| nanos(log.get("observedTimeUnixNano")))
                    .map_or_else(Utc::now, time);
                let level = log
                    .get("severityText")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_lowercase)
                    .unwrap_or_else(|| severity(log.get("severityNumber").and_then(Value::as_i64)));
                let message = match log.get("body").map(any_value) {
                    Some(Value::String(s)) => s,
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                records.push(LogRecord {
                    at,
                    level,
                    message,
                    target: target.clone(),
                    trace_id: text(log, "traceId"),
                    span_id: text(log, "spanId"),
                    attributes: Value::Object(attributes(log)),
                });
            }
        }
    }
    records
}

/// OpenTelemetry's severity numbers in words.
fn severity(number: Option<i64>) -> String {
    match number.unwrap_or(9) {
        ..=4 => "trace",
        5..=8 => "debug",
        9..=12 => "info",
        13..=16 => "warn",
        _ => "error",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_server_span_with_a_route() {
        let request = json!({ "resourceSpans": [{
            "resource": { "attributes": [
                { "key": "service.name", "value": { "stringValue": "forge" } },
                { "key": "service.version", "value": { "stringValue": "abc123" } }
            ]},
            "scopeSpans": [{ "spans": [{
                "traceId": "5B8EFFF798038103D269B633813FC60C",
                "spanId": "EEE19B7EC3C1B174",
                "name": "GET /api/specs/42",
                "kind": 2,
                "startTimeUnixNano": "1760000000000000000",
                "endTimeUnixNano": "1760000000180000000",
                "attributes": [
                    { "key": "http.request.method", "value": { "stringValue": "GET" } },
                    { "key": "http.route", "value": { "stringValue": "/api/specs/{id}" } },
                    { "key": "http.response.status_code", "value": { "intValue": "500" } }
                ],
                "status": { "code": 2, "message": "spec store unavailable" }
            }]}]
        }]});
        let spans = spans(&request);
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span.name, "GET /api/specs/{id}");
        assert_eq!(span.kind, "server");
        assert_eq!(span.trace_id, "5b8efff798038103d269b633813fc60c");
        assert_eq!(span.duration_ms, 180.0);
        assert_eq!(span.http_status, Some(500));
        assert!(span.is_error);
        assert_eq!(
            span.error_message.as_deref(),
            Some("spec store unavailable")
        );
        assert_eq!(span.version.as_deref(), Some("abc123"));
    }

    #[test]
    fn a_client_span_with_a_database_is_a_query() {
        let request = json!({ "resourceSpans": [{ "scopeSpans": [{ "spans": [{
            "traceId": "aa", "spanId": "bb", "parentSpanId": "cc",
            "name": "SELECT specs", "kind": 3,
            "startTimeUnixNano": 1, "endTimeUnixNano": 2000001,
            "attributes": [{ "key": "db.system", "value": { "stringValue": "postgresql" } }]
        }]}]}]});
        let span = &spans(&request)[0];
        assert_eq!(span.kind, "db");
        assert_eq!(span.parent_span_id.as_deref(), Some("cc"));
        assert!(!span.is_error);
    }

    #[test]
    fn reads_log_lines() {
        let request = json!({ "resourceLogs": [{ "scopeLogs": [{
            "scope": { "name": "forge_server::specs" },
            "logRecords": [{
                "timeUnixNano": "1760000000000000000",
                "severityNumber": 17,
                "body": { "stringValue": "couldn't read spec 42" },
                "traceId": "AA", "spanId": "BB"
            }]
        }]}]});
        let logs = log_records(&request);
        assert_eq!(logs[0].level, "error");
        assert_eq!(logs[0].message, "couldn't read spec 42");
        assert_eq!(logs[0].trace_id.as_deref(), Some("aa"));
        assert_eq!(logs[0].target.as_deref(), Some("forge_server::specs"));
    }
}
