//! A bounded read of check runs for an exact GitHub PR head, never a branch.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{timeout_at, Instant};

use super::{CheckRun, GitHub, ReadState};

const PAGE_SIZE: u64 = 100;
const MAX_PAGES: u64 = 10;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Serialize)]
pub struct PrChecks {
    pub checks: Vec<CheckRun>,
    pub checks_state: ReadState,
}

impl GitHub {
    /// Check runs in the PR's base repository. Like the old reader, this
    /// asks for GitHub's default `latest` view, not all superseded attempts.
    pub async fn pr_checks(&self, owner: &str, repo: &str, head_sha: &str) -> PrChecks {
        let mut out = PrChecks::default();
        if head_sha.len() != 40 || !head_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return out;
        }
        if ![owner, repo].into_iter().all(|part| {
            !matches!(part, "" | "." | "..")
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        }) {
            return out;
        }

        let path = format!("/repos/{owner}/{repo}/commits/{head_sha}/check-runs");
        let deadline = Instant::now() + READ_TIMEOUT;
        let mut seen = HashSet::new();
        let mut expected_total = None;
        let mut consistent = true;

        for page in 1..=MAX_PAGES {
            let req = self.req(reqwest::Method::GET, &path).query(&[("per_page", PAGE_SIZE), ("page", page)]);
            let Ok(Ok(body)) = timeout_at(deadline, self.json(req)).await else { break };
            let Some(runs) = body["check_runs"].as_array() else { break };
            for raw in runs {
                match RestCheckRun::deserialize(raw).ok().and_then(|run| run.for_head(head_sha)) {
                    Some((id, mut run)) => {
                        // A repeated page or a moving offset can hide a run.
                        // Keep the rows themselves: a duplicate may now show a failure.
                        consistent &= seen.insert(id);
                        // A bad link makes the read incomplete, not the outcome unknown.
                        match raw.get("html_url") {
                            Some(Value::String(url)) => run.url = Some(url.clone()),
                            None | Some(Value::Null) => {}
                            _ => consistent = false,
                        }
                        out.checks.push(run);
                    }
                    None => consistent = false,
                }
            }
            if !out.checks.is_empty() {
                out.checks_state = ReadState::Incomplete;
            }

            let Some(total) = body["total_count"].as_u64() else { break };
            consistent &= expected_total.get_or_insert(total) == &total;
            let count = runs.len() as u64;
            let returned = (page - 1) * PAGE_SIZE + count;
            if consistent && count <= PAGE_SIZE && returned == total {
                out.checks_state = ReadState::Complete;
                break;
            }
            // Page two starts at row 101 even if page one came back short.
            // A gap or an overfull page cannot establish a complete read.
            if count != PAGE_SIZE || returned >= total {
                break;
            }
        }
        out
    }
}

#[derive(Deserialize)]
struct RestCheckRun {
    id: u64,
    head_sha: String,
    name: String,
    status: String,
    conclusion: Option<String>,
}

impl RestCheckRun {
    fn for_head(self, sha: &str) -> Option<(u64, CheckRun)> {
        if self.id == 0 || !self.head_sha.eq_ignore_ascii_case(sha) || self.name.trim().is_empty() {
            return None;
        }
        let valid_state = match self.status.as_str() {
            "completed" => matches!(
                self.conclusion.as_deref(),
                Some(
                    "action_required"
                        | "cancelled"
                        | "failure"
                        | "neutral"
                        | "skipped"
                        | "stale"
                        | "startup_failure"
                        | "success"
                        | "timed_out"
                )
            ),
            "in_progress" | "queued" | "pending" | "requested" | "waiting" => self.conclusion.is_none(),
            _ => false,
        };
        valid_state.then_some((
            self.id,
            CheckRun { name: self.name, status: self.status, conclusion: self.conclusion, url: None },
        ))
    }
}
