//! Logs kept in S3 (§23): setting sources up, and the tools agents read them
//! with (OBS-8 to OBS-13). The tools are defined here, as Grafana's are, and
//! `mcp.rs` hands them their calls. Every AWS call is the `aws` CLI, so all
//! of it runs off the async runtime.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, State};

use crate::config::{AthenaTable, LogSource};
use crate::error::{Error, Result};
use crate::integrations::aws::{self, Aws, Profile, Table};
use crate::mcp::{arg, str_prop, tool};

use super::AppState;

/// What the tab needs before a source can be added: the CLI, and the
/// profiles to pick from (OBS-8, OBS-9).
#[derive(Debug, Serialize)]
pub struct AwsSetup {
    /// Where the `aws` CLI is; None when the PATH has none.
    pub cli: Option<String>,
    pub profiles: Vec<Profile>,
}

#[tauri::command]
pub async fn aws_setup() -> Result<AwsSetup> {
    super::off_runtime(|| AwsSetup { cli: aws::cli(), profiles: aws::profiles() }).await
}

/// What "Find" found for a bucket and prefix (OBS-9).
#[derive(Debug, Serialize)]
pub struct Found {
    pub folders: Vec<String>,
    pub files: usize,
    /// Listed only in part: there is more under the prefix.
    pub more: bool,
    pub tables: Vec<Table>,
    /// Those the profile may list, or when it may not, those named after a
    /// table's database that it can see. None when there are neither: the
    /// user types one.
    pub workgroups: Option<Vec<String>>,
}

/// What is under a bucket's prefix, and the Athena tables over the bucket.
#[tauri::command]
pub async fn s3_log_source_find(profile: String, region: Option<String>, bucket: String, prefix: String) -> Result<Found> {
    super::off_runtime(move || find(&profile, region.as_deref(), &bucket, &prefix)).await?
}

pub(crate) fn find(profile: &str, region: Option<&str>, bucket: &str, prefix: &str) -> Result<Found> {
    let aws = Aws::new(profile, region)?;
    let prefix = folder(prefix);
    let listed = aws.list(bucket.trim(), &prefix, None)?;
    // Reading the bucket is what was asked; tables are a help, and a
    // profile that may not read the catalog still adds the source.
    let tables = aws.tables_over(bucket.trim()).unwrap_or_default();
    // A team's workgroup is often named after its database, and a
    // profile allowed to use one is not always allowed to list them.
    let workgroups = aws.workgroups().or_else(|| {
        let seen: Vec<String> = named_like(&tables).into_iter().filter(|w| aws.workgroup_exists(w)).collect();
        (!seen.is_empty()).then_some(seen)
    });
    Ok(Found { files: listed.files.len(), more: listed.next.is_some(), folders: listed.folders, tables, workgroups })
}

/// Workgroups named after the tables' databases: `logs_db` as itself and as
/// `logs-db`.
fn named_like(tables: &[Table]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for t in tables {
        for n in [t.database.replace('_', "-"), t.database.clone()] {
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    names
}

/// Add a source, or change the one with its id. Saved only once the
/// profile has listed the bucket (OBS-9).
#[tauri::command]
pub async fn s3_log_source_save(app: AppHandle, source: LogSource) -> Result<LogSource> {
    super::blocking(app, move |state| save_source(state, source)).await
}

pub(crate) fn save_source(state: &AppState, mut s: LogSource) -> Result<LogSource> {
    let profiles = aws::profiles();
    checked_source(&mut s, &state.config.read().log_sources, &profiles)?;
    Aws::new(&s.profile, s.region.as_deref())?.list(&s.bucket, &s.prefix, None)?;
    let saved = s.clone();
    state.config.update(move |c| match c.log_sources.iter_mut().find(|x| x.id == s.id) {
        Some(x) => *x = s,
        None => c.log_sources.push(s),
    })?;
    Ok(saved)
}

/// The source as it will be kept, or why it cannot be.
fn checked_source<'a>(s: &'a mut LogSource, others: &[LogSource], profiles: &[Profile]) -> Result<&'a LogSource> {
    s.name = s.name.trim().to_string();
    s.bucket = s.bucket.trim().to_string();
    s.prefix = folder(&s.prefix);
    s.region = s.region.take().map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
    if s.id.is_empty() {
        s.id = uuid::Uuid::new_v4().to_string();
    }
    if s.name.is_empty() {
        return Err(Error::Other("give the source a name: agents ask for it by name".into()));
    }
    if others.iter().any(|o| o.id != s.id && o.name.eq_ignore_ascii_case(&s.name)) {
        return Err(Error::Other(format!("there is already a source called {}", s.name)));
    }
    if !profiles.iter().any(|p| p.name == s.profile) {
        return Err(Error::Other(format!("there is no AWS profile {:?} in ~/.aws/config", s.profile)));
    }
    aws::check_bucket(&s.bucket)?;
    if let Some(a) = &mut s.athena {
        a.workgroup = a.workgroup.trim().to_string();
        a.database = a.database.trim().to_string();
        a.table = a.table.trim().to_string();
        if a.database.is_empty() && a.table.is_empty() {
            s.athena = None;
        } else if a.workgroup.is_empty() || a.database.is_empty() || a.table.is_empty() {
            return Err(Error::Other("an Athena table needs its workgroup, database and table".into()));
        }
    }
    Ok(s)
}

