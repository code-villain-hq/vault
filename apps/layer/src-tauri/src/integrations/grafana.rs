//! Grafana: its Loki datasources, read through the site's datasource proxy
//! (LOG-*). Only reads; nothing here changes anything in Grafana.

use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::http_client;
use crate::error::{Error, Result};

/// Lines returned by one call, unless asked for fewer (OBS-4).
pub const DEFAULT_LIMIT: u32 = 100;
/// The most lines one call returns. Loki's own default ceiling is 5,000, and
/// an agent reading more than a thousand lines at once is reading noise.
pub const MAX_LIMIT: u32 = 1000;
/// A stack trace on one line can be tens of kilobytes; one such line should
/// not crowd out the rest of the answer.
const MAX_LINE: usize = 2000;

pub struct Grafana {
    base_url: String,
    auth: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Datasource {
    pub uid: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "isDefault")]
    pub is_default: bool,
}

/// What an Explore link asks for (OBS-3).
#[derive(Debug, Default, PartialEq)]
pub struct ExploreLink {
    pub datasource: Option<String>,
    pub expr: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// One query: LogQL over a time range, in nanoseconds since the epoch.
pub struct LogQuery<'a> {
    pub expr: &'a str,
    pub from_ns: i64,
    pub to_ns: i64,
    pub limit: u32,
}

impl Grafana {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        super::require_https(base_url, "The Grafana site")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth: format!("Bearer {token}"),
            client: http_client(),
        })
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let res = self
            .client
            .get(format!("{}{path}", self.base_url))
            .header("Authorization", &self.auth)
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Other(
                "Grafana does not accept the saved token (it may have expired or been deleted). \
                 Create a new service account token and enter it in Settings → Observability."
                    .into(),
            ));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(Error::Other(forbidden(path, &body)));
        }
        if !status.is_success() {
            return Err(Error::Other(format!("Grafana {status}: {}", trim(&body))));
        }
        Ok(serde_json::from_str(&body)?)
    }

    /// Every datasource the token may see. Connecting asks this first, so a
    /// token that cannot read them is refused before it is saved (OBS-1).
    pub async fn datasources(&self) -> Result<Vec<Datasource>> {
        let v = self.get("/api/datasources", &[]).await?;
        Ok(serde_json::from_value(v)?)
    }

    fn loki(uid: &str, rest: &str) -> Result<String> {
        Ok(format!("/api/datasources/proxy/uid/{}/loki/api/v1/{rest}", uid_segment(uid)?))
    }

    /// Lines for a LogQL query, newest first (OBS-4).
    pub async fn query_range(&self, uid: &str, q: &LogQuery<'_>) -> Result<Value> {
        let v = self
            .get(
                &Self::loki(uid, "query_range")?,
                &[
                    ("query", q.expr.to_string()),
                    ("start", q.from_ns.to_string()),
                    ("end", q.to_ns.to_string()),
                    ("limit", q.limit.to_string()),
                    ("direction", "backward".to_string()),
                ],
            )
            .await?;
        Ok(lines(&v, q.limit))
    }

    /// A datasource's label names, or one label's values. `selector` narrows
    /// the values to the streams it matches.
    pub async fn labels(
        &self,
        uid: &str,
        name: Option<&str>,
        selector: Option<&str>,
        from_ns: i64,
        to_ns: i64,
    ) -> Result<Vec<String>> {
        let path = match name {
            Some(n) => Self::loki(uid, &format!("label/{}/values", label_segment(n)?))?,
            None => Self::loki(uid, "labels")?,
        };
        let mut query = vec![("start", from_ns.to_string()), ("end", to_ns.to_string())];
        if let Some(s) = selector.filter(|s| !s.trim().is_empty()) {
            query.push(("query", s.to_string()));
        }
        let v = self.get(&path, &query).await?;
        Ok(v.get("data")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default())
    }
}

