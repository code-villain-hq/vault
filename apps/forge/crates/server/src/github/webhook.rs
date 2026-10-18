//! GitHub webhooks: installations and their repositories, pull requests,
//! pushes and checks. Pull requests and commits that mention an issue
//! (`FOR-12` in a branch, title, body or message) are linked to it. Only the
//! issues a pull request works on, GitHub's way (named in its branch, or
//! after a closing word like "Fixes FOR-12"), move with it; their links say
//! so (`works_on`). Opening it puts them in review once it could be merged:
//! no conflicts, and its checks pass (checks that haven't started yet, in a
//! repository that runs them, count as running). Forge asks GitHub again when the
//! default branch moves or checks (check runs or commit statuses) change,
//! and sends an issue in review back when its pull request runs into
//! conflicts or failing checks. Merging it completes them; closing it without
//! merging, or turning it back into a draft, sends an issue waiting in review
//! back to work. A commit only links: work reaches Done through a pull
//! request's review, not by pushing "Fixes FOR-12".

use std::collections::BTreeSet;
use std::sync::LazyLock;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use forge_domain::{Actor, LinkProvider, ModelKind, Operation, PullRequestBlocker};
use hmac::{Hmac, KeyInit, Mac};
use regex::Regex;
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::app::PullRequestNow;
use super::{github_agent, object, write};
use crate::error::AppError;
use crate::state::AppState;

/// `POST /api/github/webhook`
pub async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let app = state
        .github
        .as_ref()
        .ok_or_else(|| AppError::not_found("GitHub isn't connected on this server"))?;
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify(&app.webhook_secret, &body, signature) {
        return Err(AppError::unauthorized("bad webhook signature"));
    }
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let payload: Value =
        serde_json::from_slice(&body).map_err(|e| AppError::invalid(e.to_string()))?;
    handle(&state, &event, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Checks GitHub's `sha256=<hex>` HMAC of the body.
pub fn verify(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex_signature) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

pub async fn handle(state: &AppState, event: &str, payload: &Value) -> Result<(), AppError> {
    let Some(installation_id) = payload["installation"]["id"].as_i64() else {
        return Ok(());
    };
    let workspace: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM github_installations WHERE installation_id = $1",
    )
    .bind(installation_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(workspace_id) = workspace else {
        // Not linked to a workspace (yet): nothing to do.
        return Ok(());
    };

    match (event, payload["action"].as_str().unwrap_or("")) {
        ("installation", "deleted") => {
            sqlx::query("DELETE FROM github_installations WHERE installation_id = $1")
                .bind(installation_id)
                .execute(&state.pool)
                .await?;
        }
        ("installation", _) | ("installation_repositories", _) => {
            let added = payload["repositories_added"]
                .as_array()
                .or(payload["repositories"].as_array());
            // These payloads don't say which branch is the default, so new
            // repositories ask GitHub before they're cloned.
            for repo in added.into_iter().flatten() {
                if let Some(name) = repo["full_name"].as_str()
                    && super::remember_repository(&state.pool, installation_id, name, None).await?
                {
                    super::refresh_clone(state.clone(), installation_id, name.to_string(), None);
                }
            }
            for repo in payload["repositories_removed"]
                .as_array()
                .into_iter()
                .flatten()
            {
                sqlx::query(
                    "DELETE FROM github_repositories WHERE installation_id = $1 AND full_name = $2",
                )
                .bind(installation_id)
                .bind(repo["full_name"].as_str().unwrap_or(""))
                .execute(&state.pool)
                .await?;
            }
        }
        ("pull_request", action) => {
            pull_request(&state.pool, workspace_id, action, payload).await?;
            // Whether it could be merged now decides whether its issues are
            // ready for review.
            let pr = &payload["pull_request"];
            let asked_for_review = matches!(action, "opened" | "reopened" | "ready_for_review");
            if (asked_for_review || action == "synchronize")
                && let Some(number) = pr["number"].as_i64()
            {
                let from_payload = PullRequestNow {
                    open: pr["state"] == "open",
                    draft: pr["draft"] == true,
                    blocker: (pr["mergeable"] == false).then_some(PullRequestBlocker::Conflicts),
                    awaiting_checks: false,
                };
                recheck(
                    state.clone(),
                    workspace_id,
                    installation_id,
                    repository(payload),
                    vec![(number, asked_for_review.then_some(from_payload))],
                );
            }
        }
        ("status", _) => {
            // A commit status changed. The event doesn't say which pull
            // requests it's for, so ask about every open one in the repository.
            let repository = repository(payload);
            let open = open_pull_requests(&state.pool, workspace_id, &repository).await?;
            recheck(
                state.clone(),
                workspace_id,
                installation_id,
                repository,
                open.into_iter().map(|n| (n, None)).collect(),
            );
        }
        ("check_suite" | "check_run", _) => {
            // Checks for a pull request's latest commit started or finished.
            let numbers = payload[event]["pull_requests"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|pr| pr["number"].as_i64())
                .collect::<BTreeSet<_>>();
            recheck(
                state.clone(),
                workspace_id,
                installation_id,
                repository(payload),
                numbers.into_iter().map(|n| (n, None)).collect(),
            );
        }
        ("push", _) => push(state, workspace_id, installation_id, payload).await?,
        _ => {}
    }
    Ok(())
}

pub(crate) static IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b([a-z][a-z0-9]{0,4})-(\d{1,7})\b").expect("a valid pattern")
});

