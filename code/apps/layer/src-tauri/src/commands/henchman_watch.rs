//! What changed on the tasks' pull requests, for the Henchman (HEN-12): a
//! check that failed, comments to answer, changes asked for, a conflict, a
//! pull request that can merge or was merged. Every read of a task's pull
//! requests (`task_prs`: the PR tab, the window's sweep, the Henchman's
//! overview) is compared with the one before it. The window sweeps only
//! while it is in front, so while a Henchman runs this sweeps too, whenever
//! the window has not lately. The feed (`henchman_feed.rs`) tells it.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use crate::integrations::github::PullRequest;

use super::github::{CheckoutPr, GitHubGate};
use super::henchman_tools::failed;
use super::AppState;

/// How long after a sweep, the window's or this one, the next is due.
const EVERY: Duration = Duration::from_secs(5 * 60);
/// How often it looks whether one is due.
const LOOK: Duration = Duration::from_secs(60);
/// Past this many changed pull requests waiting, the oldest go.
const KEEP: usize = 50;
/// How many changes said once are remembered.
const ONCE: usize = 500;

/// One pull request's changes since it was last read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PrChange {
    pub task_id: String,
    pub repo: String,
    pub number: u64,
    pub what: Vec<String>,
}

/// Changes waiting for the feed.
static CHANGED: Mutex<Vec<PrChange>> = Mutex::new(Vec::new());
/// Changes said once per commit, by their keys.
static SAID: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// One change, and its key when it is said once per commit: GitHub reports
/// whether a pull request can merge as "unknown" while it works that out,
/// then the same state as before, which is not news a second time.
#[derive(Debug, PartialEq)]
struct Change {
    what: String,
    once: Option<String>,
}

/// A task's pull requests were read again: what changed since the read
/// before, kept while a Henchman runs to hear it. The first read of a task
/// is only a start; what stood then is in the Henchman's brief.
pub(crate) fn seen(state: &AppState, task_id: &str, before: &[CheckoutPr], now: &[CheckoutPr]) {
    let found = changes(before, now);
    if found.is_empty() || super::henchman::running(state).is_none() {
        return;
    }
    let mut said = SAID.lock();
    let mut kept = CHANGED.lock();
    for (repo, number, list) in found {
        let what: Vec<String> =
            list.into_iter().filter(|c| fresh(&mut said, c.once.as_deref())).map(|c| c.what).collect();
        if !what.is_empty() {
            kept.push(PrChange { task_id: task_id.to_string(), repo, number, what });
        }
    }
    let over = kept.len().saturating_sub(KEEP);
    kept.drain(..over);
}

/// What changed, for the feed to tell; it is then gone from here.
pub(crate) fn take_changes() -> Vec<PrChange> {
    std::mem::take(&mut *CHANGED.lock())
}

/// The window's sweep read every task: none is due here for a while.
pub(crate) async fn swept(state: &AppState, gate: &GitHubGate) {
    let mut session = state.github_session.lock().await;
    if session.generation == gate.generation {
        session.swept = Some(Instant::now());
    }
}

/// While a Henchman runs, read every task's pull requests whenever the
/// window's sweep hasn't for `EVERY`: away from the window, which is when
/// the Henchman works alone, nothing else reads them.
pub fn spawn_henchman_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            let state = app.state::<AppState>();
            let due = state.github_session.lock().await.swept.is_none_or(|at| at.elapsed() >= EVERY);
            if !due || super::henchman::running(&state).is_none() {
                continue;
            }
            sweep(&state).await;
        }
    });
}

/// The window's sweep, without following tickets: only the reads count here.
async fn sweep(state: &AppState) {
    if !state.config.with(|c| c.github_any_connected()) {
        return;
    }
    let gate = super::github::github_gate(state).await;
    use futures_util::stream::{self, StreamExt};
    stream::iter(super::github::sweep_ids(state, &gate, None).await)
        .for_each_concurrent(super::github::SWEEP_TASKS, |id| async move {
            // One that can't be read is tried again at the next sweep.
            let _ = super::github::task_prs(state, &id).await;
        })
        .await;
    swept(state, &gate).await;
}