/// Loki's answer as the agent reads it: one list of lines across every
/// stream, newest first, each naming its stream, and the streams' labels
/// once. A metric query has no lines, and goes back as Loki sent it.
fn lines(v: &Value, limit: u32) -> Value {
    let data = v.get("data").cloned().unwrap_or(Value::Null);
    let kind = data.get("resultType").and_then(Value::as_str).unwrap_or_default();
    if kind != "streams" {
        return json!({ "result_type": kind, "result": data.get("result") });
    }
    let mut streams = Vec::new();
    let mut rows: Vec<(i64, usize, String)> = Vec::new();
    for (i, s) in data.get("result").and_then(Value::as_array).into_iter().flatten().enumerate() {
        streams.push(s.get("stream").cloned().unwrap_or(json!({})));
        for pair in s.get("values").and_then(Value::as_array).into_iter().flatten() {
            let ts = pair.get(0).and_then(Value::as_str).and_then(|t| t.parse::<i64>().ok());
            let line = pair.get(1).and_then(Value::as_str);
            if let (Some(ts), Some(line)) = (ts, line) {
                rows.push((ts, i, clip(line)));
            }
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    rows.truncate(limit as usize);
    // Loki stops at the limit, so a full page may have more behind it. The
    // next page ends where this one did; Loki's end is inclusive at worst, so
    // lines at that very nanosecond can repeat, but none are skipped.
    let more = rows.len() >= limit as usize;
    let next_to = more.then(|| rows.last().map(|r| rfc3339(r.0 + 1))).flatten();
    json!({
        "lines": rows
            .into_iter()
            .map(|(ts, stream, line)| json!({ "time": rfc3339(ts), "stream": stream, "line": line }))
            .collect::<Vec<_>>(),
        "streams": streams,
        "more": more,
        "next_to": next_to,
    })
}

fn clip(line: &str) -> String {
    let count = line.chars().count();
    if count <= MAX_LINE {
        return line.to_string();
    }
    let kept: String = line.chars().take(MAX_LINE).collect();
    format!("{kept}… [cut: {count} characters]")
}

/// What a 403 means (OBS-6). A datasource the token may not query is also
/// left out of `/api/datasources`, so agents went on reading the ones it
/// could see and found nothing, or guessed a uid and got a bare "Access
/// denied to datasource" (Grafana Cloud's own logs datasource, here).
fn forbidden(path: &str, body: &str) -> String {
    if path.starts_with("/api/datasources/proxy/") {
        format!(
            "Grafana refused this datasource ({}). The service account behind the saved token \
             may not query it, or there is no such datasource: grafana_datasources lists the ones \
             it may. If the logs are in one missing from that list, the user has to give the \
             service account access to it in Grafana (the datasource's Permissions, or a role \
             that can query it).",
            trim(body)
        )
    } else {
        format!(
            "Grafana refused to list datasources ({}). The service account behind the saved token \
             needs a role that can read and query datasources.",
            trim(body)
        )
    }
}

fn trim(body: &str) -> String {
    body.chars().take(300).collect()
}

pub fn rfc3339(ns: i64) -> String {
    Utc.timestamp_nanos(ns).to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// A datasource uid as one segment of the proxy's path (OBS-6). It reaches
/// here from an agent, or from a link in a ticket someone else wrote, and
/// `../../api/…` would be an authenticated GET anywhere on the site.
fn uid_segment(raw: &str) -> Result<&str> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(Error::Other(format!("{raw:?} is not a Grafana datasource uid")));
    }
    Ok(raw)
}

/// Loki's label names are Prometheus's: a letter or `_`, then letters,
/// digits and `_`.
fn label_segment(raw: &str) -> Result<&str> {
    let raw = raw.trim();
    let mut chars = raw.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::Other(format!("{raw:?} is not a Loki label name")));
    }
    Ok(raw)
}

/// A time as Grafana writes one (OBS-7): epoch milliseconds, RFC 3339,
/// `now`, or `now-15m`. Returned in nanoseconds, which is what Loki takes.
pub fn parse_time(raw: &str, now: DateTime<Utc>) -> Result<i64> {
    let raw = raw.trim();
    let bad = || {
        Error::Other(format!(
            "{raw:?} is not a time: use epoch milliseconds, RFC 3339, now, or now-15m (s, m, h, d, w)"
        ))
    };
    let at = if raw == "now" {
        now
    } else if let Some(ago) = raw.strip_prefix("now-") {
        // At the last character, not the last byte: `now-5µ` split inside
        // the µ, and the panic took the app and every agent down. Agents
        // write these times, and so do pasted links.
        let at = ago.char_indices().last().map_or(0, |(i, _)| i);
        let (n, unit) = ago.split_at(at);
        let n: i64 = n.parse().map_err(|_| bad())?;
        let span = match unit {
            "s" => Duration::try_seconds(n),
            "m" => Duration::try_minutes(n),
            "h" => Duration::try_hours(n),
            "d" => Duration::try_days(n),
            "w" => Duration::try_weeks(n),
            _ => None,
        }
        .ok_or_else(bad)?;
        now.checked_sub_signed(span).ok_or_else(bad)?
    } else if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
        let ms: i64 = raw.parse().map_err(|_| bad())?;
        Utc.timestamp_millis_opt(ms).single().ok_or_else(bad)?
    } else {
        DateTime::parse_from_rfc3339(raw).map_err(|_| bad())?.with_timezone(&Utc)
    };
    at.timestamp_nanos_opt().ok_or_else(bad)
}