/// An identifier right after one of GitHub's closing words: "Fixes FOR-12",
/// "closed: FOR-3".
pub(crate) static CLOSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)(?:\s*:\s*|\s+)([a-z][a-z0-9]{0,4})-(\d{1,7})\b",
    )
    .expect("a valid pattern")
});

/// Identifiers in some text, as (team key, number): every one, or only those
/// after a closing word.
pub(crate) fn identifiers(text: &str, pattern: &Regex) -> BTreeSet<(String, i32)> {
    pattern
        .captures_iter(text)
        .map(|c| (c[1].to_uppercase(), c[2].parse().unwrap_or(0)))
        .collect()
}

/// An issue a pull request or commit names.
pub(crate) struct Mention {
    pub(crate) issue: Uuid,
    pub(crate) team: Uuid,
    pub(crate) state: Uuid,
    pub(crate) state_type: String,
    /// Works on it (GitHub's rule: named in the branch, or after a closing
    /// word), so it moves with the pull request. Otherwise it's only linked.
    pub(crate) works_on: bool,
}

/// The workspace's issues among some identifiers.
pub(crate) async fn mentioned(
    db: &PgPool,
    workspace_id: Uuid,
    named: BTreeSet<(String, i32)>,
    works_on: &BTreeSet<(String, i32)>,
) -> Result<Vec<Mention>, AppError> {
    let mut found = Vec::new();
    for (key, number) in named.union(works_on) {
        let issue: Option<(Uuid, Uuid, Uuid, String)> = sqlx::query_as(
            "SELECT i.id, i.team_id, i.state_id, s.type FROM issues i JOIN teams t ON t.id = i.team_id \
             JOIN workflow_states s ON s.id = i.state_id \
             WHERE t.workspace_id = $1 AND t.key = $2 AND i.number = $3",
        )
        .bind(workspace_id)
        .bind(key)
        .bind(number)
        .fetch_optional(db)
        .await?;
        found.extend(issue.map(|(issue, team, state, state_type)| Mention {
            issue,
            team,
            state,
            state_type,
            works_on: works_on.contains(&(key.clone(), *number)),
        }));
    }
    Ok(found)
}

async fn pull_request(
    pool: &PgPool,
    workspace_id: Uuid,
    action: &str,
    payload: &Value,
) -> Result<(), AppError> {
    let pr = &payload["pull_request"];
    let title = pr["title"].as_str().unwrap_or("");
    let body = pr["body"].as_str().unwrap_or("");
    let branch = pr["head"]["ref"].as_str().unwrap_or("");
    let mut named = identifiers(title, &IDENTIFIER);
    named.extend(identifiers(body, &IDENTIFIER));
    let mut works_on = identifiers(branch, &IDENTIFIER);
    works_on.extend(identifiers(title, &CLOSING));
    works_on.extend(identifiers(body, &CLOSING));
    let issues = mentioned(pool, workspace_id, named, &works_on).await?;
    if issues.is_empty() {
        return Ok(());
    }
    let merged = pr["merged"].as_bool().unwrap_or(false);
    let draft = pr["draft"].as_bool().unwrap_or(false);
    let state = match pr["state"].as_str() {
        Some("closed") if merged => "merged",
        Some("closed") => "closed",
        _ => "open",
    };

    let mut db = pool.begin().await?;
    let agent = github_agent(&mut db, workspace_id).await?;
    for mention in issues {
        upsert_link(
            &mut db,
            agent,
            mention.issue,
            json!({
                "kind": "pull_request",
                "url": pr["html_url"],
                "title": title,
                "repository": payload["repository"]["full_name"],
                "number": pr["number"],
                "state": state,
                "author": pr["user"]["login"],
                "works_on": mention.works_on,
            }),
        )
        .await?;
        if !mention.works_on {
            continue;
        }

        // A draft means the work is still going. A pull request ready for
        // review moves its issues to review once it could be merged (see
        // `settle`). Merging finishes it. Closed without merging, or turned
        // back into a draft, it leaves nothing to review: an issue waiting
        // there goes back to work (which wakes its agent, `sent_back`).
        let state_type = mention.state_type.as_str();
        let open = !["completed", "canceled"].contains(&state_type);
        let target = match action {
            "opened" | "reopened" if draft && open && state_type != "started" => {
                Some(Target::Started)
            }
            "closed" if merged && open => Some(Target::Completed),
            "closed" | "converted_to_draft"
                if status_for(&mut db, mention.team, &Target::Review).await?
                    == Some(mention.state) =>
            {
                Some(Target::Started)
            }
            _ => None,
        };
        if let Some(target) = target {
            move_issue(&mut db, agent, mention.issue, mention.team, target).await?;
        }
    }
    db.commit().await?;
    Ok(())
}