#[tauri::command]
pub fn s3_log_source_remove(state: State<AppState>, id: String) -> Result<()> {
    state.config.update(|c| c.log_sources.retain(|s| s.id != id))
}

/// A prefix as kept: no leading slash, and a folder, so `logs` does not
/// also take in `logs-old/` (OBS-10).
fn folder(prefix: &str) -> String {
    let p = prefix.trim().trim_start_matches('/');
    if p.is_empty() || p.ends_with('/') { p.to_string() } else { format!("{p}/") }
}

// ------------------------------------------------------- what queries cost

/// The data a source's queries may scan in a day unless the user sets
/// another limit (OBS-14): at $5 a TB, five cents.
pub const DEFAULT_DAILY_GB: u64 = 10;

/// What each source's queries scanned today, by source id. Kept in memory:
/// counted from when the app started that day.
struct Scanned {
    day: Option<chrono::NaiveDate>,
    by_source: Vec<(String, u64)>,
}

static SCANNED: Mutex<Scanned> = Mutex::new(Scanned { day: None, by_source: Vec::new() });

/// Today's scans, by source id, a new day starting from nothing.
pub fn scanned_today() -> HashMap<String, u64> {
    let today = chrono::Local::now().date_naive();
    let mut s = SCANNED.lock();
    if s.day != Some(today) {
        *s = Scanned { day: Some(today), by_source: Vec::new() };
    }
    s.by_source.iter().cloned().collect()
}

fn add_scanned(source: &str, bytes: u64) {
    scanned_today();
    let mut s = SCANNED.lock();
    match s.by_source.iter_mut().find(|(id, _)| id == source) {
        Some((_, n)) => *n += bytes,
        None => s.by_source.push((source.to_string(), bytes)),
    }
}

/// The source's daily limit in bytes, and refused once it is reached.
fn within_limit(source: &LogSource, a: &AthenaTable) -> Result<(u64, u64)> {
    let gb = a.daily_limit_gb.unwrap_or(DEFAULT_DAILY_GB);
    let limit = gb.saturating_mul(1 << 30);
    let done = scanned_today().get(&source.id).copied().unwrap_or(0);
    if done >= limit {
        return Err(Error::Other(format!(
            "{}'s queries have scanned {} today, its daily limit of {gb} GB: no more until tomorrow, unless the user raises the limit in Settings → Observability",
            source.name,
            aws::bytes(done)
        )));
    }
    Ok((done, limit))
}

// ------------------------------------------------------------------ tools

pub fn s3_logs_owns(name: &str) -> bool {
    name.starts_with("s3_logs_")
}

fn source_prop() -> Value {
    str_prop("The source's name, from s3_logs_sources. May be left out when there is only one")
}