/// Whether a change is news: always, unless it is said once and was.
fn fresh(said: &mut VecDeque<String>, once: Option<&str>) -> bool {
    let Some(key) = once else {
        return true;
    };
    if said.iter().any(|k| k == key) {
        return false;
    }
    if said.len() >= ONCE {
        said.pop_front();
    }
    said.push_back(key.to_string());
    true
}

/// Whether it can be merged now: ready, nothing blocking it on GitHub, and
/// no changes asked for.
fn can_merge(pr: &PullRequest, verdict: &str) -> bool {
    !pr.draft && matches!(pr.mergeable_state.as_deref(), Some("clean" | "has_hooks")) && verdict != "changes_requested"
}

/// Per pull request read both times, by its repository and number, what
/// changed. One that is new since the read before is not news yet.
fn changes(before: &[CheckoutPr], now: &[CheckoutPr]) -> Vec<(String, u64, Vec<Change>)> {
    let mut out = Vec::new();
    for row in now {
        let Some(pr) = &row.pr else {
            continue;
        };
        let Some((was_row, was)) = before
            .iter()
            .filter(|b| b.checkout_id == row.checkout_id)
            .find_map(|b| b.pr.as_ref().filter(|p| p.number == pr.number).map(|p| (b, p)))
        else {
            continue;
        };
        let once = |what: &str| Some(format!("{}#{}@{}:{what}", row.checkout_id, pr.number, pr.head_sha));
        let mut list = Vec::new();
        if was.state == "open" && pr.state != "open" {
            let what = if pr.merged { "merged" } else { "closed without merging" };
            list.push(Change { what: what.into(), once: None });
        }
        if pr.state == "open" {
            // Checks run per commit: after a push every failure is new.
            let failed_before: HashSet<&str> = if was.head_sha == pr.head_sha {
                was_row.checks.iter().filter(|c| failed(c.conclusion.as_deref())).map(|c| c.name.as_str()).collect()
            } else {
                HashSet::new()
            };
            let names: Vec<&str> = row
                .checks
                .iter()
                .filter(|c| failed(c.conclusion.as_deref()) && !failed_before.contains(c.name.as_str()))
                .map(|c| c.name.as_str())
                .collect();
            if !names.is_empty() {
                let names = names.join(", ");
                // Run again and failed again on the same commit: told already.
                list.push(Change { what: format!("checks failed: {names}"), once: once(&format!("failed {names}")) });
            }
            if row.to_reply > was_row.to_reply {
                let n = row.to_reply;
                let what = if n == 1 { "1 comment to answer".to_string() } else { format!("{n} comments to answer") };
                list.push(Change { what, once: None });
            }
            if row.verdict == "changes_requested" && was_row.verdict != "changes_requested" {
                list.push(Change { what: "changes asked for".into(), once: None });
            }
            if pr.mergeable_state.as_deref() == Some("dirty") && was.mergeable_state.as_deref() != Some("dirty") {
                list.push(Change { what: "conflicts with its base".into(), once: once("dirty") });
            }
            if can_merge(pr, &row.verdict) && !can_merge(was, &was_row.verdict) {
                list.push(Change { what: "can merge".into(), once: once("clean") });
            }
        }
        if !list.is_empty() {
            out.push((row.repo.clone(), pr.number, list));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::github::CheckRun;

    fn row(state: &str, sha: &str, merge: &str) -> CheckoutPr {
        CheckoutPr {
            source: None,
            checkout_id: "c1".into(),
            repo: "api".into(),
            pr: Some(PullRequest {
                number: 34,
                title: "Pay in two parts".into(),
                state: state.into(),
                draft: false,
                author: "me".into(),
                head: "acme-12".into(),
                head_sha: sha.into(),
                base: "main".into(),
                url: "https://github.com/acme/api/pull/34".into(),
                mergeable_state: Some(merge.into()),
                merged: false,
                comments: 0,
                review_comments: 0,
            }),
            checks: Vec::new(),
            checks_state: Default::default(),
            reviews: Vec::new(),
            past: Vec::new(),
            verdict: "none".into(),
            base: "main".into(),
            changed: 3,
            to_reply: 0,
            error: None,
        }
    }

    fn check(name: &str, conclusion: Option<&str>) -> CheckRun {
        CheckRun {
            name: name.into(),
            status: "completed".into(),
            conclusion: conclusion.map(str::to_string),
            url: None,
        }
    }

    fn said(before: &CheckoutPr, now: &CheckoutPr) -> Vec<String> {
        changes(std::slice::from_ref(before), std::slice::from_ref(now))
            .into_iter()
            .flat_map(|(_, _, list)| list.into_iter().map(|c| c.what))
            .collect()
    }

    #[test]
    fn a_check_that_fails_is_news_once_per_commit_and_again_after_a_push() {
        let before = row("open", "a1", "unstable");
        let mut now = before.clone();
        now.checks = vec![check("build", Some("failure")), check("lint", Some("success"))];
        assert_eq!(said(&before, &now), ["checks failed: build"]);
        assert!(said(&now, &now).is_empty(), "still failed is not news");
        let mut pushed = now.clone();
        if let Some(pr) = pushed.pr.as_mut() {
            pr.head_sha = "b2".into();
        }
        assert_eq!(said(&now, &pushed), ["checks failed: build"], "failed again on the new commit");
    }

    #[test]
    fn comments_changes_asked_for_and_a_conflict_are_news() {
        let before = row("open", "a1", "clean");
        let mut now = row("open", "a1", "dirty");
        now.to_reply = 2;
        now.verdict = "changes_requested".into();
        assert_eq!(said(&before, &now), ["2 comments to answer", "changes asked for", "conflicts with its base"]);
        let mut fewer = now.clone();
        fewer.to_reply = 1;
        assert!(said(&now, &fewer).is_empty(), "answering a comment is not news");
    }

    #[test]
    fn a_pull_request_that_can_merge_or_was_merged_is_news() {
        assert_eq!(said(&row("open", "a1", "blocked"), &row("open", "a1", "clean")), ["can merge"]);
        let mut merged = row("closed", "a1", "clean");
        if let Some(pr) = merged.pr.as_mut() {
            pr.merged = true;
        }
        assert_eq!(said(&row("open", "a1", "clean"), &merged), ["merged"]);
        assert_eq!(said(&row("open", "a1", "clean"), &row("closed", "a1", "clean")), ["closed without merging"]);
    }

    #[test]
    fn a_draft_or_a_pull_request_new_since_the_last_read_is_not_news() {
        let mut draft = row("open", "a1", "clean");
        if let Some(pr) = draft.pr.as_mut() {
            pr.draft = true;
        }
        assert!(said(&row("open", "a1", "blocked"), &draft).is_empty());
        let mut none = row("open", "a1", "blocked");
        none.pr = None;
        let mut failing = row("open", "a1", "clean");
        failing.checks = vec![check("build", Some("failure"))];
        assert!(said(&none, &failing).is_empty());
    }

    #[test]
    fn a_state_said_once_is_not_said_again_after_github_works_it_out_again() {
        let mut said = VecDeque::new();
        let key = "c1#34@a1:clean";
        assert!(fresh(&mut said, Some(key)));
        assert!(!fresh(&mut said, Some(key)), "clean, unknown, clean again");
        assert!(fresh(&mut said, None) && fresh(&mut said, None));
        let all = changes(&[row("open", "a1", "unknown")], &[row("open", "a1", "clean")]);
        assert_eq!(all[0].2[0].once.as_deref(), Some(key));
    }
}