async fn push(
    state: &AppState,
    workspace_id: Uuid,
    installation_id: i64,
    payload: &Value,
) -> Result<(), AppError> {
    let repository = payload["repository"]["full_name"]
        .as_str()
        .unwrap_or("")
        .to_string();
    // GitHub's own word on the default branch, which also corrects the one
    // stored for the repository; what's stored if the payload doesn't say.
    let branch = match payload["repository"]["default_branch"].as_str() {
        Some(branch) => branch.to_string(),
        None => sqlx::query_scalar(
            "SELECT default_branch FROM github_repositories WHERE installation_id = $1 AND full_name = $2",
        )
        .bind(installation_id)
        .bind(&repository)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or_else(|| "main".to_string()),
    };
    // Only pushes to the default branch change what the code does.
    let to_default = payload["ref"].as_str() == Some(&format!("refs/heads/{branch}"));
    let mut db = state.pool.begin().await?;
    let agent = github_agent(&mut db, workspace_id).await?;
    let mut changed = BTreeSet::new();
    for commit in payload["commits"].as_array().into_iter().flatten() {
        for field in ["added", "modified", "removed"] {
            for path in commit[field].as_array().into_iter().flatten() {
                changed.extend(path.as_str().map(str::to_string));
            }
        }
        let message = commit["message"].as_str().unwrap_or("");
        let named = identifiers(message, &IDENTIFIER);
        let closes = identifiers(message, &CLOSING);
        for mention in mentioned(&state.pool, workspace_id, named, &closes).await? {
            // Only a link: an issue is done once its pull request is merged,
            // so pushing "Fixes FOR-12" can't skip the review.
            upsert_link(
                &mut db,
                agent,
                mention.issue,
                json!({
                    "kind": "commit",
                    "url": commit["url"],
                    "title": message.lines().next().unwrap_or(""),
                    "repository": repository,
                    "sha": commit["id"],
                    "state": "pushed",
                    "author": commit["author"]["username"],
                    "works_on": mention.works_on,
                }),
            )
            .await?;
        }
    }
    if to_default {
        mark_code_changed(&mut db, agent, &repository, &changed).await?;
    }
    db.commit().await?;

    if !to_default {
        return Ok(());
    }
    // The default branch moved: open pull requests may now conflict with it.
    let open = open_pull_requests(&state.pool, workspace_id, &repository).await?;
    recheck(
        state.clone(),
        workspace_id,
        installation_id,
        repository.clone(),
        open.into_iter().map(|n| (n, None)).collect(),
    );
    super::refresh_then_check(
        state.clone(),
        installation_id,
        repository,
        branch,
        workspace_id,
        changed,
    );
    Ok(())
}

