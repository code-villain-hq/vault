//! Opening a task's pull requests (PR-1, PR-2): push each repo with work no
//! pull request has landed, open or reuse its PR, and announce the new ones.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::config::TrackerKind;
use crate::error::Result;
use crate::git;
use crate::integrations::github;

use super::diff::RepoResult;
use super::github_sources::checkout_client;
use super::slack::{record_post, slack_for};
use super::tickets::linked_tracker;
use super::{off_runtime, AppState};

/// Push and open a PR in every repository that has changes, then post the whole
/// set back to the Jira ticket and Slack, and list in each PR the others.
/// `skip` names checkouts the user left out: not pushed, not opened.
#[tauri::command]
pub async fn github_open_prs(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    title: String,
    body: String,
    draft: bool,
    skip: Vec<String>,
) -> Result<Vec<RepoResult>> {
    let task = state.config.task(&task_id)?;
    super::pr_task::not_a_review(&task, "pushed or opened as a pull request")?;
    let gate = super::github::github_gate(&state).await;
    let tracker = linked_tracker(&state, &task).ok();
    let kind = state.config.with(|c| c.tracker_in_use());
    let issue_for = |host: &str| {
        task.issue_key.as_deref().filter(|key| {
            crate::tickets::github::parse_key(key).is_none()
                || (kind == Some(TrackerKind::Github)
                    && state
                        .config
                        .with(|c| c.github_issue_host() == Some(host) && crate::tickets::task_on_tracker(c, &task)))
        })
    };

    let mut results = Vec::new();
    let mut opened: Vec<github::Sibling> = Vec::new();
    // Every open PR of the task, new or not: one opened earlier does not know
    // about a repo added since.
    let mut set = Vec::new();

    // Commit spec once for the task, in the repositories whose committed copy
    // is behind `spec/`, before anything is pushed.
    let mut spec_rows: std::collections::HashMap<String, RepoResult> = {
        let (task, skip) = (task.clone(), skip.clone());
        super::blocking(app.clone(), move |state| Ok(super::spec_commit::commit_changed(state, &task, &skip))).await?
    }
    .into_iter()
    .map(|r| (r.checkout_id.clone(), r))
    .collect();

    for checkout in state.config.checkouts_of(&task_id) {
        if skip.contains(&checkout.id) {
            continue;
        }
        let dir = PathBuf::from(&checkout.path);
        let project = match state.config.project(&checkout.project_id) {
            Ok(project) => project,
            Err(e) => {
                results.push(RepoResult {
                    checkout_id: checkout.id,
                    repo: checkout.project_id,
                    ok: false,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        let repo = project.name.clone();
        // The spec goes to review with the code (SPEC-35): what Commit spec
        // put in this repository above. One that could not be committed is
        // named, and its code is not opened without it.
        if let Some(row) = spec_rows.remove(&checkout.id) {
            let failed = !row.ok;
            results.push(RepoResult {
                detail: if failed {
                    format!("the spec could not be committed: {}", row.detail)
                } else {
                    format!("the spec changed since its last commit, so it was committed: {}", row.detail)
                },
                ..row
            });
            if failed {
                continue;
            }
        }

        let measure = |point: Option<String>| {
            let (dir, base) = (dir.clone(), checkout.base.clone());
            off_runtime(move || {
                (
                    git::changed_count(&dir, &base, point.as_deref(), git::Scope::Branch),
                    committed_since(&dir, &base, point.as_deref()),
                )
            })
        };
        let (mut changed, mut committed) = measure(checkout.base_commit.clone()).await?;

        // What the branch has had on GitHub, asked only of a repo with work to
        // open. A merged one moves the branch point up to what it landed
        // (UPD-6): counted from where the branch was cut, a repo merged while
        // another was still in review had its finished work opened again.
        let mut open = None;
        let mut merged = None;
        let route = if changed > 0 && committed {
            match checkout_client(&state, &checkout.path).await {
                Ok((origin, client)) => {
                    let (owner, name) = origin.slug();
                    let prs = match client.pulls_for_branch(owner, name, &task.branch).await {
                        Ok(prs) => prs,
                        Err(e) => {
                            results.push(RepoResult {
                                checkout_id: checkout.id,
                                repo,
                                ok: false,
                                detail: e.to_string(),
                            });
                            continue;
                        }
                    };
                    open = prs.iter().find(|p| p.state == "open").cloned();
                    if let Some(pr) = super::landed(&prs) {
                        if let Some(point) = super::advance(&state, &checkout, &pr.head_sha).await? {
                            (changed, committed) = measure(Some(point)).await?;
                            merged = Some(pr.number);
                        }
                    }
                    Some((origin, client))
                }
                Err(e) => {
                    results.push(RepoResult { checkout_id: checkout.id, repo, ok: false, detail: e.to_string() });
                    continue;
                }
            }
        } else {
            None
        };
        if changed == 0 {
            results.push(RepoResult {
                checkout_id: checkout.id,
                repo,
                ok: true,
                detail: match merged {
                    Some(n) => format!("#{n} merged, nothing since — skipped"),
                    None => "no changes, skipped".into(),
                },
            });
            continue;
        }
        // Changes, but none committed: pushed anyway, GitHub refused the PR
        // with a bare 422 "No commits between".
        if !committed {
            results.push(RepoResult {
                checkout_id: checkout.id,
                repo,
                ok: false,
                detail: "only uncommitted changes — commit them first".into(),
            });
            continue;
        }
        let Some((origin, client)) = route else { continue };
        let issue = issue_for(&origin.host);
        let pr_body = body_for(kind, issue, task.issue_key.as_deref(), task.issue_url.as_deref(), &body);

        // Whether the PR came out of this call or was already there. Only the
        // new ones are announced: an existing PR was posted to the ticket and
        // the channel when it was opened, and saying so again on every press
        // of the button is noise that makes the real announcements look alike.
        let outcome: Result<(github::Sibling, bool)> = async {
            // A push is a network round trip with no timeout of its own; on
            // the async workers it held up the MCP server until git gave up.
            {
                let (dir, branch, lease, expected) =
                    (dir.clone(), task.branch.clone(), checkout.push_lease.clone(), origin.clone());
                off_runtime(move || {
                    git::verify_github_push_origin(&dir, &expected)?;
                    git::push(&dir, &branch, lease.as_deref())
                })
                .await??;
                // Spent by the push itself, whatever comes after it. Kept
                // because the slug could not be read, it was sent with every
                // later push and refused each one as "someone else pushed".
                super::diff::pushed(&state, &checkout.id);
            }
            let (owner, name) = origin.slug();

            let (pr, new) = match open {
                Some(existing) => (existing, false),
                None => (
                    client.create_pull(owner, name, &title, &pr_body, &task.branch, &checkout.base, draft).await?,
                    true,
                ),
            };
            Ok((
                github::Sibling {
                    repo: repo.clone(),
                    owner: owner.into(),
                    name: name.into(),
                    url: pr.url,
                    number: pr.number,
                },
                new,
            ))
        }
        .await;

        match outcome {
            Ok((pr, new)) => {
                results.push(RepoResult {
                    checkout_id: checkout.id.clone(),
                    repo,
                    ok: true,
                    detail: if new {
                        format!("#{}", pr.number)
                    } else {
                        format!("#{} already open, pushed", pr.number)
                    },
                });
                set.push((checkout.id, origin.host, client, pr.clone()));
                if new {
                    opened.push(pr);
                }
            }
            Err(e) => results.push(RepoResult { checkout_id: checkout.id, repo, ok: false, detail: e.to_string() }),
        }
    }

    if !opened.is_empty() && set.len() > 1 {
        for (checkout, host, client, pr) in &set {
            let others: Vec<_> = set
                .iter()
                .filter(|(_, from, _, sibling)| from == host && sibling.url != pr.url)
                .map(|(_, _, _, sibling)| sibling)
                .collect();
            if others.is_empty() {
                continue;
            }
            let issue = issue_for(host);
            if let Err(e) = client.list_others(pr, &others, issue).await {
                results.push(RepoResult {
                    checkout_id: checkout.clone(),
                    repo: pr.repo.clone(),
                    ok: false,
                    detail: format!("the PR is open, but listing the others in it failed ({e})"),
                });
            }
        }
    }

    if !opened.is_empty() {
        let lines: Vec<String> = opened.iter().map(|p| format!("{}: {}", p.repo, p.url)).collect();

        // A GitHub issue lists the pull requests that fix it by itself.
        if let Some(key) = task.issue_key.as_ref() {
            if let Some(tracker) = tracker.as_ref().filter(|t| t.kind() == TrackerKind::Jira) {
                let text = format!(
                    "Pull request{} for this ticket:\n{}",
                    if opened.len() == 1 { "" } else { "s" },
                    lines.join("\n"),
                );
                // Said, not swallowed: this only runs for PRs opened by this
                // call, so a retry finds them already open and never posts —
                // the ticket that is meant to be the hub would just not know.
                if let Err(e) = tracker.comment(key, &text).await {
                    results.push(RepoResult {
                        checkout_id: key.clone(),
                        repo: key.clone(),
                        ok: false,
                        detail: format!(
                            "the PRs are open, but the comment linking them failed ({e}); add them to the ticket by hand"
                        ),
                    });
                }
            }
        }

        if let Ok(Some((client, cfg))) = slack_for(&state, "prs") {
            let text = format!("*{title}* — {} PR{} opened", opened.len(), if opened.len() == 1 { "" } else { "s" },);
            let context = opened
                .iter()
                .map(|p| format!("<{}|{} #{}>", p.url, p.repo, p.number))
                .collect::<Vec<_>>()
                .join("  ·  ");
            if let Ok(posted) = client.post(&cfg.channel, &text, Some(&context)).await {
                record_post(&state, posted);
            }
        }
    }

    // What was just opened is the sweep's to follow from now (PR-8).
    let mut session = state.github_session.lock().await;
    if session.generation == gate.generation {
        session.settled.remove(&task_id);
    }
    Ok(results)
}

/// A pull request for a GitHub issue says it fixes it, which is how GitHub
/// links the two and closes the issue on merge (TKT-18).
fn body_for(
    tracker: Option<TrackerKind>,
    allowed: Option<&str>,
    task_issue: Option<&str>,
    task_url: Option<&str>,
    body: &str,
) -> String {
    if let Some(key) = task_issue.filter(|key| allowed.is_none() && crate::tickets::github::parse_key(key).is_some()) {
        // The draft model and the agent share a body across repositories.
        // Prompts alone cannot keep the task's issue reference out of it.
        let key = key.to_ascii_lowercase();
        let url = task_url.map(str::to_ascii_lowercase);
        let short = key.rsplit_once('#').map(|(_, n)| format!("#{n}"));
        return body
            .lines()
            .filter(|line| {
                let line = line.to_ascii_lowercase();
                let has = |reference: &str| {
                    line.match_indices(reference)
                        .any(|(at, matched)| !line.as_bytes().get(at + matched.len()).is_some_and(u8::is_ascii_digit))
                };
                !has(&key)
                    && !url.as_deref().is_some_and(has)
                    && !short.as_deref().is_some_and(|short| {
                        ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"]
                            .iter()
                            .any(|verb| has(&format!("{verb} {short}")))
                    })
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    fixes(tracker, allowed, body.to_string())
}

fn fixes(tracker: Option<TrackerKind>, key: Option<&str>, body: String) -> String {
    let Some(key) = key.filter(|_| tracker == Some(TrackerKind::Github)) else {
        return body;
    };
    let line = format!("Fixes {key}");
    if body.to_lowercase().contains(&line.to_lowercase()) {
        return body;
    }
    match body.trim_end() {
        "" => line,
        text => format!("{text}\n\n{line}"),
    }
}

/// Whether the branch has a commit since `from`, or its branch point.
fn committed_since(dir: &Path, base: &str, from: Option<&str>) -> bool {
    git::commits_since(dir, base, from).is_ok_and(|c| !c.is_empty())
}

#[cfg(test)]
mod tests {
    use super::super::repos::tests::{commit, git};
    use super::*;

    #[test]
    fn a_github_issue_pull_request_says_it_fixes_it_once() {
        let gh = Some(TrackerKind::Github);
        assert_eq!(fixes(gh, Some("acme/web#12"), "Loops no more.\n".into()), "Loops no more.\n\nFixes acme/web#12");
        assert_eq!(fixes(gh, Some("acme/web#12"), "fixes acme/web#12".into()), "fixes acme/web#12");
        assert_eq!(fixes(gh, Some("acme/web#12"), String::new()), "Fixes acme/web#12");
        // Jira moves its tickets itself, and a task without one fixes nothing.
        assert_eq!(fixes(Some(TrackerKind::Jira), Some("ACME-1"), "x".into()), "x");
        assert_eq!(fixes(gh, None, "x".into()), "x");
    }

    #[test]
    fn a_foreign_pr_cannot_publish_the_tasks_issue_from_a_generated_body() {
        let src = "Summary\n\nIssue: https://git.example.com/acme/web/issues/42\nFIXES Acme/Web#42\nCloses #42\nKeep acme/web#420.";
        let body = body_for(
            Some(TrackerKind::Github),
            None,
            Some("acme/web#42"),
            Some("https://git.example.com/acme/web/issues/42"),
            src,
        );
        assert_eq!(body, "Summary\n\nKeep acme/web#420.");
    }

    #[test]
    fn only_committed_work_counts_as_committed() {
        let dir = std::env::temp_dir().join(format!("vl-open-prs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "test@villain.local"]);
        git(&dir, &["config", "user.name", "Test"]);
        commit(&dir, "a.txt");
        git(&dir, &["checkout", "-q", "-b", "task"]);
        let point = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();

        std::fs::write(dir.join("draft.txt"), "draft\n").unwrap();
        assert!(!committed_since(&dir, "main", Some(&point)));
        commit(&dir, "infra.tf");
        assert!(committed_since(&dir, "main", Some(&point)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
