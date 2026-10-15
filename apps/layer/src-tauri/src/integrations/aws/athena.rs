//! Athena, over a log source's files (OBS-12, OBS-14): queries, their
//! results and cost, and the Glue tables to query. Split from `aws.rs` at
//! its size ceiling; the CLI it runs is that file's.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::{bytes, check_bucket, Aws};
use crate::error::{Error, Result};

/// How long a query may run before it is stopped (OBS-12).
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_ROWS: u32 = 100;
/// What Athena charges for the data a query reads: $5 a TB (OBS-14).
const USD_PER_TB: f64 = 5.0;
/// Athena bills at least this much for any query.
const MIN_BILLED: u64 = 10 * 1024 * 1024;
pub const MAX_ROWS: u32 = 1000;

impl Aws {
    /// Run one read-only statement and read its first `limit` rows (OBS-12).
    pub fn query(&self, workgroup: &str, database: &str, sql: &str, limit: u32) -> Result<Rows> {
        let sql = read_only(sql)?;
        check_name(database, "database")?;
        let started = self.json(&[
            "athena".into(),
            "start-query-execution".into(),
            format!("--work-group={workgroup}"),
            format!("--query-execution-context=Database={database}"),
            format!("--query-string={sql}"),
        ])?;
        let id = started
            .get("QueryExecutionId")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Other("Athena did not start the query".into()))?
            .to_string();
        let deadline = Instant::now() + QUERY_TIMEOUT;
        let execution = loop {
            let e = self.json(&["athena".into(), "get-query-execution".into(), format!("--query-execution-id={id}")])?;
            let state = e.pointer("/QueryExecution/Status/State").and_then(Value::as_str).unwrap_or_default();
            match state {
                "SUCCEEDED" => break e,
                "FAILED" | "CANCELLED" => {
                    let why = e.pointer("/QueryExecution/Status/StateChangeReason").and_then(Value::as_str).unwrap_or(state);
                    return Err(Error::Other(format!("the query failed: {why}")));
                }
                _ if Instant::now() >= deadline => {
                    let _ = self.json(&["athena".into(), "stop-query-execution".into(), format!("--query-execution-id={id}")]);
                    return Err(Error::Other(format!(
                        "the query was stopped after {} seconds: filter on the table's partition columns so it reads less",
                        QUERY_TIMEOUT.as_secs()
                    )));
                }
                _ => std::thread::sleep(Duration::from_millis(500)),
            }
        };
        let limit = limit.clamp(1, MAX_ROWS);
        // One more than asked: the first row of a SELECT's results is its header.
        let page = self.json(&[
            "athena".into(),
            "get-query-results".into(),
            format!("--query-execution-id={id}"),
            format!("--max-items={}", limit + 1),
        ])?;
        let scanned = execution.pointer("/QueryExecution/Statistics/DataScannedInBytes").and_then(Value::as_u64).unwrap_or(0);
        Ok(rows(&page, limit, scanned, id))
    }

    /// The tables of the Glue catalog whose data is in `bucket` (OBS-9).
    /// A database the profile may not read is passed over.
    pub fn tables_over(&self, bucket: &str) -> Result<Vec<Table>> {
        check_bucket(bucket)?;
        let dbs = self.json(&["glue".into(), "get-databases".into()])?;
        let mut found = Vec::new();
        for db in dbs.get("DatabaseList").and_then(Value::as_array).into_iter().flatten() {
            let Some(name) = db.get("Name").and_then(Value::as_str) else { continue };
            let Ok(tables) = self.json(&["glue".into(), "get-tables".into(), format!("--database-name={name}")]) else { continue };
            found.extend(tables_in(&tables, name, bucket));
        }
        Ok(found)
    }

    /// One table's columns and partition keys, for agents to write a query by.
    pub fn table(&self, database: &str, table: &str) -> Result<Table> {
        check_name(database, "database")?;
        check_name(table, "table")?;
        let t = self.json(&["glue".into(), "get-table".into(), format!("--database-name={database}"), format!("--name={table}")])?;
        table_of(t.get("Table").unwrap_or(&Value::Null), database).ok_or_else(|| Error::NotFound(format!("table {database}.{table}")))
    }

    /// Whether the profile can see a workgroup by this name. A profile that
    /// may not list workgroups may still look one up.
    pub fn workgroup_exists(&self, name: &str) -> bool {
        check_name(name, "workgroup").is_ok()
            && self.json(&["athena".into(), "get-work-group".into(), format!("--work-group={name}")]).is_ok()
    }

    /// The Athena workgroups the profile may list; None when it may not.
    pub fn workgroups(&self) -> Option<Vec<String>> {
        let w = self.json(&["athena".into(), "list-work-groups".into()]).ok()?;
        Some(
            w.get("WorkGroups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|g| g.get("Name").and_then(Value::as_str).map(str::to_string))
                .collect(),
        )
    }
}

/// A Glue database or table name.
fn check_name<'a>(name: &'a str, what: &str) -> Result<&'a str> {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        Ok(name)
    } else {
        Err(Error::Other(format!("{name:?} is not an Athena {what} name")))
    }
}