/// The services whose code a push to the default branch changed: their
/// requirement verdicts, checked before it, are stale now.
async fn mark_code_changed(
    db: &mut PgConnection,
    agent: Actor,
    repository: &str,
    changed: &BTreeSet<String>,
) -> Result<(), AppError> {
    let services: Vec<(Uuid, Vec<String>)> = sqlx::query_as(
        "SELECT id, paths FROM services WHERE workspace_id = $1 AND repository = $2",
    )
    .bind(agent.workspace_id)
    .bind(format!("github:{repository}"))
    .fetch_all(&mut *db)
    .await?;
    let now = chrono::Utc::now();
    let operations: Vec<Operation> = services
        .into_iter()
        .filter(|(_, paths)| {
            changed
                .iter()
                .any(|path| forge_ai::matches_any(paths, path))
        })
        .map(|(id, _)| Operation::Update {
            model: ModelKind::Service,
            id,
            data: object(json!({ "code_changed_at": now })),
        })
        .collect();
    if operations.is_empty() {
        return Ok(());
    }
    write(db, agent, operations).await
}

/// The open pull requests of a repository that work on an issue.
async fn open_pull_requests(
    pool: &PgPool,
    workspace_id: Uuid,
    repository: &str,
) -> Result<Vec<i64>, AppError> {
    let open: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT number FROM issue_links WHERE workspace_id = $1 AND kind = 'pull_request' \
         AND repository = $2 AND state = 'open' AND works_on AND number IS NOT NULL \
         AND provider = 'github'",
    )
    .bind(workspace_id)
    .bind(repository)
    .fetch_all(pool)
    .await?;
    Ok(open.into_iter().map(i64::from).collect())
}

