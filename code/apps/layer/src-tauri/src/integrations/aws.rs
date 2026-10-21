//! AWS for log sources (§23): S3 and Athena, through the `aws` CLI (OBS-8).
//!
//! The CLI rather than an SDK: it signs in as the user's own terminal does,
//! from their profiles, SSO included, so the app holds no AWS key and adds
//! no dependency that would have to get SSO right itself. Every value from
//! outside (a bucket, a key, a query) goes as its own `--option=value`
//! argument, never through a shell, and never where it could pass for an
//! option (OBS-13). Everything here blocks: callers run it off the async
//! runtime.

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::str_at;
use crate::error::{Error, Result};

mod athena;
pub use athena::{narrowed, read_only, Table, DEFAULT_ROWS, MAX_ROWS};

/// How long one CLI call may take. A query waits on Athena in many calls.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The most of a CLI's output kept: a listing or a page of rows is far less.
const MAX_OUTPUT: usize = 32 * 1024 * 1024;
/// How much of a file is downloaded (OBS-11).
pub const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024;
/// How much of a file's text is returned (OBS-11).
pub const MAX_TEXT: usize = 1024 * 1024;
/// How many matching lines `contains` returns (OBS-11).
pub const MAX_LINES: usize = 500;
/// Folders and files in one listing (OBS-11).
pub const MAX_LISTED: u32 = 200;

/// A profile the AWS config names, with its region if it sets one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    pub name: String,
    pub region: Option<String>,
}

/// The CLI, as one profile in one region.
pub struct Aws {
    program: String,
    profile: String,
    region: Option<String>,
}

impl Aws {
    pub fn new(profile: &str, region: Option<&str>) -> Result<Aws> {
        let program = cli().ok_or_else(|| {
            Error::Other("the aws CLI is not on your PATH: install it (brew install awscli), then try again".into())
        })?;
        Aws::with_program(program, profile, region)
    }

    fn with_program(program: String, profile: &str, region: Option<&str>) -> Result<Aws> {
        if profile.trim().is_empty() {
            return Err(Error::Other("name an AWS profile".into()));
        }
        let region = region.map(str::trim).filter(|r| !r.is_empty());
        if let Some(r) = region {
            if !r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err(Error::Other(format!("{r} is not an AWS region")));
            }
        }
        Ok(Aws { program, profile: profile.to_string(), region: region.map(str::to_string) })
    }

    /// Run the CLI with `args` and read its JSON answer.
    fn json(&self, args: &[String]) -> Result<Value> {
        let out = self.run(args, CALL_TIMEOUT)?;
        if out.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&out)?)
    }

    fn run(&self, args: &[String], timeout: Duration) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.program);
        cmd.arg(format!("--profile={}", self.profile)).arg("--output=json").arg("--no-cli-pager");
        if let Some(r) = &self.region {
            cmd.arg(format!("--region={r}"));
        }
        let mut child = cmd
            .args(args)
            .env_clear()
            .envs(crate::shellenv::user_env())
            .env("AWS_PAGER", "")
            // Nothing to type into: a profile that asks (an MFA code) fails
            // instead of waiting for ever.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| Error::Other(format!("could not start the aws CLI: {e}")))?;
        let out = child.stdout.take().map(|o| read_capped(o, MAX_OUTPUT));
        let err = child.stderr.take().map(|e| read_capped(e, 64 * 1024));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if Instant::now() >= deadline {
                // SAFETY: a negative pid is the process group, which is the
                // CLI's own (process_group(0) above).
                unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let out = out.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
        let err = err.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
        match status {
            None => Err(Error::Other(format!("the aws CLI did not answer within {} seconds", timeout.as_secs()))),
            Some(s) if s.success() => Ok(out),
            Some(_) => Err(Error::Other(explain(&String::from_utf8_lossy(&err), &self.profile))),
        }
    }

    // ------------------------------------------------------------------ S3

    /// The folders and files directly under `prefix` (OBS-11).
    pub fn list(&self, bucket: &str, prefix: &str, next: Option<&str>) -> Result<Listing> {
        check_bucket(bucket)?;
        let mut args = vec![
            "s3api".into(),
            "list-objects-v2".into(),
            format!("--bucket={bucket}"),
            format!("--prefix={}", check_path(prefix)?),
            "--delimiter=/".into(),
            format!("--max-items={MAX_LISTED}"),
        ];
        if let Some(n) = next.filter(|n| !n.is_empty()) {
            args.push(format!("--starting-token={n}"));
        }
        Ok(listing(&self.json(&args)?))
    }

    /// A file's text: at most `MAX_DOWNLOAD` of it, unpacked if gzip, and at
    /// most `MAX_TEXT` returned, or only the lines containing `contains`.
    pub fn read(&self, bucket: &str, key: &str, contains: Option<&str>) -> Result<FileText> {
        check_bucket(bucket)?;
        let key = check_path(key)?;
        if key.is_empty() || key.ends_with('/') {
            return Err(Error::Other("name a file, not a folder: s3_logs_list shows what is in one".into()));
        }
        let file = std::env::temp_dir().join(format!("villain-s3-{}", uuid::Uuid::new_v4()));
        let args = vec![
            "s3api".into(),
            "get-object".into(),
            format!("--bucket={bucket}"),
            format!("--key={key}"),
            format!("--range=bytes=0-{}", MAX_DOWNLOAD - 1),
            file.to_string_lossy().to_string(),
        ];
        let got = self.json(&args);
        let bytes = std::fs::read(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        let meta = match got {
            Ok(meta) => meta,
            // An empty file has no first byte to ask for.
            Err(e) if e.to_string().contains("InvalidRange") => Value::Null,
            Err(e) => return Err(e),
        };
        let size = total_size(&meta).unwrap_or(bytes.len() as u64);
        let (raw, unpacked) = if bytes.starts_with(&[0x1f, 0x8b]) { (gunzip(&bytes)?, true) } else { (bytes, false) };
        Ok(file_text(&String::from_utf8_lossy(&raw), size, unpacked, contains))
    }

}

