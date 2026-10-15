//! Grafana: connecting a site, and the tools agents read its logs with
//! (§23). The tools are defined here rather than in `mcp.rs`, as the
//! browser's are, and `mcp.rs` hands them their calls.

use chrono::Utc;
use serde_json::{Value, json};
use tauri::State;

use crate::config::GrafanaConfig;
use crate::error::{Error, Result};
use crate::integrations::grafana::{self, Datasource, Grafana, LogQuery};
use crate::mcp::{arg, str_prop, tool};
use crate::secrets;

use super::AppState;

pub(crate) fn grafana_client(state: &AppState) -> Result<(Grafana, GrafanaConfig)> {
    let cfg = state.config.read().grafana.ok_or(Error::NotConfigured("Grafana"))?;
    let token = secrets::get(secrets::GRAFANA)?.ok_or(Error::NotConfigured("Grafana"))?;
    Ok((Grafana::new(&cfg.base_url, &token)?, cfg))
}

/// Connect a Grafana site, answering how many Loki datasources the token
/// can see. Verified before anything is saved (OBS-1), so a token that
/// cannot read the site never looks like a working setup.
#[tauri::command]
pub async fn grafana_connect(state: State<'_, AppState>, base_url: String, token: String) -> Result<usize> {
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    let was = state.config.read().grafana.map(|g| g.base_url);
    let token = super::stored_or(secrets::GRAFANA, &token, "Grafana", was.as_deref(), &base_url)?;
    let loki = loki(Grafana::new(&base_url, &token)?.datasources().await?);
    secrets::set(secrets::GRAFANA, &token)?;
    let datasource = kept_choice(state.config.read().grafana, &base_url, &loki);
    state.config.update(|c| c.grafana = Some(GrafanaConfig { base_url, datasource }))?;
    Ok(loki.len())
}

/// The connected site's Loki datasources, to choose the one agents query
/// when they name none (OBS-5).
#[tauri::command]
pub async fn grafana_list_datasources(state: State<'_, AppState>) -> Result<Vec<Datasource>> {
    let (client, _) = grafana_client(&state)?;
    Ok(loki(client.datasources().await?))
}

/// Choose the Loki datasource agents query when they name none, or with
/// None go back to asking them to name one (OBS-5). Checked against the
/// site, so a uid that is not one of its Loki datasources is never saved.
#[tauri::command]
pub async fn grafana_set_datasource(state: State<'_, AppState>, uid: Option<String>) -> Result<()> {
    let (client, _) = grafana_client(&state)?;
    if let Some(uid) = &uid {
        if !loki(client.datasources().await?).iter().any(|d| &d.uid == uid) {
            return Err(Error::Other(format!("{uid} is not a Loki datasource of this Grafana")));
        }
    }
    state.config.update(|c| {
        if let Some(g) = c.grafana.as_mut() {
            g.datasource = uid;
        }
    })
}

fn loki(all: Vec<Datasource>) -> Vec<Datasource> {
    all.into_iter().filter(|d| d.kind == "loki").collect()
}

/// Reconnecting (a new token, say) keeps the datasource chosen, as long as
/// it is the same site and the site still has it.
fn kept_choice(was: Option<GrafanaConfig>, base_url: &str, loki: &[Datasource]) -> Option<String> {
    was.filter(|g| g.base_url == base_url)
        .and_then(|g| g.datasource)
        .filter(|uid| loki.iter().any(|d| &d.uid == uid))
}

pub fn grafana_owns(name: &str) -> bool {
    name.starts_with("grafana_")
}

fn time_props() -> Value {
    json!({
        "from": str_prop("Start: epoch milliseconds, RFC 3339, or now-1h (s, m, h, d, w). Default now-1h, or the link's"),
        "to": str_prop("End, as from. Default now, or the link's. To read further back, pass the next_to of the last answer"),
    })
}

fn with_times(mut props: Value) -> Value {
    if let (Some(p), Some(t)) = (props.as_object_mut(), time_props().as_object()) {
        p.extend(t.clone());
    }
    props
}

pub fn grafana_tools() -> Vec<Value> {
    vec![
        tool(
            "grafana_datasources",
            "The Loki datasources of the Grafana the user connected to Villain Layer: uid, \
             name, which is the site's default, and `chosen`: the one the user picked for \
             their services' logs, used when no `datasource` is given. Pass a uid as \
             `datasource` to the other grafana_ tools to read another.",
            json!({}),
            vec![],
        ),
        tool(
            "grafana_logs",
            "Read service logs from Grafana Loki, newest first. Give a LogQL `query`, or a \
             Grafana Explore `url` (a link from a ticket, a Slack thread or the user) and its \
             query, datasource and time range are used. Returns lines (each with its time and \
             the index of its stream), the streams' labels, and, when there are more lines \
             than `limit`, more: true and next_to to pass as `to` for the page before.",
            with_times(json!({
                "url": str_prop("A Grafana Explore link to the connected Grafana"),
                "query": str_prop("LogQL, e.g. {service_name=\"checkout\"} |= \"error\". Overrides the link's"),
                "datasource": str_prop("A Loki datasource uid from grafana_datasources. Overrides the link's"),
                "limit": { "type": "integer", "description": "Most lines to return, default 100, at most 1000" },
            })),
            vec![],
        ),
        tool(
            "grafana_labels",
            "A Loki datasource's label names, or with `label` that label's values. Use it to \
             find the right stream selector (service_name, environment, …) before querying.",
            with_times(json!({
                "datasource": str_prop("A Loki datasource uid from grafana_datasources"),
                "label": str_prop("A label name, to list its values"),
                "selector": str_prop("A stream selector narrowing the values, e.g. {environment=\"production\"}"),
            })),
            vec![],
        ),
    ]
}