fn repository(payload: &Value) -> String {
    payload["repository"]["full_name"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// How many more times to ask about a pull request whose checks haven't
/// started yet, [`GitHubApp::checks_after`] apart, before its missing checks
/// count as passed: about a minute.
const CHECK_ASKS: usize = 6;

/// Asks GitHub, in the background, whether some pull requests could be
/// merged now, and [`settle`]s each. With what the webhook said about one
/// (`Some`), it was just opened or marked ready for review, and that stands
/// in when GitHub can't be asked. Otherwise nothing changes until it can.
///
/// A pull request whose checks haven't started yet is held back as checks
/// running, and asked about again until they have: their own webhooks take
/// it from there. If none ever start, it counts as having passed.
///
/// [`GitHubApp::checks_after`]: super::app::GitHubApp::checks_after
fn recheck(
    state: AppState,
    workspace_id: Uuid,
    installation_id: i64,
    repository: String,
    pull_requests: Vec<(i64, Option<PullRequestNow>)>,
) {
    let Some(app) = state.github.clone() else {
        return;
    };
    if pull_requests.is_empty() {
        return;
    }
    app.clone().spawn(async move {
        // Right after the default branch moves, GitHub may still answer
        // with what it knew before.
        if pull_requests
            .iter()
            .all(|(_, from_payload)| from_payload.is_none())
        {
            tokio::time::sleep(app.retry_after()).await;
        }
        for (number, from_payload) in pull_requests {
            let asked_for_review = from_payload.is_some();
            let now = match app
                .pull_request(installation_id, &repository, number, true)
                .await
            {
                Ok(now) => now,
                Err(e) => {
                    tracing::warn!(repository, number, error = %e, "couldn't ask GitHub about a pull request");
                    match from_payload {
                        Some(now) => now,
                        None => continue,
                    }
                }
            };
            if let Err(e) = settle(
                &state.pool,
                workspace_id,
                LinkProvider::Github,
                &repository,
                number,
                now,
                asked_for_review,
            )
            .await
            {
                tracing::warn!(repository, number, error = ?e, "couldn't settle a pull request");
            }
            if now.awaiting_checks {
                await_checks(
                    state.clone(),
                    workspace_id,
                    installation_id,
                    repository.clone(),
                    number,
                );
            }
        }
    });
}

/// Asks GitHub about a pull request again, in the background, until its
/// checks have started, and [`settle`]s it once they have; after
/// [`CHECK_ASKS`] asks without any, as having none.
fn await_checks(
    state: AppState,
    workspace_id: Uuid,
    installation_id: i64,
    repository: String,
    number: i64,
) {
    let Some(app) = state.github.clone() else {
        return;
    };
    app.clone().spawn(async move {
        for ask in 1..=CHECK_ASKS {
            tokio::time::sleep(app.checks_after()).await;
            let expect_checks = ask < CHECK_ASKS;
            let now = match app
                .pull_request(installation_id, &repository, number, expect_checks)
                .await
            {
                Ok(now) => now,
                Err(e) => {
                    tracing::warn!(repository, number, error = %e, "couldn't ask GitHub about a pull request");
                    continue;
                }
            };
            if now.awaiting_checks {
                continue;
            }
            if let Err(e) = settle(
                &state.pool,
                workspace_id,
                LinkProvider::Github,
                &repository,
                number,
                now,
                false,
            )
            .await
            {
                tracing::warn!(repository, number, error = ?e, "couldn't settle a pull request");
            }
            return;
        }
    });
}

/// A pull request's link to an issue, with where the issue is.
#[derive(sqlx::FromRow)]
struct PullRequestLink {
    link: Uuid,
    issue: Uuid,
    team: Uuid,
    state: Uuid,
    state_type: String,
    blocker: Option<String>,
    held: bool,
    works_on: bool,
}

/// Brings an open pull request's links up to date with what keeps it from
/// being merged, and moves the issues it works on:
/// - into review once nothing does, when it was just asked for review or
///   Forge was holding the issue back;
/// - back from review to the first started status when it runs into
///   conflicts or failing checks, holding it there (the link wakes the
///   agent, `pr_blocked`);
/// - asked for review while something does: to the first started status,
///   if it wasn't started yet, held there.
///
/// Checks still running hold an issue back from review, but don't send one
/// already there back.
pub async fn settle(
    pool: &PgPool,
    workspace_id: Uuid,
    provider: LinkProvider,
    repository: &str,
    number: i64,
    now: PullRequestNow,
    asked_for_review: bool,
) -> Result<(), AppError> {
    if !now.open {
        return Ok(());
    }
    let links: Vec<PullRequestLink> = sqlx::query_as(
        "SELECT l.id AS link, l.issue_id AS issue, i.team_id AS team, i.state_id AS state, \
         s.type AS state_type, l.blocker, l.held, l.works_on \
         FROM issue_links l JOIN issues i ON i.id = l.issue_id \
         JOIN workflow_states s ON s.id = i.state_id \
         WHERE l.workspace_id = $1 AND l.kind = 'pull_request' AND l.repository = $2 \
         AND l.number = $3 AND l.state = 'open' AND l.provider = $4",
    )
    .bind(workspace_id)
    .bind(repository)
    .bind(i32::try_from(number).unwrap_or(0))
    .bind(provider_name(provider))
    .fetch_all(pool)
    .await?;
    if links.is_empty() {
        return Ok(());
    }
    let blocker = now.blocker.map(|b| match b {
        PullRequestBlocker::Conflicts => "conflicts",
        PullRequestBlocker::ChecksFailed => "checks_failed",
        PullRequestBlocker::ChecksRunning => "checks_running",
    });

    let mut db = pool.begin().await?;
    let agent = match provider {
        LinkProvider::Github => github_agent(&mut db, workspace_id).await?,
        LinkProvider::Forgejo => crate::forgejo::forgejo_agent(&mut db, workspace_id).await?,
    };
    for PullRequestLink {
        link,
        issue,
        team,
        state,
        state_type,
        blocker: was_blocker,
        held: was_held,
        works_on,
    } in links
    {
        let open = !["completed", "canceled"].contains(&state_type.as_str());
        let mut held = was_held;
        let mut target = None;
        if works_on && open && !now.draft {
            let in_review = status_for(&mut db, team, &Target::Review).await? == Some(state);
            match now.blocker {
                Some(PullRequestBlocker::Conflicts | PullRequestBlocker::ChecksFailed)
                    if in_review =>
                {
                    held = true;
                    target = Some(Target::Started);
                }
                Some(_) if asked_for_review => {
                    held = true;
                    target = (state_type != "started").then_some(Target::Started);
                }
                None if asked_for_review || was_held => {
                    held = false;
                    // Not if someone moved it out of the started statuses meanwhile.
                    target =
                        (asked_for_review || state_type == "started").then_some(Target::Review);
                }
                _ => {}
            }
        }
        // The link first: the issue moving back then knows why.
        if was_blocker.as_deref() != blocker || was_held != held {
            write(
                &mut db,
                agent,
                vec![Operation::Update {
                    model: ModelKind::IssueLink,
                    id: link,
                    data: object(json!({ "blocker": blocker, "held": held })),
                }],
            )
            .await?;
        }
        if let Some(target) = target {
            move_issue(&mut db, agent, issue, team, target).await?;
        }
    }
    db.commit().await?;
    Ok(())
}

/// A provider as its links store it.
pub(crate) fn provider_name(provider: LinkProvider) -> &'static str {
    match provider {
        LinkProvider::Github => "github",
        LinkProvider::Forgejo => "forgejo",
    }
}

/// Links a pull request or commit to an issue, or updates the link.
pub(crate) async fn upsert_link(
    db: &mut PgConnection,
    agent: Actor,
    issue: Uuid,
    link: Value,
) -> Result<(), AppError> {
    let url = link["url"].as_str().unwrap_or("").to_string();
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM issue_links WHERE issue_id = $1 AND url = $2")
            .bind(issue)
            .bind(&url)
            .fetch_optional(&mut *db)
            .await?;
    let operation = match existing {
        Some(id) => Operation::Update {
            model: ModelKind::IssueLink,
            id,
            // A pull request edited to say "Fixes FOR-12" starts working on
            // the issue, and stops when the closing word goes.
            // Forgejo's links also say what their checks are and whether
            // they're drafts.
            data: {
                let mut data = object(
                    json!({ "title": link["title"], "state": link["state"], "works_on": link["works_on"] }),
                );
                for field in ["checks", "draft"] {
                    if let Some(value) = link.get(field) {
                        data.insert(field.into(), value.clone());
                    }
                }
                data
            },
        },
        None => {
            let mut data = object(link);
            data.insert("issue_id".into(), json!(issue));
            data.retain(|_, v| !v.is_null());
            Operation::Create {
                model: ModelKind::IssueLink,
                id: Uuid::now_v7(),
                data,
            }
        }
    };
    write(db, agent, vec![operation]).await
}

/// Where a pull request moves the issues it works on.
pub(crate) enum Target {
    /// The team's first started status (In Progress).
    Started,
    /// The review status: the last started one (In Review), as the issue
    /// page's review bar has it. The only started one, for a team with one.
    Review,
    /// The first completed status (Done).
    Completed,
}

/// The team's status a target stands for.
pub(crate) async fn status_for(
    db: &mut PgConnection,
    team: Uuid,
    target: &Target,
) -> Result<Option<Uuid>, AppError> {
    let (kind, order) = match target {
        Target::Started => ("started", "ASC"),
        Target::Review => ("started", "DESC"),
        Target::Completed => ("completed", "ASC"),
    };
    // Safe: the order is one of two constants above.
    let sql = format!(
        "SELECT id FROM workflow_states WHERE team_id = $1 AND type = $2 ORDER BY position {order} LIMIT 1"
    );
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(team)
        .bind(kind)
        .fetch_optional(&mut *db)
        .await?)
}