/// One read-only statement (OBS-12): `SELECT` or `WITH`, after any leading
/// comments, with no second statement after a `;`. A `;` inside a string is
/// refused too, which is the price of not parsing SQL.
pub fn read_only(sql: &str) -> Result<String> {
    let body = sql.trim().trim_end_matches(';').trim();
    let mut rest = body;
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("--") {
            rest = r.split_once('\n').map_or("", |(_, after)| after);
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map_or("", |(_, after)| after);
        } else {
            break;
        }
    }
    let first: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_uppercase();
    if body.contains(';') {
        return Err(Error::Other("one statement at a time: remove the ;".into()));
    }
    if first != "SELECT" && first != "WITH" {
        return Err(Error::Other("only a SELECT (or WITH … SELECT) is run: the app reads logs, it never changes them".into()));
    }
    Ok(body.to_string())
}

/// A query narrowed to the table's partitions (OBS-14): its `WHERE` names
/// one of them, and one of its time keys when it has any. Without that
/// Athena reads every file, and is paid for every byte. A table with no
/// partitions cannot be narrowed, and is not held to it.
pub fn narrowed(sql: &str, keys: &[Column]) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let lower = sql.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
    let after_where: &[&str] = words.iter().position(|w| *w == "where").map_or(&[], |at| &words[at + 1..]);
    let named = |k: &Column| after_where.contains(&k.name.to_lowercase().as_str());
    let time: Vec<&Column> = keys.iter().filter(|k| is_time_key(&k.name)).collect();
    let list = |ks: &[&Column]| ks.iter().map(|k| k.name.as_str()).collect::<Vec<_>>().join(", ");
    if !time.is_empty() && !time.iter().any(|k| named(k)) {
        return Err(Error::Other(format!(
            "narrow the query to a time first: filter in its WHERE on {} (Athena is paid for by the data it reads, and without them it reads every file)",
            list(&time)
        )));
    }
    if !keys.iter().any(named) {
        let all: Vec<&Column> = keys.iter().collect();
        return Err(Error::Other(format!(
            "narrow the query to the table's partitions: filter in its WHERE on {} (Athena is paid for by the data it reads)",
            list(&all)
        )));
    }
    Ok(())
}

fn is_time_key(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "year" | "month" | "day" | "date" | "dt" | "ds" | "hour")
}

