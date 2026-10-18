//! Forgejo's HTTP API (`/api/v1`), with the connection's token. Read-only:
//! Forge never merges or changes anything on Forgejo.

use std::time::Duration;

use chrono::{DateTime, Utc};
use forge_domain::PullRequestChecks;
use reqwest::StatusCode;
use serde_json::Value;

use crate::github::app::valid_full_name;

/// Pull requests asked for per page, Forgejo's usual maximum.
const PAGE: usize = 50;

/// Pages of closed pull requests read at most in one check.
const MAX_PAGES: u32 = 20;

pub struct Client {
    http: reqwest::Client,
    server: String,
    token: String,
}

impl Client {
    /// A client for a server, as `normalize_server` returns it.
    pub fn new(server: &str, token: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("forge")
            .build()
            .expect("a client without special settings builds");
        Client {
            http,
            server: server.to_string(),
            token: token.to_string(),
        }
    }

    /// `None` when Forgejo says there's no such thing.
    async fn get(&self, path: &str) -> Result<Option<Value>, String> {
        let response = self
            .http
            .get(format!("{}/api/v1{path}", self.server))
            .header("Authorization", format!("token {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("couldn't reach Forgejo: {}", without_url(&e)))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            let message = response
                .json::<Value>()
                .await
                .ok()
                .and_then(|body| body["message"].as_str().map(str::to_string))
                .unwrap_or_default();
            return Err(match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    format!("Forgejo refused the token ({status})")
                }
                _ if message.is_empty() => format!("Forgejo answered {status}"),
                _ => format!("Forgejo answered {status}: {message}"),
            });
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| format!("Forgejo's answer wasn't readable: {}", without_url(&e)))
    }

    async fn get_found(&self, path: &str, what: &str) -> Result<Value, String> {
        self.get(path)
            .await?
            .ok_or_else(|| format!("Forgejo doesn't know {what}, or the token can't read it"))
    }

    /// Checks that the token can read a repository.
    pub async fn repository(&self, full_name: &str) -> Result<(), String> {
        check_name(full_name)?;
        self.get_found(&format!("/repos/{full_name}"), full_name)
            .await
            .map(|_| ())
    }

    /// Every open pull request of a repository.
    pub async fn open_pull_requests(&self, full_name: &str) -> Result<Vec<Value>, String> {
        check_name(full_name)?;
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let batch = self.pull_requests(full_name, "open", page).await?;
            let last = batch.len() < PAGE;
            all.extend(batch);
            if last {
                break;
            }
        }
        Ok(all)
    }

    /// The closed (and merged) pull requests of a repository updated since
    /// a time, newest first.
    pub async fn closed_since(
        &self,
        full_name: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<Value>, String> {
        check_name(full_name)?;
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let batch = self.pull_requests(full_name, "closed", page).await?;
            let last = batch.len() < PAGE;
            let mut older = false;
            for pr in batch {
                if updated_at(&pr).is_some_and(|at| at < since) {
                    older = true;
                } else {
                    all.push(pr);
                }
            }
            if last || older {
                break;
            }
        }
        Ok(all)
    }

    async fn pull_requests(
        &self,
        full_name: &str,
        state: &str,
        page: u32,
    ) -> Result<Vec<Value>, String> {
        let path = format!(
            "/repos/{full_name}/pulls?state={state}&sort=recentupdate&page={page}&limit={PAGE}"
        );
        match self.get_found(&path, full_name).await? {
            Value::Array(prs) => Ok(prs),
            _ => Err("Forgejo's list of pull requests wasn't a list".into()),
        }
    }

    /// One pull request; `None` if Forgejo no longer has it.
    pub async fn pull_request(
        &self,
        full_name: &str,
        number: i64,
    ) -> Result<Option<Value>, String> {
        check_name(full_name)?;
        self.get(&format!("/repos/{full_name}/pulls/{number}"))
            .await
    }

    /// What the commit statuses on a commit say.
    pub async fn checks(&self, full_name: &str, sha: &str) -> Result<PullRequestChecks, String> {
        check_name(full_name)?;
        if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("unexpected commit {sha}"));
        }
        let combined = self
            .get_found(&format!("/repos/{full_name}/commits/{sha}/status"), sha)
            .await?;
        Ok(checks_from(&combined))
    }
}

/// Commit statuses (Forgejo's combined status, the latest per context):
/// failed when any failed or errored, otherwise running while any is
/// pending, passed when all passed, `None` without any.
pub fn checks_from(combined: &Value) -> PullRequestChecks {
    let states: Vec<&str> = combined["statuses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| s["status"].as_str().or(s["state"].as_str()).unwrap_or(""))
        .collect();
    if states.is_empty() {
        PullRequestChecks::None
    } else if states.iter().any(|s| matches!(*s, "failure" | "error")) {
        PullRequestChecks::Failed
    } else if states.contains(&"pending") {
        PullRequestChecks::Running
    } else {
        PullRequestChecks::Passed
    }
}

pub fn updated_at(pr: &Value) -> Option<DateTime<Utc>> {
    time(&pr["updated_at"])
}

pub fn time(value: &Value) -> Option<DateTime<Utc>> {
    value
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn check_name(full_name: &str) -> Result<(), String> {
    if valid_full_name(full_name) {
        Ok(())
    } else {
        Err(format!("{full_name} isn't a repository name (owner/name)"))
    }
}

/// An error without the address it was for: the token never shows in
/// errors anyway, but the address needn't either.
fn without_url(e: &reqwest::Error) -> String {
    let mut e = e.to_string();
    if let Some(at) = e.find(" for url") {
        e.truncate(at);
    }
    e
}

/// A server address as Forge keeps it: `http(s)://host[:port][/path]`,
/// without a trailing slash.
pub fn normalize_server(server: &str) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(server)
        .map_err(|_| format!("{server} isn't an address like https://git.example.com"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
    {
        return Err(format!(
            "{server} isn't an address like https://git.example.com"
        ));
    }
    Ok(server.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn commit_statuses_become_checks() {
        let of = |states: &[&str]| {
            checks_from(&json!({
                "statuses": states.iter().map(|s| json!({ "status": s })).collect::<Vec<_>>()
            }))
        };
        assert_eq!(
            checks_from(&json!({ "statuses": [] })),
            PullRequestChecks::None
        );
        assert_eq!(
            checks_from(&json!({ "statuses": null })),
            PullRequestChecks::None
        );
        assert_eq!(of(&["success", "success"]), PullRequestChecks::Passed);
        assert_eq!(of(&["success", "pending"]), PullRequestChecks::Running);
        assert_eq!(of(&["pending", "error"]), PullRequestChecks::Failed);
        assert_eq!(of(&["failure"]), PullRequestChecks::Failed);
        assert_eq!(of(&["warning"]), PullRequestChecks::Passed);
    }

    #[test]
    fn server_addresses_are_checked() {
        assert_eq!(
            normalize_server(" https://git.codevillain.eu/ ").unwrap(),
            "https://git.codevillain.eu"
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:3001").unwrap(),
            "http://127.0.0.1:3001"
        );
        assert!(normalize_server("git.codevillain.eu").is_err());
        assert!(normalize_server("ftp://git.codevillain.eu").is_err());
        assert!(normalize_server("https://me:secret@git.codevillain.eu").is_err());
        assert!(normalize_server("https://git.codevillain.eu/?x=1").is_err());
    }
}