pub fn s3_logs_tools() -> Vec<Value> {
    vec![
        tool(
            "s3_logs_sources",
            "The log sources in S3 the user set up in Layer: each one's name, where \
             it is (s3://bucket/prefix), and, when Athena has a table over it, the table's \
             columns and partition keys. Start here, then browse with s3_logs_list and \
             s3_logs_read, or query with s3_logs_query.",
            json!({}),
            vec![],
        ),
        tool(
            "s3_logs_list",
            "The folders and files under a path in a log source, at most 200 a call; pass \
             `next` from the answer for the rest. Paths are inside the source's prefix.",
            json!({
                "source": source_prop(),
                "path": str_prop("A folder in the source, e.g. checkout-service/payments/2026/10/12/. Default: the source's top"),
                "next": str_prop("The next of the last answer, to carry on"),
            }),
            vec![],
        ),
        tool(
            "s3_logs_read",
            "A file's text from a log source: gzip is unpacked, at most 8 MB is downloaded and \
             1 MB of text returned. With `contains`, only the lines containing it (at most 500): \
             use it to find one request id or trace id in a large file.",
            json!({
                "source": source_prop(),
                "key": str_prop("The file's key, as s3_logs_list gives it"),
                "contains": str_prop("Return only the lines containing this text"),
            }),
            vec!["key"],
        ),
        tool(
            "s3_logs_query",
            "Run one read-only SQL query (SELECT or WITH) with Athena over a log source that \
             has a table: the rows (at most `limit`), whether there were more, the data Athena \
             scanned and what it cost. Athena is paid for by the data it reads, so the WHERE \
             must filter on the table's partition keys (see s3_logs_sources), a time one \
             included (year, month, day, hour): narrow to one service and one day or hour, or \
             the query is refused. Each source may scan only so much a day. Prefer a narrow \
             query, and s3_logs_read with `contains` on one file when you know where to look. \
             Quote a column whose name starts with @ in double quotes.",
            json!({
                "source": source_prop(),
                "sql": str_prop("The query, e.g. SELECT \"@timestamp\", http_uri FROM all_service_request_logs WHERE service = 'checkout' AND day = '12' LIMIT 20"),
                "limit": { "type": "integer", "description": "Most rows to return, default 100, at most 1000" },
            }),
            vec!["sql"],
        ),
    ]
}

/// The source an agent named, by name or id, or the only one there is.
fn source_named(sources: &[LogSource], named: Option<&str>) -> Result<LogSource> {
    let names = || sources.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
    match (named, sources) {
        (_, []) => Err(Error::Other("no S3 log source is set up: the user adds one in Settings → Observability".into())),
        (None, [one]) => Ok(one.clone()),
        (None, _) => Err(Error::Other(format!("name a source: {}", names()))),
        (Some(n), _) => sources
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(n.trim()) || s.id == n)
            .cloned()
            .ok_or_else(|| Error::Other(format!("there is no source {n}: there are {}", names()))),
    }
}

/// A path an agent gave, as a key in the source: relative to its prefix, or
/// already under it. An `s3://` address of another bucket, or anything
/// outside the prefix, is refused (OBS-10).
fn inside(source: &LogSource, path: &str) -> Result<String> {
    let path = path.trim();
    let key = match path.strip_prefix("s3://") {
        Some(rest) => {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            if bucket != source.bucket {
                return Err(Error::Other(format!("{path} is not in this source's bucket, {}", source.bucket)));
            }
            key.to_string()
        }
        None => path.trim_start_matches('/').to_string(),
    };
    if key.starts_with(&source.prefix) {
        Ok(key)
    } else if path.starts_with("s3://") {
        Err(Error::Other(format!("{path} is outside this source: it reads only s3://{}/{}", source.bucket, source.prefix)))
    } else {
        Ok(format!("{}{key}", source.prefix))
    }
}

/// Answer one of the tools. Only the config is read here; the CLI runs off
/// the async runtime.
pub async fn s3_logs_call(state: &AppState, name: &str, args: Value) -> Result<Value> {
    let sources = state.config.read().log_sources;
    let name = name.to_string();
    super::off_runtime(move || call(&sources, &name, &args)).await?
}

fn call(sources: &[LogSource], name: &str, args: &Value) -> Result<Value> {
    if name == "s3_logs_sources" {
        return Ok(json!(sources.iter().map(describe).collect::<Vec<_>>()));
    }
    let source = source_named(sources, arg(args, "source"))?;
    let aws = || Aws::new(&source.profile, source.region.as_deref());
    match name {
        "s3_logs_list" => {
            let path = inside(&source, arg(args, "path").unwrap_or_default())?;
            let mut out = json!(aws()?.list(&source.bucket, &path, arg(args, "next"))?);
            out["path"] = json!(path);
            Ok(out)
        }
        "s3_logs_read" => {
            let key = inside(&source, arg(args, "key").unwrap_or_default())?;
            let mut out = json!(aws()?.read(&source.bucket, &key, arg(args, "contains"))?);
            out["key"] = json!(key);
            Ok(out)
        }
        "s3_logs_query" => {
            let a = source.athena.as_ref().ok_or_else(|| {
                Error::Other(format!("{} has no Athena table to query: browse it with s3_logs_list and s3_logs_read", source.name))
            })?;
            let sql = aws::read_only(arg(args, "sql").unwrap_or_default())?;
            let (done, daily) = within_limit(&source, a)?;
            let aws = aws()?;
            aws::narrowed(&sql, &aws.table(&a.database, &a.table)?.partition_keys)?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(aws::DEFAULT_ROWS as u64).clamp(1, aws::MAX_ROWS as u64) as u32;
            let rows = aws.query(&a.workgroup, &a.database, &sql, limit)?;
            add_scanned(&source.id, rows.scanned_bytes);
            let mut out = json!(rows);
            out["scanned_today"] = json!(aws::bytes(done + rows.scanned_bytes));
            out["daily_limit"] = json!(aws::bytes(daily));
            Ok(out)
        }
        other => Err(Error::NotFound(format!("tool {other}"))),
    }
}