/// What a Grafana Explore link asks for (OBS-3). Only links to `site`: the
/// datasource uid in a link to another Grafana would mean nothing here, or
/// another datasource that happens to share it.
pub fn parse_explore(link: &str, site: &str) -> Result<ExploreLink> {
    let url = reqwest::Url::parse(link.trim())
        .map_err(|_| Error::Other("the Explore link is not a web address".into()))?;
    let origin = |u: &reqwest::Url| (u.scheme().to_string(), u.host_str().map(str::to_lowercase), u.port_or_known_default());
    let site_url = reqwest::Url::parse(site).map_err(|_| Error::Other(format!("{site} is not a web address")))?;
    if origin(&url) != origin(&site_url) {
        return Err(Error::Other(format!(
            "the link is to {}, but the connected Grafana is {}; only its own links can be read",
            url.host_str().unwrap_or("another site"),
            site_url.host_str().unwrap_or(site),
        )));
    }
    let panes = url
        .query_pairs()
        .find(|(k, _)| k == "panes")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| Error::Other("the link has no Explore query (no panes= in it)".into()))?;
    let panes: Value = serde_json::from_str(&panes)
        .map_err(|_| Error::Other("the link's Explore query is not readable".into()))?;
    // Split view has two panes; the first is the one the link was made from.
    let pane = panes
        .as_object()
        .and_then(|o| o.values().next())
        .ok_or_else(|| Error::Other("the link's Explore query is empty".into()))?;
    let query = pane.get("queries").and_then(Value::as_array).and_then(|q| q.first());
    let expr = query
        .and_then(|q| q.get("expr"))
        .and_then(Value::as_str)
        .filter(|e| !e.trim().is_empty())
        .ok_or_else(|| Error::Other("the link's Explore query has no LogQL in it".into()))?;
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    Ok(ExploreLink {
        // The query's own datasource wins over the pane's: a pane on
        // "-- Mixed --" names no real one.
        datasource: text(query.and_then(|q| q.pointer("/datasource/uid"))).or_else(|| text(pane.get("datasource"))),
        expr: expr.to_string(),
        from: text(pane.pointer("/range/from")),
        to: text(pane.pointer("/range/to")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://acme.grafana.net";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0).single().unwrap_or_default()
    }

    #[test]
    fn an_explore_link_gives_its_datasource_query_and_range() {
        // Shaped like a link copied from Grafana Cloud's Explore, names made up.
        let link = "https://acme.grafana.net/explore?schemaVersion=1&panes=%7B%22or0%22:%7B%22datasource%22:%22acme-logs%22,%22queries%22:%5B%7B%22datasource%22:%7B%22type%22:%22loki%22,%22uid%22:%22acme-logs%22%7D,%22expr%22:%22%7Bservice_name%3D%5C%22checkout%5C%22%7D%20%7C%3D%20%5C%22failed%5C%22%22,%22refId%22:%22A%22%7D%5D,%22range%22:%7B%22from%22:%221791793920000%22,%22to%22:%221791798900000%22%7D%7D%7D";
        let got = parse_explore(link, SITE).expect("a link to the connected site reads");
        assert_eq!(
            got,
            ExploreLink {
                datasource: Some("acme-logs".into()),
                expr: r#"{service_name="checkout"} |= "failed""#.into(),
                from: Some("1791793920000".into()),
                to: Some("1791798900000".into()),
            }
        );
    }

    #[test]
    fn a_link_to_another_grafana_is_refused() {
        let link = "https://other.grafana.net/explore?panes=%7B%7D";
        let err = parse_explore(link, SITE).expect_err("another site's link is refused");
        assert!(err.to_string().contains("other.grafana.net"), "{err}");
        assert!(parse_explore("https://acme.grafana.net/explore", SITE).is_err(), "no query in it");
    }

    #[test]
    fn times_are_read_as_grafana_writes_them() {
        let noon = now().timestamp_nanos_opt().unwrap_or_default();
        assert_eq!(parse_time("now", now()).ok(), Some(noon));
        assert_eq!(parse_time("now-15m", now()).ok(), Some(noon - 15 * 60 * 1_000_000_000));
        assert_eq!(parse_time("now-2d", now()).ok(), Some(noon - 2 * 86_400 * 1_000_000_000));
        assert_eq!(parse_time("1791793920000", now()).ok(), Some(1_791_793_920_000_000_000));
        assert_eq!(parse_time("2026-10-12T12:00:00Z", now()).ok(), Some(noon));
        for bad in ["", "yesterday", "now-15", "now-1y", "now-1h/h", "12:00", "now-5µ", "now-1ч", "now-"] {
            assert!(parse_time(bad, now()).is_err(), "{bad:?} is not a time");
        }
    }

    #[test]
    fn nothing_from_outside_climbs_out_of_the_proxy_path() {
        assert!(uid_segment("acme-logs_1").is_ok());
        assert!(uid_segment("../../api/admin").is_err());
        assert!(uid_segment("a/b").is_err());
        assert!(uid_segment("").is_err());
        assert!(label_segment("service_name").is_ok());
        assert!(label_segment("1abc").is_err());
        assert!(label_segment("a/../b").is_err());
    }

    #[test]
    fn a_full_page_of_lines_says_where_the_next_one_ends() {
        // Loki's query_range answer, as the datasource proxy returns it.
        let body = json!({
            "status": "success",
            "data": {
                "resultType": "streams",
                "result": [
                    { "stream": { "service_name": "checkout", "level": "error" },
                      "values": [["1791798000000000000", "newest"], ["1791796000000000000", "oldest"]] },
                    { "stream": { "service_name": "checkout", "level": "info" },
                      "values": [["1791797000000000000", "middle"]] }
                ]
            }
        });
        let page = lines(&body, 3);
        let got: Vec<&str> = page["lines"].as_array().into_iter().flatten().filter_map(|l| l["line"].as_str()).collect();
        assert_eq!(got, ["newest", "middle", "oldest"], "newest first, across streams");
        assert_eq!(page["lines"][1]["stream"], 1);
        assert_eq!(page["streams"][1]["level"], "info");
        assert_eq!(page["more"], true);
        assert_eq!(page["next_to"], rfc3339(1_791_796_000_000_000_001));

        let short = lines(&body, 10);
        assert_eq!(short["more"], false, "fewer than asked for is everything");
        assert!(short["next_to"].is_null());
    }

    #[test]
    fn a_datasource_the_token_may_not_query_says_access_has_to_be_given_in_grafana() {
        let body = r#"{"message":"Access denied to datasource","traceID":"1"}"#;
        let path = Grafana::loki("acme-cloud-logs", "query_range").unwrap_or_default();
        let msg = forbidden(&path, body);
        assert!(msg.contains("Access denied to datasource"), "{msg}");
        assert!(msg.contains("grafana_datasources"), "{msg}");
        assert!(msg.contains("access to it in Grafana"), "{msg}");
        let listing = forbidden("/api/datasources", body);
        assert!(listing.contains("read and query datasources"), "{listing}");
    }

    #[test]
    fn a_long_line_is_cut_and_says_so() {
        let long = "x".repeat(MAX_LINE + 50);
        let cut = clip(&long);
        assert!(cut.starts_with(&"x".repeat(MAX_LINE)));
        assert!(cut.ends_with(&format!("[cut: {} characters]", MAX_LINE + 50)), "{}", &cut[MAX_LINE..]);
        assert_eq!(clip("short"), "short");
    }

    #[test]
    fn a_metric_query_goes_back_as_loki_sent_it() {
        let body = json!({ "data": { "resultType": "matrix", "result": [{ "metric": {}, "values": [[1, "2"]] }] } });
        let got = lines(&body, 10);
        assert_eq!(got["result_type"], "matrix");
        assert_eq!(got["result"][0]["values"][0][1], "2");
    }
}