/// The `aws` CLI, if the user's PATH has one.
pub fn cli() -> Option<String> {
    crate::shellenv::which("aws")
}

/// The profiles `~/.aws/config` and `~/.aws/credentials` name (or the files
/// AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE point at). Only their
/// names and regions are read: a credentials file's keys are skipped over.
pub fn profiles() -> Vec<Profile> {
    let env = crate::shellenv::user_env();
    let home = env.get("HOME").cloned().unwrap_or_default();
    let config = env.get("AWS_CONFIG_FILE").cloned().unwrap_or_else(|| format!("{home}/.aws/config"));
    let credentials = env.get("AWS_SHARED_CREDENTIALS_FILE").cloned().unwrap_or_else(|| format!("{home}/.aws/credentials"));
    let mut all = parse_profiles(&std::fs::read_to_string(config).unwrap_or_default(), true);
    for p in parse_profiles(&std::fs::read_to_string(credentials).unwrap_or_default(), false) {
        if !all.iter().any(|a| a.name == p.name) {
            all.push(p);
        }
    }
    all
}

/// The profiles one file names. In the config file a profile is
/// `[profile name]` (or `[default]`); in the credentials file, `[name]`.
fn parse_profiles(text: &str, config: bool) -> Vec<Profile> {
    let mut out: Vec<Profile> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines().map(str::trim) {
        if let Some(head) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let head = head.trim();
            let name = match (config, head.strip_prefix("profile ")) {
                (true, Some(n)) => Some(n.trim()),
                (true, None) if head == "default" => Some("default"),
                (true, None) => None,
                (false, _) => Some(head),
            };
            current = name.map(|n| {
                out.push(Profile { name: n.to_string(), region: None });
                out.len() - 1
            });
        } else if let (Some(i), Some((k, v))) = (current, line.split_once('=')) {
            if k.trim() == "region" {
                out[i].region = Some(v.trim().to_string());
            }
        }
    }
    out
}

fn read_capped(mut from: impl Read + Send + 'static, cap: usize) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        while let Ok(n) = from.read(&mut buf) {
            if n == 0 {
                break;
            }
            // Read to the end regardless: a full pipe would stop the CLI.
            if kept.len() < cap {
                kept.extend_from_slice(&buf[..n.min(cap - kept.len())]);
            }
        }
        kept
    })
}