/// The datasource named, the one the user chose, or the only Loki one
/// there is (OBS-5).
async fn datasource(client: &Grafana, cfg: &GrafanaConfig, named: Option<String>) -> Result<String> {
    if let Some(uid) = named.or_else(|| cfg.datasource.clone()) {
        return Ok(uid);
    }
    only(&loki(client.datasources().await?))
}

fn only(loki: &[Datasource]) -> Result<String> {
    match loki {
        [one] => Ok(one.uid.clone()),
        [] => Err(Error::Other("this Grafana has no Loki datasource the token can see".into())),
        many => Err(Error::Other(format!(
            "this Grafana has {} Loki datasources and the user has not chosen one in Settings → \
             Observability; name one as datasource: {}",
            many.len(),
            many.iter().map(|d| format!("{} ({})", d.uid, d.name)).collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// `from` and `to` in nanoseconds, the named ones over the link's (OBS-3),
/// the last hour over nothing (OBS-7).
fn range(args: &Value, link_from: Option<String>, link_to: Option<String>) -> Result<(i64, i64)> {
    let now = Utc::now();
    let from = arg(args, "from").map(str::to_string).or(link_from).unwrap_or_else(|| "now-1h".into());
    let to = arg(args, "to").map(str::to_string).or(link_to).unwrap_or_else(|| "now".into());
    let (from, to) = (grafana::parse_time(&from, now)?, grafana::parse_time(&to, now)?);
    if from >= to {
        return Err(Error::Other("from has to be before to".into()));
    }
    Ok((from, to))
}

pub async fn grafana_call(state: &AppState, name: &str, args: Value) -> Result<Value> {
    let (client, cfg) = grafana_client(state)?;
    match name {
        "grafana_datasources" => {
            let mut out = serde_json::to_value(loki(client.datasources().await?))?;
            for d in out.as_array_mut().into_iter().flatten() {
                let chosen = d.get("uid").and_then(Value::as_str).is_some_and(|u| cfg.datasource.as_deref() == Some(u));
                if let Some(o) = d.as_object_mut() {
                    o.insert("chosen".into(), json!(chosen));
                }
            }
            Ok(out)
        }

        "grafana_logs" => {
            let link = arg(&args, "url").map(|u| grafana::parse_explore(u, &cfg.base_url)).transpose()?.unwrap_or_default();
            let expr = arg(&args, "query").map(str::to_string).unwrap_or(link.expr);
            if expr.trim().is_empty() {
                return Err(Error::Other("give a LogQL query, or a Grafana Explore url".into()));
            }
            let (from_ns, to_ns) = range(&args, link.from, link.to)?;
            let uid = datasource(&client, &cfg, arg(&args, "datasource").map(str::to_string).or(link.datasource)).await?;
            // Clamped before the cast: a huge value would wrap.
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(grafana::DEFAULT_LIMIT as u64)
                .clamp(1, grafana::MAX_LIMIT as u64) as u32;
            let mut out = client.query_range(&uid, &LogQuery { expr: &expr, from_ns, to_ns, limit }).await?;
            if let Some(o) = out.as_object_mut() {
                o.insert("datasource".into(), json!(uid));
                o.insert("query".into(), json!(expr));
                o.insert("from".into(), json!(grafana::rfc3339(from_ns)));
                o.insert("to".into(), json!(grafana::rfc3339(to_ns)));
            }
            Ok(out)
        }

        "grafana_labels" => {
            let (from_ns, to_ns) = range(&args, None, None)?;
            let uid = datasource(&client, &cfg, arg(&args, "datasource").map(str::to_string)).await?;
            Ok(json!(client.labels(&uid, arg(&args, "label"), arg(&args, "selector"), from_ns, to_ns).await?))
        }

        other => Err(Error::NotFound(format!("tool {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(uid: &str) -> Datasource {
        Datasource { uid: uid.into(), name: uid.into(), kind: "loki".into(), is_default: false }
    }

    fn site(url: &str, chosen: Option<&str>) -> Option<GrafanaConfig> {
        Some(GrafanaConfig { base_url: url.into(), datasource: chosen.map(str::to_string) })
    }

    #[test]
    fn reconnecting_keeps_the_chosen_datasource_while_the_site_still_has_it() {
        let loki = [source("acme-logs"), source("acme-usage")];
        let was = site("https://acme.grafana.net", Some("acme-logs"));
        assert_eq!(kept_choice(was, "https://acme.grafana.net", &loki).as_deref(), Some("acme-logs"));
        let gone = site("https://acme.grafana.net", Some("acme-old"));
        assert_eq!(kept_choice(gone, "https://acme.grafana.net", &loki), None);
    }

    #[test]
    fn connecting_another_site_forgets_the_chosen_datasource() {
        let loki = [source("acme-logs")];
        let was = site("https://acme.grafana.net", Some("acme-logs"));
        assert_eq!(kept_choice(was, "https://other.grafana.net", &loki), None);
    }

    #[test]
    fn several_datasources_and_no_choice_say_where_to_choose_one() {
        assert_eq!(only(&[source("acme-logs")]).ok().as_deref(), Some("acme-logs"));
        let Err(e) = only(&[source("acme-logs"), source("acme-usage")]) else {
            panic!("two datasources and no choice should not pick one");
        };
        assert!(e.to_string().contains("Settings → Observability"), "{e}");
    }
}