/// What a query that read `scanned` bytes cost, as Athena bills it.
pub fn cost(scanned: u64) -> String {
    let usd = scanned.max(MIN_BILLED) as f64 / (1u64 << 40) as f64 * USD_PER_TB;
    match usd {
        u if u < 0.0001 => "under $0.0001".into(),
        u if u < 0.01 => format!("${u:.4}"),
        u => format!("${u:.2}"),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    /// There were more rows than `limit`.
    pub more: bool,
    /// What Athena read for it, e.g. "12.4 MB".
    pub data_scanned: String,
    pub scanned_bytes: u64,
    /// What it cost at $5 a TB, e.g. "$0.0001".
    pub cost: String,
    pub query_id: String,
}

fn rows(page: &Value, limit: u32, scanned: u64, query_id: String) -> Rows {
    let columns: Vec<String> = page
        .pointer("/ResultSet/ResultSetMetadata/ColumnInfo")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("Name").and_then(Value::as_str).map(str::to_string))
        .collect();
    let mut all: Vec<Vec<Option<String>>> = page
        .pointer("/ResultSet/Rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| {
            r.get("Data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|d| d.get("VarCharValue").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .collect();
    if all.first().is_some_and(|r| r.iter().map(|c| c.as_deref().unwrap_or_default()).eq(columns.iter().map(String::as_str))) {
        all.remove(0);
    }
    let more = all.len() > limit as usize || page.get("NextToken").is_some();
    all.truncate(limit as usize);
    Rows { columns, rows: all, more, data_scanned: bytes(scanned), scanned_bytes: scanned, cost: cost(scanned), query_id }
}

/// An Athena table: where its data is, and what it is made of.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Table {
    pub database: String,
    pub table: String,
    pub location: String,
    pub columns: Vec<Column>,
    /// The columns its data is split by. A query that filters on them reads
    /// only that part, which is what keeps it quick and cheap.
    pub partition_keys: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Column {
    pub name: String,
    pub kind: String,
}

fn table_of(t: &Value, database: &str) -> Option<Table> {
    let cols = |v: Option<&Value>| -> Vec<Column> {
        v.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(Column {
                    name: c.get("Name")?.as_str()?.to_string(),
                    kind: c.get("Type").and_then(Value::as_str).unwrap_or_default().to_string(),
                })
            })
            .collect()
    };
    // A table with partition projection says where its data is in a template;
    // its location is then only the bucket's root.
    let location = t
        .pointer("/Parameters/storage.location.template")
        .or_else(|| t.pointer("/StorageDescriptor/Location"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(Table {
        database: database.to_string(),
        table: t.get("Name")?.as_str()?.to_string(),
        location,
        columns: cols(t.pointer("/StorageDescriptor/Columns")),
        partition_keys: cols(t.get("PartitionKeys")),
    })
}

fn tables_in(tables: &Value, database: &str, bucket: &str) -> Vec<Table> {
    let root = format!("s3://{bucket}");
    tables
        .get("TableList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| table_of(t, database))
        .filter(|t| t.location == root || t.location.starts_with(&format!("{root}/")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_one_read_only_statement_is_run() {
        assert_eq!(read_only(" SELECT 1; ").unwrap(), "SELECT 1");
        assert!(read_only("with x as (select 1) select * from x").is_ok());
        assert!(read_only("-- today's\n/* bookings */ select 1").is_ok());
        for bad in [
            "DROP TABLE logs",
            "INSERT INTO t SELECT 1",
            "CREATE TABLE t AS SELECT 1",
            "UNLOAD (SELECT 1) TO 's3://x/'",
            "SELECT 1; DROP TABLE logs",
            "-- select\nDELETE FROM t",
            "",
        ] {
            assert!(read_only(bad).is_err(), "{bad}");
        }
    }

    fn keys(names: &[&str]) -> Vec<Column> {
        names.iter().map(|n| Column { name: n.to_string(), kind: "string".into() }).collect()
    }

    #[test]
    fn a_query_is_run_only_narrowed_to_a_time_and_a_partition() {
        let k = keys(&["service", "source", "year", "month", "day", "hour"]);
        assert!(narrowed("SELECT * FROM logs WHERE service = 'checkout' AND day = '12' LIMIT 10", &k).is_ok());
        assert!(narrowed("select count(*) from logs where \"year\" = '2026'", &k).is_ok());
        let no_time = narrowed("SELECT * FROM logs WHERE service = 'checkout'", &k).unwrap_err().to_string();
        assert!(no_time.contains("year, month, day, hour"), "{no_time}");
        assert!(narrowed("SELECT * FROM logs", &k).is_err(), "no WHERE at all");
        assert!(narrowed("SELECT day FROM logs WHERE http_status = '500'", &k).is_err(), "named, but not in the WHERE");
        let untimed = keys(&["service"]);
        assert!(narrowed("SELECT * FROM t WHERE http_status = '500'", &untimed).is_err());
        assert!(narrowed("SELECT * FROM t WHERE service = 'x'", &untimed).is_ok());
        assert!(narrowed("SELECT * FROM t", &[]).is_ok(), "a table with no partitions cannot be narrowed");
    }

    #[test]
    fn a_query_costs_five_dollars_a_terabyte_and_at_least_ten_megabytes() {
        assert_eq!(cost(1u64 << 40), "$5.00");
        assert_eq!(cost(100u64 << 30), "$0.49");
        assert_eq!(cost(0), cost(10 * 1024 * 1024));
        assert_eq!(cost(2 * 1024 * 1024), "under $0.0001");
        assert_eq!(cost(1u64 << 30), "$0.0049");
    }

    #[test]
    fn query_results_lose_their_header_row_and_say_when_there_is_more() {
        let page = json!({
            "ResultSet": {
                "ResultSetMetadata": { "ColumnInfo": [{ "Name": "http_uri" }, { "Name": "http_status" }] },
                "Rows": [
                    { "Data": [{ "VarCharValue": "http_uri" }, { "VarCharValue": "http_status" }] },
                    { "Data": [{ "VarCharValue": "/book" }, { "VarCharValue": "500" }] },
                    { "Data": [{ "VarCharValue": "/price" }, {}] }
                ]
            },
            "NextToken": "more"
        });
        let r = rows(&page, 2, 3 * 1024 * 1024, "q-1".into());
        assert_eq!(r.columns, ["http_uri", "http_status"]);
        assert_eq!(r.rows, [vec![Some("/book".into()), Some("500".into())], vec![Some("/price".into()), None]]);
        assert!(r.more);
        assert_eq!(r.data_scanned, "3.0 MB");
    }

    #[test]
    fn a_table_over_the_bucket_is_found_by_its_location_or_its_projection_template() {
        let tables = json!({ "TableList": [
            { "Name": "all_service_request_logs",
              "StorageDescriptor": { "Location": "s3://prod-logs/", "Columns": [{ "Name": "message", "Type": "string" }] },
              "PartitionKeys": [{ "Name": "service", "Type": "string" }, { "Name": "day", "Type": "string" }],
              "Parameters": { "storage.location.template": "s3://prod-logs/${service}/${day}/" } },
            { "Name": "elsewhere", "StorageDescriptor": { "Location": "s3://prod-logs-archive/x/" } },
            { "Name": "rooted", "StorageDescriptor": { "Location": "s3://prod-logs" } }
        ]});
        let found = tables_in(&tables, "prod", "prod-logs");
        assert_eq!(found.iter().map(|t| t.table.as_str()).collect::<Vec<_>>(), ["all_service_request_logs", "rooted"]);
        assert_eq!(found[0].partition_keys.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["service", "day"]);
        assert_eq!(found[0].location, "s3://prod-logs/${service}/${day}/");
    }
}