/// What the CLI's error means, and what to do about it. The common ones are
/// said plainly; anything else as the CLI said it.
fn explain(stderr: &str, profile: &str) -> String {
    let s = stderr.trim();
    let lower = s.to_lowercase();
    if lower.contains("sso") && (lower.contains("expired") || lower.contains("token") || lower.contains("refresh"))
        || lower.contains("unauthorizedssotoken")
    {
        return format!(
            "the AWS sign-in for profile {profile} has expired: run `aws sso login --profile {profile}` in a terminal, then try again"
        );
    }
    if lower.contains("profile") && lower.contains("could not be found") {
        return format!("there is no AWS profile {profile} in ~/.aws/config");
    }
    if lower.contains("unable to locate credentials") {
        return format!("profile {profile} has no credentials: sign in with `aws sso login --profile {profile}`, or configure it");
    }
    let first: String = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("the aws CLI failed").chars().take(500).collect();
    if lower.contains("accessdenied") || lower.contains("not authorized") {
        return format!("profile {profile} is not allowed to do that: {first}");
    }
    first
}

/// A bucket name S3 allows (OBS-13): 3 to 63 lowercase letters, digits,
/// dots and hyphens, starting and ending with a letter or digit.
pub fn check_bucket(bucket: &str) -> Result<()> {
    let ok = (3..=63).contains(&bucket.len())
        && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        && bucket.starts_with(|c: char| c.is_ascii_alphanumeric())
        && bucket.ends_with(|c: char| c.is_ascii_alphanumeric());
    if ok { Ok(()) } else { Err(Error::Other(format!("{bucket:?} is not an S3 bucket name"))) }
}

/// A key or prefix: no leading slash, no control characters, as S3 keys go.
fn check_path(path: &str) -> Result<&str> {
    let path = path.trim_start_matches('/');
    if path.len() > 1024 || path.chars().any(char::is_control) {
        return Err(Error::Other("that is not an S3 key".into()));
    }
    Ok(path)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    pub folders: Vec<String>,
    pub files: Vec<S3File>,
    /// Pass as `next` for the rest; None when this was all.
    pub next: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct S3File {
    pub key: String,
    pub size: u64,
    pub modified: String,
}

fn listing(v: &Value) -> Listing {
    Listing {
        folders: v.get("CommonPrefixes").and_then(Value::as_array).into_iter().flatten().map(|p| str_at(p, "Prefix")).collect(),
        files: v
            .get("Contents")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|f| S3File { key: str_at(f, "Key"), size: f.get("Size").and_then(Value::as_u64).unwrap_or(0), modified: str_at(f, "LastModified") })
            .collect(),
        next: v.get("NextToken").and_then(Value::as_str).map(str::to_string),
    }
}

/// The whole object's size, from a ranged answer's `bytes 0-8388607/123456789`.
fn total_size(meta: &Value) -> Option<u64> {
    meta.get("ContentRange").and_then(Value::as_str)?.rsplit('/').next()?.parse().ok()
}