pub(crate) async fn move_issue(
    db: &mut PgConnection,
    agent: Actor,
    issue: Uuid,
    team: Uuid,
    target: Target,
) -> Result<(), AppError> {
    let state = status_for(db, team, &target).await?;
    let current: Option<Uuid> = sqlx::query_scalar("SELECT state_id FROM issues WHERE id = $1")
        .bind(issue)
        .fetch_optional(&mut *db)
        .await?;
    // Already there: nothing to record in its history.
    if let Some(state) = state.filter(|s| Some(*s) != current) {
        // Through the client rules, like a person moving it.
        let transaction = forge_domain::Transaction {
            id: Uuid::now_v7(),
            operations: vec![Operation::Update {
                model: ModelKind::Issue,
                id: issue,
                data: object(json!({ "state_id": state })),
            }],
        };
        forge_sync::apply_in(db, agent, &transaction, forge_sync::Origin::Client).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_are_checked() {
        let body = br#"{"zen":"Keep it logically awesome."}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let good = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify("secret", body, &good));
        assert!(!verify("other", body, &good));
        assert!(!verify("secret", b"tampered", &good));
        assert!(!verify("secret", body, "sha1=abc"));
        assert!(!verify("secret", body, ""));
    }

    #[test]
    fn identifiers_are_found_anywhere() {
        let found: Vec<String> = IDENTIFIER
            .captures_iter("for-12-rate-limiting fixes FOR-7, closes ENG2-104; not ABCDEF-1 or 1-2")
            .map(|c| format!("{}-{}", c[1].to_uppercase(), &c[2]))
            .collect();
        assert_eq!(found, ["FOR-12", "FOR-7", "ENG2-104"]);
    }

    #[test]
    fn only_closing_words_work_on_an_issue() {
        let text = "Fixes FOR-1. closes: FOR-2, Resolved for-3 and FOR-4; \
                    follow-up filed as FOR-5, see fixing FOR-6 or prefix FOR-7";
        let found: Vec<String> = identifiers(text, &CLOSING)
            .into_iter()
            .map(|(key, number)| format!("{key}-{number}"))
            .collect();
        assert_eq!(found, ["FOR-1", "FOR-2", "FOR-3"]);
    }
}