/// A source as agents are told of it, its table's columns asked of Glue.
fn describe(s: &LogSource) -> Value {
    let table = |a: &AthenaTable| -> Value {
        let found = Aws::new(&s.profile, s.region.as_deref()).and_then(|aws| aws.table(&a.database, &a.table));
        match found {
            Ok(t) => json!({ "database": a.database, "table": a.table, "workgroup": a.workgroup,
                             "location": t.location, "partition_keys": t.partition_keys, "columns": t.columns }),
            Err(e) => json!({ "database": a.database, "table": a.table, "workgroup": a.workgroup, "error": e.to_string() }),
        }
    };
    json!({
        "name": s.name,
        "where": format!("s3://{}/{}", s.bucket, s.prefix),
        "athena": s.athena.as_ref().map(table),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(prefix: &str) -> LogSource {
        LogSource {
            id: "s1".into(),
            name: "Production request logs".into(),
            profile: "acme-prod".into(),
            bucket: "prod-logs".into(),
            prefix: prefix.into(),
            ..Default::default()
        }
    }

    #[test]
    fn an_agent_reads_only_inside_its_sources_bucket_and_prefix() {
        let s = source("requests/");
        assert_eq!(inside(&s, "").unwrap(), "requests/");
        assert_eq!(inside(&s, "orders-service/2026/").unwrap(), "requests/orders-service/2026/");
        assert_eq!(inside(&s, "/requests/orders-service/").unwrap(), "requests/orders-service/");
        assert_eq!(inside(&s, "s3://prod-logs/requests/a.gz").unwrap(), "requests/a.gz");
        assert!(inside(&s, "s3://prod-logs/billing/a.gz").is_err(), "outside the prefix");
        assert!(inside(&s, "s3://other-bucket/requests/a.gz").is_err(), "another bucket");
        assert_eq!(inside(&source(""), "anything/").unwrap(), "anything/");
    }

    #[test]
    fn workgroups_are_looked_for_by_the_names_of_the_tables_databases() {
        let t = |db: &str| Table { database: db.into(), table: "t".into(), location: String::new(), columns: vec![], partition_keys: vec![] };
        assert_eq!(named_like(&[t("ci_request_logs"), t("ci_request_logs"), t("plain")]), ["ci-request-logs", "ci_request_logs", "plain"]);
        assert!(named_like(&[]).is_empty());
    }

    #[test]
    fn a_prefix_is_kept_as_a_folder_so_it_takes_in_nothing_beside_it() {
        assert_eq!(folder("logs"), "logs/");
        assert_eq!(folder("/logs/"), "logs/");
        assert_eq!(folder(" "), "");
        assert!(inside(&source(&folder("logs")), "s3://prod-logs/logs-old/x").is_err());
    }

    #[test]
    fn a_source_is_named_by_its_name_or_id_or_is_the_only_one() {
        let one = vec![source("")];
        assert_eq!(source_named(&one, None).unwrap().id, "s1");
        assert_eq!(source_named(&one, Some("production REQUEST logs")).unwrap().id, "s1");
        let mut two = one.clone();
        two.push(LogSource { id: "s2".into(), name: "CI request logs".into(), ..source("") });
        assert!(source_named(&two, None).unwrap_err().to_string().contains("CI request logs"));
        assert_eq!(source_named(&two, Some("s2")).unwrap().name, "CI request logs");
        assert!(source_named(&[], None).unwrap_err().to_string().contains("Settings → Observability"));
    }

    #[test]
    fn a_source_is_kept_only_with_a_name_a_known_profile_a_bucket_and_a_whole_table() {
        let profiles = vec![Profile { name: "acme-prod".into(), region: Some("eu-central-1".into()) }];
        let mut s = LogSource { id: String::new(), prefix: "requests".into(), region: Some(" ".into()), ..source("") };
        let kept = checked_source(&mut s, &[], &profiles).unwrap();
        assert!(!kept.id.is_empty(), "a new source gets an id");
        assert_eq!((kept.prefix.as_str(), kept.region.as_ref()), ("requests/", None));

        let taken = vec![LogSource { id: "other".into(), ..source("") }];
        assert!(checked_source(&mut source(""), &taken, &profiles).is_err(), "two sources by one name");
        assert!(checked_source(&mut LogSource { profile: "nope".into(), ..source("") }, &[], &profiles).is_err());
        assert!(checked_source(&mut LogSource { bucket: "--endpoint-url=x".into(), ..source("") }, &[], &profiles).is_err());
        let half = AthenaTable { workgroup: String::new(), database: "logs".into(), table: "all".into(), daily_limit_gb: None };
        assert!(checked_source(&mut LogSource { athena: Some(half), ..source("") }, &[], &profiles).is_err());
        let blank = AthenaTable::default();
        let mut s = LogSource { athena: Some(blank), ..source("") };
        assert_eq!(checked_source(&mut s, &[], &profiles).unwrap().athena, None, "an empty table is none");
    }

    #[test]
    fn a_source_past_its_daily_limit_is_refused_until_the_next_day() {
        let s = LogSource { id: format!("limit-{}", uuid::Uuid::new_v4()), ..source("") };
        let one_gb = AthenaTable { workgroup: "w".into(), database: "d".into(), table: "t".into(), daily_limit_gb: Some(1) };
        assert_eq!(within_limit(&s, &one_gb).unwrap(), (0, 1 << 30));
        add_scanned(&s.id, 600 << 20);
        add_scanned(&s.id, 600 << 20);
        let said = within_limit(&s, &one_gb).unwrap_err().to_string();
        assert!(said.contains("1.2 GB today") && said.contains("daily limit of 1 GB"), "{said}");
        let default = AthenaTable { daily_limit_gb: None, ..one_gb };
        assert!(within_limit(&s, &default).is_ok(), "the default is {DEFAULT_DAILY_GB} GB");
        SCANNED.lock().day = chrono::Local::now().date_naive().pred_opt();
        assert_eq!(scanned_today().get(&s.id), None, "a new day starts from nothing");
    }

    #[test]
    fn a_query_over_a_source_with_no_table_says_how_to_read_it_instead() {
        let said = call(&[source("")], "s3_logs_query", &json!({ "sql": "SELECT 1" })).unwrap_err().to_string();
        assert!(said.contains("no Athena table") && said.contains("s3_logs_read"), "{said}");
    }

    /// The tools as an agent calls them, against a real source:
    /// `VILLAIN_S3="profile bucket database workgroup table" cargo test --lib real_source -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_source_answers_its_tools_and_refuses_a_query_that_is_not_narrowed() {
        let Ok(spec) = std::env::var("VILLAIN_S3") else { return };
        let p: Vec<&str> = spec.split_whitespace().collect();
        let s = LogSource {
            id: "real".into(),
            name: "Real".into(),
            profile: p[0].into(),
            bucket: p[1].into(),
            athena: Some(AthenaTable { database: p[2].into(), workgroup: p[3].into(), table: p[4].into(), daily_limit_gb: Some(1) }),
            ..Default::default()
        };
        let all = [s];
        let described = call(&all, "s3_logs_sources", &json!({})).unwrap();
        eprintln!("sources: {}", described.to_string().chars().take(600).collect::<String>());
        let listed = call(&all, "s3_logs_list", &json!({})).unwrap();
        eprintln!("list: {}", listed["folders"]);
        let wide = format!("SELECT count(*) FROM {} WHERE http_status = '500'", p[4]);
        let refused = call(&all, "s3_logs_query", &json!({ "sql": wide })).unwrap_err().to_string();
        eprintln!("refused: {refused}");
        assert!(refused.contains("narrow"), "{refused}");
        let narrow = format!("SELECT count(*) AS n FROM {} WHERE year = '2026' AND month = '10' AND day = '12' AND hour = '09'", p[4]);
        let found = find(p[0], None, p[1], "").unwrap();
        eprintln!("find: {} folders, tables {:?}, workgroups {:?}", found.folders.len(), found.tables.iter().map(|t| &t.table).collect::<Vec<_>>(), found.workgroups);
        let answered = call(&all, "s3_logs_query", &json!({ "sql": narrow })).unwrap();
        eprintln!("answered: rows {} scanned {} cost {} today {} of {}", answered["rows"], answered["data_scanned"], answered["cost"], answered["scanned_today"], answered["daily_limit"]);
    }
}