/// Unpacked with the gzip macOS ships. A file cut at `MAX_DOWNLOAD` ends
/// mid-stream; what came before the cut is still the text, and is kept.
fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = Command::new("/usr/bin/gzip")
        .arg("-dc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Other(format!("could not unpack the file: {e}")))?;
    let out = child.stdout.take().map(|o| read_capped(o, 8 * MAX_TEXT + MAX_DOWNLOAD as usize));
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(bytes);
    }
    let _ = child.wait();
    Ok(out.map(|h| h.join().unwrap_or_default()).unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileText {
    pub text: String,
    /// The whole file's size in bytes, as stored.
    pub size: u64,
    pub unpacked: bool,
    /// Only part of the file was downloaded, or of its text returned.
    pub cut: bool,
    /// With `contains`: how many lines matched, of those read.
    pub matched: Option<usize>,
}

fn file_text(text: &str, size: u64, unpacked: bool, contains: Option<&str>) -> FileText {
    let downloaded_all = size <= MAX_DOWNLOAD;
    if let Some(needle) = contains.filter(|c| !c.is_empty()) {
        let hits: Vec<&str> = text.lines().filter(|l| l.contains(needle)).collect();
        let kept: Vec<&str> = hits.iter().take(MAX_LINES).copied().collect();
        return FileText { text: kept.join("\n"), size, unpacked, cut: !downloaded_all || hits.len() > MAX_LINES, matched: Some(hits.len()) };
    }
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    FileText { text: text[..end].to_string(), size, unpacked, cut: !downloaded_all || end < text.len(), matched: None }
}

pub fn bytes(n: u64) -> String {
    match n {
        n if n >= 1 << 30 => format!("{:.1} GB", n as f64 / (1u64 << 30) as f64),
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KB", n as f64 / 1024.0),
        n => format!("{n} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profiles_are_read_by_name_and_region_and_a_credentials_files_keys_are_not() {
        let config = "[default]\nregion = eu-west-1\n\n[profile acme-ci]\nsso_session = s\nregion = eu-central-1\n[sso-session s]\nsso_region = eu-central-1\n";
        assert_eq!(
            parse_profiles(config, true),
            [
                Profile { name: "default".into(), region: Some("eu-west-1".into()) },
                Profile { name: "acme-ci".into(), region: Some("eu-central-1".into()) },
            ]
        );
        let credentials = "[legacy]\naws_access_key_id = AKIA\naws_secret_access_key = secret\n";
        let read = parse_profiles(credentials, false);
        assert_eq!(read, [Profile { name: "legacy".into(), region: None }]);
        assert!(!format!("{read:?}").contains("secret"));
    }

    #[test]
    fn only_a_bucket_name_s3_allows_is_used() {
        assert!(check_bucket("production-acme-request-logs").is_ok());
        assert!(check_bucket("logs.example.com").is_ok());
        for bad in ["--endpoint-url=http://evil", "ab", "Logs", "logs/other", "-logs", "logs-", ""] {
            assert!(check_bucket(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_expired_sign_in_says_how_to_renew_it() {
        let said = explain("Error when retrieving token from sso: Token has expired and refresh failed", "acme-prod");
        assert!(said.contains("aws sso login --profile acme-prod"), "{said}");
        let denied = explain("\nAn error occurred (AccessDenied) when calling the ListObjectsV2 operation: Access Denied\n", "acme-ci");
        assert!(denied.starts_with("profile acme-ci is not allowed to do that: An error occurred (AccessDenied)"), "{denied}");
        assert!(explain("The config profile (nope) could not be found", "nope").contains("no AWS profile nope"));
    }

    #[test]
    fn a_listing_has_its_folders_its_files_and_where_to_carry_on() {
        let v = json!({
            "CommonPrefixes": [{ "Prefix": "orders-service/" }, { "Prefix": "billing-service/" }],
            "Contents": [{ "Key": "README", "Size": 12, "LastModified": "2026-10-12T09:00:00+00:00" }],
            "NextToken": "eyJ"
        });
        let l = listing(&v);
        assert_eq!(l.folders, ["orders-service/", "billing-service/"]);
        assert_eq!(l.files[0], S3File { key: "README".into(), size: 12, modified: "2026-10-12T09:00:00+00:00".into() });
        assert_eq!(l.next.as_deref(), Some("eyJ"));
        assert_eq!(listing(&json!({})), Listing { folders: vec![], files: vec![], next: None });
    }

    #[test]
    fn a_file_is_cut_where_it_was_and_contains_keeps_only_its_lines() {
        let text = "a req-1 x\nb\nc req-1 y\n";
        let all = file_text(text, 30, false, None);
        assert_eq!((all.text.as_str(), all.cut, all.matched), (text, false, None));
        let some = file_text(text, 30, false, Some("req-1"));
        assert_eq!((some.text.as_str(), some.matched), ("a req-1 x\nc req-1 y", Some(2)));
        assert!(file_text(text, MAX_DOWNLOAD + 1, false, None).cut, "a file bigger than was downloaded is cut");
        let long = "é".repeat(MAX_TEXT);
        let cut = file_text(&long, long.len() as u64, false, None);
        assert!(cut.cut && cut.text.len() <= MAX_TEXT);
        assert_eq!(total_size(&json!({ "ContentRange": "bytes 0-8388607/123456789" })), Some(123_456_789));
    }

    #[test]
    fn a_gzip_file_is_unpacked_even_when_cut_short() {
        // Varied, as a log is: repeated text packs into one block, and half a
        // block unpacks to nothing.
        let line: String = (0..20_000u64).map(|i| format!("{{\"n\":{i},\"id\":\"{:x}\"}}\n", i.wrapping_mul(0x9E37_79B9_7F4A_7C15))).collect();
        let mut child = Command::new("/usr/bin/gzip").arg("-c").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        // From a thread: gzip's output fills its pipe while its input is written.
        let mut stdin = child.stdin.take().unwrap();
        let input = line.clone();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            stdin.write_all(input.as_bytes()).unwrap();
        });
        let packed = child.wait_with_output().unwrap().stdout;
        writer.join().unwrap();
        assert_eq!(gunzip(&packed).unwrap(), line.as_bytes());
        let half = gunzip(&packed[..packed.len() / 2]).unwrap();
        assert!(!half.is_empty() && line.as_bytes().starts_with(&half), "what came before the cut is kept");
    }

    /// A stand-in for the CLI, written to a temp folder.
    fn fake_cli(script: &str) -> (std::path::PathBuf, Aws) {
        let dir = std::env::temp_dir().join(format!("vl-aws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("aws");
        std::fs::write(&fake, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let aws = Aws::with_program(fake.to_string_lossy().into(), "acme-ci", Some("eu-central-1")).unwrap();
        (dir, aws)
    }

    #[test]
    fn what_comes_from_outside_reaches_the_cli_as_its_own_option_and_no_shell_reads_it() {
        let (dir, aws) = fake_cli(r#"printf '%s\n' "$@""#);
        let out = String::from_utf8(aws.run(&["--prefix=flight service/$(touch pwned)/".into()], CALL_TIMEOUT).unwrap()).unwrap();
        assert_eq!(out.lines().collect::<Vec<_>>(), [
            "--profile=acme-ci", "--output=json", "--no-cli-pager", "--region=eu-central-1", "--prefix=flight service/$(touch pwned)/",
        ]);
        assert!(!std::path::Path::new("pwned").exists() && !dir.join("pwned").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cli_that_fails_says_why_and_one_that_does_not_answer_is_stopped() {
        let (dir, aws) = fake_cli("echo 'An error occurred (NoSuchBucket) when calling the ListObjectsV2 operation' >&2; exit 254");
        let said = aws.list("prod-logs", "", None).unwrap_err().to_string();
        assert!(said.contains("NoSuchBucket"), "{said}");
        let _ = std::fs::remove_dir_all(&dir);

        let (dir, aws) = fake_cli("sleep 5");
        let started = Instant::now();
        let said = aws.run(&[], Duration::from_millis(300)).unwrap_err().to_string();
        assert!(said.contains("did not answer"), "{said}");
        assert!(started.elapsed() < Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against a real bucket, with the user's own sign-in:
    /// `VILLAIN_S3="profile bucket [database workgroup table]" cargo test --lib real_bucket -- --ignored --nocapture`.
    /// Lists the top, reads the first file found a few folders down, and,
    /// given a table, counts one hour of it.
    #[test]
    #[ignore]
    fn a_real_bucket_lists_reads_and_queries() {
        let Ok(spec) = std::env::var("VILLAIN_S3") else { return };
        let parts: Vec<&str> = spec.split_whitespace().collect();
        let aws = Aws::new(parts[0], None).unwrap();
        let top = aws.list(parts[1], "", None).unwrap();
        eprintln!("top: {:?}", top.folders);
        let mut prefix = top.folders.first().cloned().unwrap_or_default();
        let mut file = None;
        for _ in 0..8 {
            let l = aws.list(parts[1], &prefix, None).unwrap();
            if let Some(f) = l.files.first() {
                file = Some(f.key.clone());
                break;
            }
            match l.folders.last() {
                Some(next) => prefix = next.clone(),
                None => break,
            }
        }
        if let Some(key) = file {
            let read = aws.read(parts[1], &key, None).unwrap();
            eprintln!("{key}: {} bytes stored, unpacked {}, cut {}, first line {:?}", read.size, read.unpacked, read.cut,
                read.text.lines().next().map(|l| l.chars().take(160).collect::<String>()));
        }
        if let [_, _, database, workgroup, table] = parts.as_slice() {
            eprintln!("tables over it: {:?}", aws.tables_over(parts[1]).unwrap().iter().map(|t| &t.table).collect::<Vec<_>>());
            let sql = format!("SELECT count(*) AS n FROM {table} WHERE year = '2026' AND month = '10' AND day = '12' AND hour = '09'");
            let rows = aws.query(workgroup, database, &sql, 10).unwrap();
            eprintln!("query: {:?} {:?}, scanned {}", rows.columns, rows.rows, rows.data_scanned);
        }
    }
}
