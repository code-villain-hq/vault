//! GitHub connect, PR listing, retarget. Opening PRs is in `open_prs.rs`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};

use super::panes::agent_file_dir;
use super::{off_runtime, AppState};
use crate::config::github::GithubSource;
use crate::error::{Error, Result};
use crate::git;
use crate::integrations::github::{self, CodeLine, GitHub, ReadState};

mod session;
pub(crate) use session::{github_gate, GitHubGate, NativeGitHub};

// ------------------------------------------------------------------ github

#[tauri::command]
pub async fn github_connect<R: Runtime>(
    app: AppHandle<R>,
    source: Option<String>,
    api_url: String,
    web_url: String,
    token: String,
    review_team: Option<String>,
) -> Result<String> {
    super::github_sources::connect(app, source, &api_url, &web_url, token, review_team.as_deref()).await
}

pub(crate) fn apply_github_settings<T>(state: &AppState, write: impl FnOnce() -> Result<T>) -> Result<T> {
    state.github_session.blocking_lock().update(state, false, write)
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutPr {
    pub checkout_id: String,
    pub repo: String,
    pub source: Option<String>,
    pub pr: Option<crate::integrations::github::PullRequest>,
    pub checks: Vec<crate::integrations::github::CheckRun>,
    pub checks_state: ReadState,
    /// Every review submitted on `pr`, so the panel can name who said what.
    pub reviews: Vec<crate::integrations::github::Review>,
    /// Earlier pull requests from this same branch, newest first. A branch
    /// abandoned once and retried has them, and losing them off the screen
    /// loses the record of what was already tried.
    pub past: Vec<crate::integrations::github::PullRequest>,
    /// The decision those reviews add up to: "approved", "changes_requested",
    /// "commented" or "none".
    pub verdict: String,
    /// Where this repository's next PR will be opened against. An open PR
    /// keeps whatever base it was created with until it is retargeted, so
    /// this and `pr.base` can differ, and the panel says so when they do.
    pub base: String,
    /// Files changed on the branch since its branch point, which a merged PR
    /// moves up to what it landed (UPD-6).
    pub changed: usize,
    /// Threads and comments on the open `pr` waiting on its author (PR-6).
    pub to_reply: u32,
    pub error: Option<String>,
}

/// Point a checkout's pull requests at a different branch.
///
/// The base is chosen when the worktree is made, from the repository's own
/// default branch, and there was no way to say otherwise — work meant for a
/// long-lived integration branch had to be retargeted by hand on GitHub every
/// time. This decides where the *next* PR is opened; one already open keeps
/// its base until `github_retarget_pr` moves it.
#[tauri::command]
pub async fn set_checkout_base(state: State<'_, AppState>, checkout_id: String, base: String) -> Result<()> {
    set_base(&state, &checkout_id, &base)
}

/// [`set_checkout_base`], for agents too (`set_base`).
pub(crate) fn set_base(state: &AppState, checkout_id: &str, base: &str) -> Result<()> {
    let base = base.trim().to_string();
    if base.is_empty() {
        return Err(Error::Other("a pull request needs a base branch".into()));
    }
    // It reaches git where options are still read, on a timer: a base of
    // `--output=<file>` would have written that file every PR sweep.
    if base.starts_with('-') {
        return Err(Error::Other(format!("{base:?} is not a branch name")));
    }
    state.config.update(|c| {
        let Some(found) = c.checkouts.iter_mut().find(|c| c.id == checkout_id) else {
            return Err(Error::NotFound(format!("checkout {checkout_id}")));
        };
        found.base = base.clone();
        // The recorded branch point belongs to the base it was taken from, and
        // `baseline` uses it as-is — keeping it would measure this branch
        // against where it left a branch it is no longer going to. Dropped, so
        // the diff falls back to the merge base with whatever the base is now.
        found.base_commit = None;
        Ok(())
    })?
}

/// What each of a task's branches is measured against, one row per repository.
#[derive(Debug, Serialize)]
pub struct RepoBranchFacts {
    pub checkout_id: String,
    pub repo: String,
    pub base: String,
    #[serde(flatten)]
    pub facts: git::BranchFacts,
}

/// Off the command thread: `branch_facts` is half a dozen git calls per
/// repository, and the Diff view asks for all of them again every time its
/// file list is rebuilt.
#[tauri::command]
pub async fn task_branch_facts(app: AppHandle, task_id: String) -> Result<Vec<RepoBranchFacts>> {
    super::blocking(app, move |state| task_branch_facts_inner(state, task_id)).await
}

fn task_branch_facts_inner(state: &AppState, task_id: String) -> Result<Vec<RepoBranchFacts>> {
    let task = state.config.task(&task_id)?;
    Ok(state
        .config
        .checkouts_of(&task_id)
        .into_iter()
        .map(|c| RepoBranchFacts {
            repo: state.config.project(&c.project_id).map(|p| p.name).unwrap_or_else(|_| "(unknown)".into()),
            facts: git::branch_facts(&PathBuf::from(&c.path), &task.branch, &c.base, c.base_commit.as_deref()),
            base: c.base,
            checkout_id: c.id,
        })
        .collect())
}

/// What this repository could open a pull request against.
#[tauri::command]
pub async fn checkout_branches(app: AppHandle, checkout_id: String) -> Result<Vec<String>> {
    super::blocking(app, move |state| {
        let checkout = state.config.checkout(&checkout_id)?;
        git::remote_branches(&PathBuf::from(&checkout.path))
    })
    .await
}

/// Move the open pull request for a checkout onto its current base.
#[tauri::command]
pub async fn github_retarget_pr(
    state: State<'_, AppState>,
    checkout_id: String,
    expected_url: String,
) -> Result<String> {
    let checkout = state.config.checkout(&checkout_id)?;
    let task = state.config.task(&checkout.task_id)?;
    let (origin, client) = super::github_sources::checkout_client(&state, &checkout.path).await?;
    let (owner, name) = origin.slug();
    let branches = super::pr_branches::branches(&checkout, &task.branch).await?;
    let pr = super::pr_branches::open_pull(&client, owner, name, &branches)
        .await?
        .ok_or_else(|| Error::NotFound(format!("an open PR for {}", task.branch)))?;
    super::github_sources::verify_pull_url(&expected_url, &pr.url)?;

    client.set_pull_base(owner, name, pr.number, &checkout.base).await?;
    Ok(format!("#{} now targets {}", pr.number, checkout.base))
}

/// Every task's rows in one sweep, for the watch that polls in the background.
#[derive(Debug, Serialize)]
pub struct TaskPrs {
    pub task_id: String,
    pub rows: Vec<CheckoutPr>,
    /// What was done about its ticket in this sweep (TKT-8).
    pub ticket: Option<super::TicketMove>,
}

/// PR state for every repository in the task, one row each.
#[tauri::command]
pub async fn github_task_prs(state: State<'_, AppState>, task_id: String) -> Result<Vec<CheckoutPr>> {
    task_prs(&state, &task_id).await
}

/// The same for every task, routing each checkout to its GitHub source.
///
/// A task whose rows cannot be read is left out rather than failing the sweep:
/// the watch runs unattended, and one unreadable repo must not blind the rest.
///
/// A recently settled task can be left out only when the window has rows for
/// it. The Henchman may have read it before a new window or after a reconnect.
#[tauri::command]
pub async fn github_all_prs(state: State<'_, AppState>, known_task_ids: HashSet<String>) -> Result<Vec<TaskPrs>> {
    let gate = github_gate(&state).await;
    let ids = sweep_ids(&state, &gate, Some(&known_task_ids)).await;

    // A few tasks at a time rather than one after another: with every task's
    // repos in series a sweep of a dozen tasks took longer than the interval
    // between sweeps. Not all at once either — GitHub throttles a client that
    // opens dozens of requests together.
    use futures_util::stream::{self, StreamExt};
    let state = &*state;
    let gate = &gate;
    // Tickets follow only with Jira connected; the sweep runs without it.
    let jira = super::jira_client(state).ok().map(|(j, _)| j);
    let jira = &jira;
    let out = stream::iter(ids)
        .map(|task_id| async move {
            let task = jira.as_ref().and_then(|_| state.config.task(&task_id).ok());
            let rows = task_prs(state, &task_id).await.ok()?;
            let ticket = match (jira, task) {
                (Some(j), Some(task)) => super::ticket_flow::follow(state, gate, j, &task, &rows).await,
                _ => None,
            };
            Some(TaskPrs { task_id, rows, ticket })
        })
        .buffered(SWEEP_TASKS)
        .filter_map(|t| async move { t })
        .collect::<Vec<_>>()
        .await;
    // The Henchman's own sweep waits while the window's runs (HEN-12).
    super::henchman_watch::swept(state, gate).await;
    Ok(out)
}

/// The tasks a sweep reads: every one but a review task left from 0.5.0,
/// which has no pull request of its own on its branch and whose ticket is
/// someone else's to move. Only the window needs rows it has not seen yet.
pub(crate) async fn sweep_ids(
    state: &AppState,
    gate: &GitHubGate,
    known_task_ids: Option<&HashSet<String>>,
) -> Vec<String> {
    let mut session = state.github_session.lock().await;
    if session.generation != gate.generation {
        return Vec::new();
    }
    let ids: Vec<String> =
        state.config.with(|c| c.tasks.iter().filter(|t| t.review.is_none()).map(|t| t.id.clone()).collect());
    session.settled.retain(|id, _| ids.contains(id));
    due(ids, &session.settled, std::time::Instant::now(), known_task_ids)
}

/// How many tasks the PR sweep asks GitHub about at once.
pub(crate) const SWEEP_TASKS: usize = 4;

/// How often the sweep asks about a task whose pull requests were all
/// closed or merged. Every task kept was swept every 90 seconds, done ones
/// included: a `git diff`, an `ls-files` walk of the worktree and a GitHub
/// call per repository, forever, and the app got slower the more tasks it
/// kept. A finished PR is reopened rarely, and the PR tab still asks at once.
const SETTLED_SWEEP: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// A settled task is read only when due, unless the window lacks its rows.
fn due(
    ids: Vec<String>,
    settled: &HashMap<String, std::time::Instant>,
    now: std::time::Instant,
    known_task_ids: Option<&HashSet<String>>,
) -> Vec<String> {
    ids.into_iter()
        .filter(|id| {
            known_task_ids.is_some_and(|known| !known.contains(id))
                || settled.get(id).is_none_or(|at| now.duration_since(*at) >= SETTLED_SWEEP)
        })
        .collect()
}

/// Whether a task's rows are settled: some pull request, and every repo's
/// newest one closed or merged, or none and nothing on its branch to open
/// one for. A row that could not be read is not settled.
fn settled(rows: &[CheckoutPr]) -> bool {
    rows.iter().any(|r| r.pr.is_some())
        && rows.iter().all(|r| {
            r.error.is_none()
                && match &r.pr {
                    Some(pr) => pr.state != "open",
                    None => r.changed == 0,
                }
        })
}

/// Pull requests waiting on the signed-in user, and on the configured team.
#[derive(Debug, Serialize)]
pub struct ReviewQueue {
    pub sources: Vec<SourceReviews>,
}

#[derive(Debug, Serialize)]
pub struct SourceReviews {
    pub source: String,
    #[serde(skip)]
    pub(crate) revision: Option<uuid::Uuid>,
    pub mine: Vec<github::ReviewRequest>,
    /// GitHub had more personal hits than the one page we asked for.
    pub mine_more: bool,
    pub mine_error: Option<String>,
    /// Absent when no review team is configured. Present, with `error` set,
    /// when the team was configured but could not be resolved or searched —
    /// the personal list is still worth showing.
    pub team: Option<TeamReviews>,
    /// The other side: pull requests you opened (REV-4). Only the view asks
    /// for them; the watch that banners new requests has no use for them.
    pub authored: Option<AuthoredPrs>,
}

#[derive(Debug, Serialize)]
pub struct AuthoredPrs {
    pub prs: Vec<github::AuthoredPr>,
    pub more: bool,
    /// Its own error, as the team's: it does not take the review lists down.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TeamReviews {
    /// `org/slug`.
    pub slug: String,
    pub name: String,
    pub prs: Vec<github::ReviewRequest>,
    pub more: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn github_review_queue(app: AppHandle) -> Result<ReviewQueue> {
    let state = app.state::<AppState>();
    let gate = github_gate(&state).await;
    let queue = review_queue(&state, &gate, true).await?;
    crate::news::saw_reviews(app.clone(), gate.reviews_generation, queue).await
}

/// A failure here is a note on its own section (REV-5), never the queue's.
async fn authored_prs(client: &GitHub, source: &str) -> AuthoredPrs {
    match client.authored_prs().await {
        Ok((mut prs, more)) => {
            for pr in &mut prs {
                pr.source = source.to_string();
            }
            AuthoredPrs { prs, more, error: None }
        }
        Err(e) => AuthoredPrs { prs: Vec::new(), more: false, error: Some(e.to_string()) },
    }
}

/// The queue itself, for the UI and for the watch that banners what is new.
pub(crate) async fn review_queue(state: &AppState, gate: &GitHubGate, with_authored: bool) -> Result<ReviewQueue> {
    use futures_util::stream::{self, StreamExt};
    let cfg = state.config.read();
    let sources = off_runtime(move || {
        cfg.github_source_list()
            .into_iter()
            .filter(|source| source.status != "disconnected")
            .map(|source| {
                let route = super::github_sources::client_for(&cfg, &source.key).map(|(client, _)| client);
                (source, route)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let sources = stream::iter(sources)
        .map(|(source, client)| source_reviews(state, gate, source, client, with_authored))
        .buffered(SWEEP_TASKS)
        .collect()
        .await;
    Ok(ReviewQueue { sources })
}

async fn source_reviews(
    state: &AppState,
    gate: &GitHubGate,
    source: GithubSource,
    client: Result<GitHub>,
    with_authored: bool,
) -> SourceReviews {
    let mut result = SourceReviews {
        source: source.key.clone(),
        revision: source.revision,
        mine: Vec::new(),
        mine_more: false,
        mine_error: None,
        team: None,
        authored: None,
    };
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            let error = error.to_string();
            result.mine_error = Some(error.clone());
            if with_authored {
                result.authored = Some(AuthoredPrs { prs: Vec::new(), more: false, error: Some(error) });
            }
            return result;
        }
    };
    let team = async {
        match source.config.review_team.as_deref() {
            Some(raw) => Some(load_team_reviews(state, gate, &client, &source, raw).await),
            None => None,
        }
    };
    let authored = async {
        if with_authored {
            Some(authored_prs(&client, &source.key).await)
        } else {
            None
        }
    };
    let (mine, team, authored) = tokio::join!(client.requested_prs(github::mine_review_query(), None), team, authored);
    match mine {
        Ok((mut prs, more)) => {
            for pr in &mut prs {
                pr.source = source.key.clone();
            }
            result.mine = prs;
            result.mine_more = more;
        }
        Err(e) => result.mine_error = Some(e.to_string()),
    }
    result.team = team.map(|mut t| {
        github::drop_already_listed(&mut t.prs, &result.mine);
        for pr in &mut t.prs {
            pr.source = source.key.clone();
        }
        t
    });
    result.authored = authored;
    result
}

/// The team column must not take the personal one down with it: a missing
/// `read:org` scope, or a slug that matches nothing, is a problem with that
/// column alone.
async fn load_team_reviews(
    state: &AppState,
    gate: &GitHubGate,
    client: &GitHub,
    source: &GithubSource,
    raw: &str,
) -> TeamReviews {
    let fallback = raw.trim_start_matches('@').to_string();
    // A bare slug may walk up to ten pages of /user/teams; reuse it only
    // for the connection and team preference it was resolved for.
    let cached = {
        let session = state.github_session.lock().await;
        (session.reviews_generation == gate.reviews_generation)
            .then(|| {
                session.resolved_team.get(&source.key).filter(|(setting, _)| setting == raw).map(|(_, t)| t.clone())
            })
            .flatten()
    };
    let team = match cached {
        Some(team) => team,
        None => match client.resolve_team(raw).await {
            Ok(team) => {
                let mut session = state.github_session.lock().await;
                if session.reviews_generation == gate.reviews_generation {
                    session.resolved_team.insert(source.key.clone(), (raw.to_string(), team.clone()));
                }
                team
            }
            Err(e) => {
                return TeamReviews {
                    slug: fallback.clone(),
                    name: fallback,
                    prs: Vec::new(),
                    more: false,
                    error: Some(e.to_string()),
                };
            }
        },
    };
    let slug = format!("{}/{}", team.org, team.slug);
    match client.requested_prs(&github::team_review_query(&team.org, &team.slug), Some(&team.slug)).await {
        Ok((prs, more)) => TeamReviews { slug, name: team.name, prs, more, error: None },
        Err(e) => TeamReviews { slug, name: team.name, prs: Vec::new(), more: false, error: Some(e.to_string()) },
    }
}

pub(crate) async fn task_prs(state: &AppState, task_id: &str) -> Result<Vec<CheckoutPr>> {
    read_task_prs(state, task_id, false).await
}

pub(super) async fn read_task_prs(state: &AppState, task_id: &str, finish_preflight: bool) -> Result<Vec<CheckoutPr>> {
    // Before any client is made: a reconnect after this drops what we read.
    let gate = github_gate(state).await;
    let task = state.config.task(task_id)?;
    let checkouts = state.config.checkouts_of(task_id);

    // What git knows, for every repo, first and off the async workers: these
    // are subprocesses, and on the runtime they held up the MCP server and
    // every other request sharing it.
    let local = {
        let checkouts = checkouts.clone();
        let task_branch = task.branch.clone();
        let cfg = state.config.read();
        off_runtime(move || {
            checkouts
                .iter()
                .map(|c| {
                    let dir = PathBuf::from(&c.path);
                    // A count, not the list: this runs for every repository of
                    // every task on a timer, and building the list reads each
                    // untracked file.
                    let changed = git::changed_count(&dir, &c.base, c.base_commit.as_deref(), git::Scope::Branch);
                    let branches = super::pr_branches::branches_of(c, &task_branch);
                    let origin =
                        if finish_preflight { git::github_origin(&dir) } else { git::github_origin_cached(&dir) };
                    let source = origin.as_ref().ok().map(|r| r.host.clone());
                    let route = origin.and_then(|remote| {
                        super::github_sources::client_for_repo(&cfg, &remote).map(|(client, _)| (remote, client))
                    });
                    (changed, source, route, branches)
                })
                .collect::<Vec<_>>()
        })
        .await?
    };

    // Then GitHub, for every repo at once.
    let rows = checkouts.into_iter().zip(local).map(|(checkout, (changed, source, route, branches))| {
        async move {
            let repo =
                state.config.project(&checkout.project_id).map(|p| p.name).unwrap_or_else(|_| "(unknown)".into());

            let mut row = CheckoutPr {
                checkout_id: checkout.id.clone(),
                repo,
                source,
                pr: None,
                checks: Vec::new(),
                checks_state: ReadState::Unavailable,
                reviews: Vec::new(),
                past: Vec::new(),
                verdict: "none".into(),
                base: checkout.base.clone(),
                changed,
                to_reply: 0,
                error: None,
            };

            // One repo without a GitHub remote should not fail the whole view.
            match route {
                Ok((remote, client)) => {
                    let (owner, name) = remote.slug();
                    match super::pr_branches::pulls(&client, owner, name, &branches).await {
                        Ok(mut all) if !all.is_empty() => {
                            // The open one is the one still being decided. With
                            // none open, the newest says what became of the branch.
                            let at = all.iter().position(|p| p.state == "open").unwrap_or(0);
                            // What merged moves the branch point (UPD-6). Counted
                            // from where the branch was cut, a repo whose PR had
                            // merged never went back to zero, so a task with one
                            // repo merged and another in review read "partly up
                            // for review, one repo still without a PR".
                            if let Some(pr) = super::landed(&all).filter(|_| !finish_preflight) {
                                if let Ok(Some(point)) = super::advance(state, &checkout, &pr.head_sha).await {
                                    let (dir, base) = (PathBuf::from(&checkout.path), checkout.base.clone());
                                    if let Ok(n) = off_runtime(move || {
                                        git::changed_count(&dir, &base, Some(&point), git::Scope::Branch)
                                    })
                                    .await
                                    {
                                        row.changed = n;
                                    }
                                }
                            }
                            let found = all.remove(at);

                            if found.state == "open" {
                                let number = found.number;
                                let details = async {
                                    // The listing has no comment counts. The
                                    // fuller read may also see a push, so only
                                    // ask about the head we will display.
                                    let pr = client
                                        .pull(owner, name, number)
                                        .await
                                        .ok()
                                        .filter(|pr| {
                                            pr.number == number && matches!(pr.state.as_str(), "open" | "closed")
                                        })
                                        .unwrap_or(found);
                                    let checks = if pr.state == "open" {
                                        client.pr_checks(owner, name, &pr.head_sha).await
                                    } else {
                                        github::PrChecks::default()
                                    };
                                    (pr, checks)
                                };
                                let ((pr, checks), reviews, to_reply) = tokio::join!(
                                    details,
                                    client.reviews(owner, name, number),
                                    client.pr_to_reply(owner, name, number),
                                );

                                row.checks = checks.checks;
                                row.checks_state = checks.checks_state;
                                row.reviews = reviews.unwrap_or_default();
                                row.verdict = github::verdict(&row.reviews).to_string();
                                row.to_reply = to_reply.unwrap_or(0);
                                row.pr = Some(pr);
                            } else {
                                // Closed is final: its checks and reviews will not
                                // change, and nothing on screen reads them. Three
                                // calls saved per finished repository, on every
                                // sweep, for as long as the task is kept — which
                                // is what keeps a dozen done tasks from eating
                                // the API budget the open ones need.
                                row.pr = Some(found);
                            }
                            row.past = all;
                        }
                        Ok(_) => {}
                        Err(e) => row.error = Some(e.to_string()),
                    }
                }
                Err(e) => row.error = Some(e.to_string()),
            }
            row
        }
    });
    let rows = futures_util::future::join_all(rows).await;
    if finish_preflight {
        return Ok(rows);
    }
    let mut session = state.github_session.lock().await;
    // An old sweep can finish after reconnect, even if this was its first
    // read of the task. It cannot set the new session's pace or baseline.
    if session.generation != gate.generation {
        return Ok(rows);
    }
    if settled(&rows) {
        session.settled.insert(task_id.to_string(), std::time::Instant::now());
    } else {
        session.settled.remove(task_id);
    }
    let before = state.prs_seen.lock().insert(task_id.to_string(), rows.clone());
    // The Henchman hears what changed since the read before (HEN-12).
    if let Some(before) = before {
        super::henchman_watch::seen(state, task_id, &before, &rows);
    }
    Ok(rows)
}

/// What reviewers and CI have said on one repository's open pull request.
#[derive(Debug, Serialize)]
pub struct RepoFeedback {
    pub checkout_id: String,
    pub repo: String,
    pub source: Option<String>,
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(flatten)]
    pub feedback: github::PrFeedback,
    pub checks: Vec<github::FailedCheck>,
    pub error: Option<String>,
}

/// How many failed jobs' logs are read per repository. A matrix build that
/// fails everywhere fails for one reason, and forty logs say it forty times.
const LOGS_PER_REPO: usize = 6;

/// Everything said on the task's open pull requests, for picking what to
/// hand the agent.
#[tauri::command]
pub async fn github_pr_feedback(state: State<'_, AppState>, task_id: String) -> Result<Vec<RepoFeedback>> {
    let task = state.config.task(&task_id)?;
    let checkouts = state.config.checkouts_of(&task_id);

    let rows = checkouts.into_iter().map(|checkout| {
        let (task, state) = (&task, &*state);
        async move {
            let repo =
                state.config.project(&checkout.project_id).map(|p| p.name).unwrap_or_else(|_| "(unknown)".into());
            let mut source = None;
            let found: Result<Option<RepoFeedback>> = async {
                let (origin, client) = super::github_sources::checkout_client(state, &checkout.path).await?;
                source = Some(origin.host.clone());
                let (owner, name) = origin.slug();
                let branches = super::pr_branches::branches(&checkout, &task.branch).await?;
                let Some(pr) = super::pr_branches::open_pull(&client, owner, name, &branches).await? else {
                    return Ok(None);
                };
                // The commit GitHub has, not the local branch: what was checked
                // is what was pushed, and local commits since have no runs yet.
                let at = if pr.head_sha.is_empty() { task.branch.clone() } else { pr.head_sha.clone() };
                let (feedback, checks) =
                    tokio::join!(client.pr_feedback(owner, name, pr.number), client.failed_checks(owner, name, &at),);
                let mut checks = checks.unwrap_or_default();
                // Counted among the checks that have a log to read: six failing
                // external checks listed first left the Actions job without one.
                let jobs: Vec<(usize, u64)> = checks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| c.job_id.map(|id| (i, id)))
                    .take(LOGS_PER_REPO)
                    .collect();
                let logs = futures_util::future::join_all(jobs.iter().map(|&(_, id)| {
                    let client = &client;
                    async move { client.job_log_tail(owner, name, id).await.ok() }
                }))
                .await;
                for (&(i, _), log) in jobs.iter().zip(logs) {
                    checks[i].log = log.filter(|l| !l.is_empty());
                }
                Ok(Some(RepoFeedback {
                    checkout_id: checkout.id.clone(),
                    repo: repo.clone(),
                    source: source.clone(),
                    number: pr.number,
                    title: pr.title,
                    url: pr.url,
                    feedback: feedback?,
                    checks,
                    error: None,
                }))
            }
            .await;
            match found {
                Ok(row) => row,
                // Said on the row, so one repository GitHub will not answer
                // for does not hide what the others' reviewers wrote.
                Err(e) => Some(RepoFeedback {
                    checkout_id: checkout.id,
                    repo,
                    source,
                    number: 0,
                    title: String::new(),
                    url: String::new(),
                    feedback: Default::default(),
                    checks: Vec::new(),
                    error: Some(e.to_string()),
                }),
            }
        }
    });
    Ok(futures_util::future::join_all(rows).await.into_iter().flatten().collect())
}

/// One piece of feedback chosen to hand to the agent, sent back as shown.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeedbackItem {
    Thread {
        checkout_id: String,
        path: String,
        line: Option<u64>,
        #[serde(default)]
        start_line: Option<u64>,
        #[serde(default)]
        code: Vec<CodeLine>,
        #[serde(default)]
        outdated: bool,
        #[serde(default)]
        resolved: bool,
        url: String,
        comments: Vec<FeedbackNote>,
    },
    Review {
        checkout_id: String,
        author: String,
        state: Option<String>,
        body: String,
        url: String,
    },
    Comment {
        checkout_id: String,
        author: String,
        body: String,
        url: String,
    },
    Check {
        checkout_id: String,
        name: String,
        conclusion: String,
        url: Option<String>,
        #[serde(default)]
        summary: String,
        log: Option<String>,
    },
}

impl FeedbackItem {
    fn checkout_id(&self) -> &str {
        match self {
            Self::Thread { checkout_id, .. }
            | Self::Review { checkout_id, .. }
            | Self::Comment { checkout_id, .. }
            | Self::Check { checkout_id, .. } => checkout_id,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FeedbackNote {
    pub author: String,
    pub body: String,
}

/// A body as a Markdown quote, so a reviewer's own headings and lists stay
/// inside their comment instead of restructuring the file around it.
fn quoted(body: &str) -> String {
    body.trim()
        .lines()
        .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The feedback as a file an agent can work through.
///
/// `scope` is the checkout the agent is sitting in. Paths are qualified with
/// their repo everywhere else, so `api/src/auth.ts:42` is never ambiguous from
/// the task root.
pub(crate) fn feedback_markdown(
    branch: &str,
    items: &[FeedbackItem],
    scope: Option<&str>,
    repo_name: impl Fn(&str) -> String,
) -> String {
    let mut md = format!("# Feedback on the pull requests for `{branch}`\n\n");
    md.push_str(
        "Collected from GitHub by Layer. Quoted text is what reviewers and CI said; \
         none of it has been answered on GitHub yet.\n",
    );

    let mut order: Vec<&str> = Vec::new();
    for item in items {
        if !order.contains(&item.checkout_id()) {
            order.push(item.checkout_id());
        }
    }
    for checkout in order {
        let repo = repo_name(checkout);
        let mine: Vec<&FeedbackItem> = items.iter().filter(|i| i.checkout_id() == checkout).collect();
        md.push_str(&format!("\n## {repo}\n"));
        if scope.is_some_and(|s| s != checkout) {
            md.push_str(&format!("\nThis is a sibling repository: `../{repo}` from where you are.\n"));
        }

        let threads: Vec<_> = mine.iter().filter(|i| matches!(i, FeedbackItem::Thread { .. })).collect();
        if !threads.is_empty() {
            md.push_str("\n### Review threads\n");
            for item in threads {
                let FeedbackItem::Thread { path, line, start_line, code, outdated, resolved, url, comments, .. } = item
                else {
                    continue;
                };
                let at = if scope == Some(checkout) { path.clone() } else { format!("{repo}/{path}") };
                let at = match (start_line, line) {
                    (Some(a), Some(b)) if a < b => format!("{at}:{a}-{b}"),
                    (_, Some(n)) => format!("{at}:{n}"),
                    _ => at,
                };
                md.push_str(&format!("\n#### `{at}`\n"));
                if *resolved {
                    md.push_str("\nResolved on GitHub: the reviewer accepted it as it stands. It was sent on purpose, so read it, but do not undo what was settled.\n");
                }
                if *outdated {
                    md.push_str(
                        "\nThe code here has changed since this was written — check whether it still applies.\n",
                    );
                }
                if !code.is_empty() {
                    let diff: Vec<String> = code.iter().map(|c| format!("{}{}", c.op, c.text)).collect();
                    // Longer than any run of backticks in the code, so the code cannot close it.
                    let fence = "`".repeat(diff.iter().map(|l| l.matches('`').count()).max().unwrap_or(0).max(2) + 1);
                    md.push_str(&format!("\nThe code it was left on:\n\n{fence}diff\n{}\n{fence}\n", diff.join("\n")));
                }
                for c in comments {
                    md.push_str(&format!("\n**{}**:\n{}\n", c.author, quoted(&c.body)));
                }
                md.push_str(&format!("\n{url}\n"));
            }
        }

        let said: Vec<_> =
            mine.iter().filter(|i| matches!(i, FeedbackItem::Review { .. } | FeedbackItem::Comment { .. })).collect();
        if !said.is_empty() {
            md.push_str("\n### On the pull request as a whole\n");
            for item in said {
                let (author, verb, body, url) = match item {
                    FeedbackItem::Review { author, state, body, url, .. } => (
                        author,
                        match state.as_deref() {
                            Some("CHANGES_REQUESTED") => "requested changes",
                            Some("APPROVED") => "approved, adding",
                            _ => "reviewed",
                        },
                        body,
                        url,
                    ),
                    FeedbackItem::Comment { author, body, url, .. } => (author, "commented", body, url),
                    _ => continue,
                };
                md.push_str(&format!("\n**{author}** {verb}:\n{}\n\n{url}\n", quoted(body)));
            }
        }

        let checks: Vec<_> = mine.iter().filter(|i| matches!(i, FeedbackItem::Check { .. })).collect();
        if !checks.is_empty() {
            md.push_str("\n### Failing checks\n");
            for item in checks {
                let FeedbackItem::Check { name, conclusion, url, summary, log, .. } = item else { continue };
                md.push_str(&format!("\n#### {name} — {}\n", conclusion.replace('_', " ")));
                if let Some(url) = url {
                    md.push_str(&format!("\n{url}\n"));
                }
                if !summary.trim().is_empty() {
                    md.push_str(&format!("\nWhat the check reported:\n\n```\n{}\n```\n", summary.trim()));
                }
                match log {
                    Some(log) if !log.trim().is_empty() => md.push_str(&format!(
                        "\nThe end of its log, up to where it failed:\n\n```\n{}\n```\n",
                        log.trim()
                    )),
                    _ if summary.trim().is_empty() => {
                        md.push_str("\nNo log could be read — open the link, or run the same step locally.\n")
                    }
                    _ => {}
                }
            }
        }
    }
    md
}

/// "3 review threads, 1 comment and 2 failing checks".
fn feedback_summary(items: &[FeedbackItem]) -> String {
    let count = |f: fn(&FeedbackItem) -> bool| items.iter().filter(|i| f(i)).count();
    let parts: Vec<String> = [
        (count(|i| matches!(i, FeedbackItem::Thread { .. })), "review thread"),
        (count(|i| matches!(i, FeedbackItem::Review { .. } | FeedbackItem::Comment { .. })), "comment"),
        (count(|i| matches!(i, FeedbackItem::Check { .. })), "failing check"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}{}", if n == 1 { "" } else { "s" }))
    .collect();
    match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Hand the chosen feedback to an agent.
///
/// Written to a file beside the worktrees and pointed at, not typed in: a
/// failing build's log is a hundred lines, and pasted into a TUI it arrives as
/// "[Pasted text +140 lines]" at best and as a hundred submissions at worst.
///
/// With `pane_id`, the prompt is typed into that agent. Without, it is only
/// returned, for starting a new agent with it — `scope` then says where that
/// agent will run.
#[tauri::command]
pub fn send_pr_feedback(
    state: State<AppState>,
    task_id: String,
    pane_id: Option<String>,
    scope: Option<String>,
    items: Vec<FeedbackItem>,
) -> Result<String> {
    if items.is_empty() {
        return Err(Error::Other("nothing chosen to send".into()));
    }
    let task = state.config.task(&task_id)?;
    let scope = match &pane_id {
        Some(id) => state.ptys.info(id)?.checkout_id,
        None => scope,
    };
    let md = feedback_markdown(&task.branch, &items, scope.as_deref(), |id| {
        state
            .config
            .checkout(id)
            .and_then(|c| state.config.project(&c.project_id))
            .map(|p| p.name)
            .unwrap_or_else(|_| "(unknown repo)".into())
    });

    let ask = "Work through every point. Fix what is right; where you think a reviewer is \
               wrong, say so and why instead of quietly skipping it. For a failing check, \
               find the cause before changing anything, and reproduce it locally if you can. \
               When you are done, go through the points one by one and say what you did \
               about each.";
    let head = format!(
        "Reviewers and CI have come back on the pull requests for `{}`: {}.",
        task.branch,
        feedback_summary(&items),
    );
    let prompt = match agent_file_dir(&state, &task) {
        Some(dir) => {
            let path = dir.join(FEEDBACK_FILE);
            // What to do goes in the file with the points: typed, the two
            // together were long enough to lose their start (`pty::MAX_TYPED`).
            std::fs::write(&path, format!("{head}\n\n{ask}\n\n{md}"))?;
            format!("{head} It is all in {}. Read that file, then work through every point.", path.display())
        }
        // A task from the one-repo layout has no folder outside its worktree,
        // and a file in the worktree is one the agent might commit.
        None => format!("{head}\n\n{md}\n\n{ask}"),
    };

    if let Some(id) = &pane_id {
        super::hand_over(&state, &task, id, FEEDBACK_FILE, &prompt)?;
    }
    Ok(prompt)
}

pub(crate) const FEEDBACK_FILE: &str = "PR_FEEDBACK.md";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GithubConfig;
    use axum::{
        extract::{Path, Query},
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    mod reconnect;

    fn thread(checkout: &str, path: &str, line: Option<u64>) -> FeedbackItem {
        FeedbackItem::Thread {
            checkout_id: checkout.into(),
            path: path.into(),
            line,
            start_line: None,
            code: Vec::new(),
            outdated: false,
            resolved: false,
            url: "https://gh/t".into(),
            comments: vec![FeedbackNote { author: "ana".into(), body: "Off by one?\n\n- really".into() }],
        }
    }

    fn check(checkout: &str) -> FeedbackItem {
        FeedbackItem::Check {
            checkout_id: checkout.into(),
            name: "test".into(),
            conclusion: "timed_out".into(),
            url: None,
            summary: String::new(),
            log: Some("ERROR: boom".into()),
        }
    }

    fn row(state: Option<&str>, changed: usize) -> CheckoutPr {
        let pr = state.map(|s| github::PullRequest {
            number: 1,
            title: "t".into(),
            state: s.into(),
            draft: false,
            author: "me".into(),
            head: "b".into(),
            head_sha: String::new(),
            base: "main".into(),
            url: String::new(),
            mergeable_state: None,
            merged: s == "closed",
            comments: 0,
            review_comments: 0,
        });
        CheckoutPr {
            checkout_id: "c".into(),
            repo: "api".into(),
            source: None,
            pr,
            checks: Vec::new(),
            checks_state: ReadState::Unavailable,
            reviews: Vec::new(),
            past: Vec::new(),
            verdict: "none".into(),
            base: "main".into(),
            changed,
            to_reply: 0,
            error: None,
        }
    }

    #[test]
    fn a_task_is_settled_only_when_every_repo_is_done_with_its_pull_requests() {
        assert!(settled(&[row(Some("closed"), 3), row(None, 0)]), "merged, and a repo with nothing to open");
        assert!(!settled(&[row(Some("closed"), 0), row(Some("open"), 2)]), "one still in review");
        assert!(!settled(&[row(Some("closed"), 0), row(None, 4)]), "a repo with work and no PR yet");
        assert!(!settled(&[row(None, 0)]), "no PR at all: one may be opened any moment");
        let mut failed = row(Some("closed"), 0);
        failed.error = Some("rate limited".into());
        assert!(!settled(&[failed]), "what could not be read is not known to be done");
    }

    #[test]
    fn a_settled_task_is_swept_again_only_after_a_quarter_of_an_hour() {
        let now = std::time::Instant::now();
        let ids = || vec!["open".to_string(), "done".to_string(), "done-long-ago".to_string()];
        let mut settled = HashMap::new();
        settled.insert("done".to_string(), now - std::time::Duration::from_secs(60));
        settled.insert("done-long-ago".to_string(), now - SETTLED_SWEEP);
        assert_eq!(due(ids(), &settled, now, None), vec!["open".to_string(), "done-long-ago".to_string()]);
    }

    fn name(id: &str) -> String {
        if id == "c1" {
            "api".into()
        } else {
            "web".into()
        }
    }

    #[test]
    fn paths_are_bare_only_inside_their_own_repo() {
        let items = vec![thread("c1", "src/a.ts", Some(42)), thread("c2", "src/b.ts", None)];

        let from_root = feedback_markdown("ACME-1", &items, None, name);
        assert!(from_root.contains("`api/src/a.ts:42`"));
        assert!(from_root.contains("`web/src/b.ts`"));

        let from_api = feedback_markdown("ACME-1", &items, Some("c1"), name);
        assert!(from_api.contains("`src/a.ts:42`"));
        assert!(from_api.contains("`web/src/b.ts`"));
        assert!(from_api.contains("`../web` from where you are"));
    }

    #[test]
    fn a_reviewers_markdown_stays_inside_the_quote() {
        let md = feedback_markdown("ACME-1", &[thread("c1", "a", Some(1))], None, name);
        assert!(md.contains("> Off by one?\n>\n> - really"));
    }

    #[test]
    fn a_thread_on_a_range_names_the_range_and_carries_its_code() {
        let mut item = thread("c1", "a.scss", Some(20));
        if let FeedbackItem::Thread { start_line, code, .. } = &mut item {
            *start_line = Some(19);
            *code = vec![
                CodeLine { n: Some(19), op: "+".into(), text: ".id {".into() },
                CodeLine { n: Some(20), op: "+".into(), text: "  content: \"```\";".into() },
            ];
        }
        let md = feedback_markdown("ACME-1", &[item], None, name);
        assert!(md.contains("#### `api/a.scss:19-20`"));
        // A fence the code's own backticks cannot close.
        assert!(md.contains("````diff\n+.id {\n+  content: \"```\";\n````"));
    }

    #[test]
    fn a_resolved_thread_sent_on_purpose_says_it_was_settled() {
        let mut item = thread("c1", "a.ts", Some(3));
        if let FeedbackItem::Thread { resolved, .. } = &mut item {
            *resolved = true;
        }
        assert!(feedback_markdown("ACME-1", &[item], None, name).contains("Resolved on GitHub"));
        assert!(!feedback_markdown("ACME-1", &[thread("c1", "a.ts", Some(3))], None, name).contains("Resolved"));
    }

    #[test]
    fn a_check_says_what_it_reported_and_where_it_failed() {
        let md = feedback_markdown("ACME-1", &[check("c1")], None, name);
        assert!(md.contains("#### test — timed out"));
        assert!(md.contains("```\nERROR: boom\n```"));
    }

    #[test]
    fn the_summary_counts_in_words() {
        assert_eq!(feedback_summary(&[check("c1")]), "1 failing check");
        assert_eq!(
            feedback_summary(&[thread("c1", "a", None), thread("c1", "b", None), check("c1")]),
            "2 review threads and 1 failing check"
        );
    }

    #[test]
    fn items_arrive_tagged_by_kind() {
        let items: Vec<FeedbackItem> = serde_json::from_value(serde_json::json!([
            { "kind": "comment", "checkout_id": "c1", "author": "bo", "body": "hi", "url": "u" },
            { "kind": "check", "checkout_id": "c1", "name": "lint", "conclusion": "failure", "url": null, "log": null }
        ]))
        .unwrap();
        assert!(matches!(items[0], FeedbackItem::Comment { .. }));
        assert!(matches!(items[1], FeedbackItem::Check { .. }));
    }

    const LIST_HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FULL_HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn check_run(id: u64, sha: &str, name: &str, conclusion: &str) -> Value {
        json!({
            "id": id, "head_sha": sha, "name": name, "status": "completed", "conclusion": conclusion,
            "html_url": format!("https://gh.example/acme/api/runs/{id}")
        })
    }

    fn check_page(total: u64, runs: Vec<Value>) -> (StatusCode, Json<Value>) {
        (StatusCode::OK, Json(json!({ "total_count": total, "check_runs": runs })))
    }

    fn check_page_of_passing(start: u64) -> Vec<Value> {
        (start..start + 100).map(|i| check_run(i, LIST_HEAD, "build", "success")).collect()
    }

    async fn read_task_pr_checks(
        listing_head: &str,
        detail_head: &str,
        page: impl Fn(&str, u32) -> (StatusCode, Json<Value>) + Send + Sync + 'static,
    ) -> (Value, Vec<(String, u32)>) {
        let dir = std::env::temp_dir().join(format!("vl-task-pr-checks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        git::run_for_tests(&dir, &["init", "-q", "-b", "task"]).unwrap();
        git::run_for_tests(&dir, &["remote", "add", "origin", "git@gh.example:acme/api.git"]).unwrap();
        let mut cfg: crate::config::AppConfig = serde_json::from_value(json!({
            "projects": [{"id": "p", "name": "api", "path": dir, "default_branch": "main"}],
            "tasks": [{"id": "t", "name": "task", "root": dir, "branch": "task", "created_at": "2026-11-01T12:00:00Z"}],
            "checkouts": [{"id": "c", "task_id": "t", "project_id": "p", "path": dir, "base": "main"}]
        }))
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source = GithubConfig {
            api_url: format!("http://{}", listener.local_addr().unwrap()),
            web_url: "https://gh.example".into(),
            review_team: None,
        };
        let credential = crate::config::github::GithubCredential::new(&source).unwrap();
        credential.save("gh.example", "test-token").unwrap();
        cfg.github_sources.credentials.insert("gh.example".into(), credential.clone());
        cfg.github_sources.local.push(source);
        let state = AppState {
            config: crate::config::ConfigStore::for_tests(dir.join("config.json"), cfg),
            ptys: Default::default(),
            ticket_types: Default::default(),
            epic_field_missing: Default::default(),
            pending_notices: Default::default(),
            status_cache: Default::default(),
            github_session: Default::default(),
            prs_seen: Default::default(),
            news: Default::default(),
            messages: crate::messages::Messages::for_tests(dir.join("messages.json")),
            notes: crate::notes::Notes::load(&dir),
            browser: Default::default(),
            loops: Default::default(),
            runs: crate::runs::Runs::for_tests(dir.join("runs.json")),
            slack_bot: Default::default(),
        };
        let pr = |sha| {
            json!({
                "number": 42, "title": "CI read", "state": "open", "draft": false, "user": {"login": "ana"},
                "head": {"ref": "task", "sha": sha}, "base": {"ref": "main"},
                "html_url": "https://gh.example/acme/api/pull/42"
            })
        };
        let listed = json!([pr(listing_head)]);
        let detail = pr(detail_head);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let page = Arc::new(page);
        let app = Router::new()
            .route("/repos/acme/api/pulls", get(move || async { Json(listed) }))
            .route("/repos/acme/api/pulls/42", get(move || async { Json(detail) }))
            .route(
                "/repos/acme/api/pulls/42/reviews",
                get(|| async {
                    Json(json!([{"id": 1, "user": {"login": "sam"}, "state": "APPROVED",
                        "submitted_at": "2026-11-01T12:00:00Z", "html_url": "https://gh.example/acme/api/pull/42#review"}]))
                }),
            )
            .route("/graphql", post(|| async { StatusCode::FORBIDDEN }))
            .route(
                "/repos/acme/api/commits/{sha}/check-runs",
                get(move |Path(sha): Path<String>, Query(query): Query<HashMap<String, String>>| {
                    let recorded = Arc::clone(&recorded);
                    let page = Arc::clone(&page);
                    async move {
                        assert_eq!(query["per_page"], "100");
                        assert!(query.get("filter").is_none_or(|v| v == "latest"));
                        let n = query.get("page").map_or(1, |v| v.parse().unwrap());
                        recorded.lock().unwrap().push((sha.clone(), n));
                        page(&sha, n)
                    }
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let rows = task_prs(&state, "t").await.unwrap();
        server.abort();
        credential.delete("gh.example").unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        let calls = calls.lock().unwrap().clone();
        (json!(rows[0]), calls)
    }

    #[tokio::test]
    async fn task_pr_checks_a_successful_empty_page_is_complete_even_when_feedback_count_fails() {
        let (row, calls) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |_, _| check_page(0, vec![])).await;
        assert_eq!(row["checks"], json!([]));
        assert_eq!(row["checks_state"], "complete");
        assert_eq!(calls, [(LIST_HEAD.to_string(), 1)]);
    }

    #[tokio::test]
    async fn task_pr_checks_a_passing_duplicate_does_not_hide_a_failure_on_the_next_page() {
        let (row, calls) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |sha, n| match n {
            1 => check_page(101, check_page_of_passing(1)),
            2 => check_page(101, vec![check_run(101, sha, "build", "failure")]),
            _ => panic!("unexpected page {n}"),
        })
        .await;
        let checks = row["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 101);
        assert_eq!(checks[0]["conclusion"], "success");
        assert_eq!(
            checks[100],
            json!({ "name": "build", "status": "completed", "conclusion": "failure",
                "url": "https://gh.example/acme/api/runs/101" })
        );
        assert_eq!(row["checks_state"], "complete");
        assert_eq!(calls, [(LIST_HEAD.to_string(), 1), (LIST_HEAD.to_string(), 2)]);
    }

    #[tokio::test]
    async fn task_pr_checks_a_late_page_failure_keeps_the_checks_and_the_review_verdict() {
        for bad_page in [
            (StatusCode::FORBIDDEN, json!({ "message": "not allowed" })),
            (StatusCode::OK, json!({ "total_count": 101, "check_runs": null })),
        ] {
            let (row, calls) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, move |_, n| match n {
                1 => check_page(101, check_page_of_passing(1)),
                2 => (bad_page.0, Json(bad_page.1.clone())),
                _ => panic!("unexpected page {n}"),
            })
            .await;
            assert_eq!(row["checks"].as_array().unwrap().len(), 100);
            assert_eq!(row["checks_state"], "incomplete");
            assert_eq!(row["verdict"], "approved");
            assert!(row["error"].is_null());
            assert_eq!(calls.len(), 2);
        }
    }

    #[tokio::test]
    async fn task_pr_checks_an_unreadable_first_page_is_not_complete_empty() {
        for (status, body) in [
            (StatusCode::NOT_FOUND, json!({ "message": "commit not found" })),
            (StatusCode::OK, json!({ "total_count": 0 })),
            (StatusCode::OK, json!({ "check_runs": [] })),
            (StatusCode::OK, json!({ "total_count": "0", "check_runs": [] })),
            (StatusCode::OK, json!({ "total_count": 1, "check_runs": [] })),
        ] {
            let (row, _) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, move |_, _| (status, Json(body.clone()))).await;
            assert_eq!(row["checks"], json!([]));
            assert_eq!(row["checks_state"], "unavailable", "first page: {status}");
        }
    }

    #[tokio::test]
    async fn task_pr_checks_malformed_runs_do_not_hide_readable_ones_or_certify_the_page() {
        let (row, _) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |sha, _| {
            let mut malformed = check_run(2, sha, "missing conclusion", "success");
            malformed["conclusion"] = Value::Null;
            check_page(
                3,
                vec![check_run(1, sha, "kept", "success"), malformed, check_run(3, FULL_HEAD, "wrong SHA", "failure")],
            )
        })
        .await;
        assert_eq!(row["checks_state"], "incomplete");
        assert_eq!(row["checks"].as_array().unwrap().len(), 1);
        assert_eq!(row["checks"][0]["name"], "kept");
    }

    #[tokio::test]
    async fn task_pr_checks_a_malformed_link_does_not_hide_a_failure_with_a_passing_peer() {
        let (row, _) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |sha, _| {
            let mut failure = check_run(2, sha, "build", "failure");
            failure["html_url"] = json!(42);
            check_page(2, vec![check_run(1, sha, "build", "success"), failure])
        })
        .await;
        assert_eq!(
            row["checks"],
            json!([
                {"name": "build", "status": "completed", "conclusion": "success",
                    "url": "https://gh.example/acme/api/runs/1"},
                {"name": "build", "status": "completed", "conclusion": "failure", "url": null}
            ])
        );
        assert_eq!(row["checks_state"], "incomplete");
    }

    #[tokio::test]
    async fn task_pr_checks_missing_and_null_links_do_not_make_a_read_incomplete() {
        let (row, _) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |sha, _| {
            let mut missing = check_run(1, sha, "lint", "success");
            missing.as_object_mut().unwrap().remove("html_url");
            let mut null = check_run(2, sha, "build", "failure");
            null["html_url"] = Value::Null;
            check_page(2, vec![missing, null])
        })
        .await;
        assert_eq!(
            row["checks"],
            json!([
                {"name": "lint", "status": "completed", "conclusion": "success", "url": null},
                {"name": "build", "status": "completed", "conclusion": "failure", "url": null}
            ])
        );
        assert_eq!(row["checks_state"], "complete");
    }

    #[tokio::test]
    async fn task_pr_checks_only_the_head_on_the_displayed_pr_is_checked_after_a_push() {
        let (row, calls) = read_task_pr_checks(LIST_HEAD, FULL_HEAD, |sha, _| {
            let (name, conclusion) = if sha == FULL_HEAD { ("new head", "success") } else { ("old head", "failure") };
            check_page(1, vec![check_run(1, sha, name, conclusion)])
        })
        .await;
        assert_eq!(row["pr"]["head_sha"], FULL_HEAD);
        assert_eq!(row["checks"][0]["name"], "new head");
        assert_eq!(row["checks_state"], "complete");
        assert_eq!(calls, [(FULL_HEAD.to_string(), 1)]);
    }

    #[tokio::test]
    async fn task_pr_checks_a_missing_displayed_head_does_not_read_old_head_or_a_branch() {
        for listed in [LIST_HEAD, ""] {
            let (row, calls) = read_task_pr_checks(listed, "", |sha, _| {
                check_page(1, vec![check_run(1, sha, "not tied to displayed head", "success")])
            })
            .await;
            assert_eq!(row["pr"]["head_sha"], "");
            assert_eq!(row["checks"], json!([]));
            assert_eq!(row["checks_state"], "unavailable");
            assert!(calls.is_empty());
        }
    }

    #[tokio::test]
    async fn task_pr_checks_the_page_cap_keeps_available_rows_without_certifying_them() {
        let (row, calls) = read_task_pr_checks(LIST_HEAD, LIST_HEAD, |_, n| {
            check_page(1100, check_page_of_passing(u64::from(n - 1) * 100 + 1))
        })
        .await;
        assert_eq!(row["checks"].as_array().unwrap().len(), 1000);
        assert_eq!(row["checks_state"], "incomplete");
        assert_eq!(calls.len(), 10);
    }
}
