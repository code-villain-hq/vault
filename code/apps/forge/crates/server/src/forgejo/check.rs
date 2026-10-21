//! One check of a workspace's Forgejo connection: ask Forgejo about the
//! watched repositories' pull requests, then link them to the issues they
//! name and move the issues they work on.
//!
//! Everything is asked first; if any of it fails, nothing changes and the
//! connection records why. Moves only follow what changed since the last
//! check (a pull request opened, marked ready, turned back into a draft,
//! merged or closed), so seeing the same merge twice changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use forge_domain::{LinkProvider, PullRequestBlocker, PullRequestChecks};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::client::{self, Client};
use super::forgejo_agent;
use crate::error::AppError;
use crate::github::app::PullRequestNow;
use crate::github::webhook::{
    CLOSING, IDENTIFIER, Target, identifiers, mentioned, move_issue, settle, status_for,
    upsert_link,
};

/// How far back the first check looks for closed pull requests to link.
const FIRST_LOOK_BACK: Duration = Duration::days(30);

/// Overlap between checks, so a pull request closed while one ran isn't
/// missed by the next.
const OVERLAP: Duration = Duration::minutes(10);

/// A pull request as Forgejo describes it.
struct PullRequest {
    repository: String,
    number: i64,
    url: String,
    title: String,
    body: String,
    /// Its branch; none for one pushed with `refs/for/main` (AGit).
    branch: Option<String>,
    author: Option<String>,
    /// open, merged or closed
    state: &'static str,
    draft: bool,
    mergeable: bool,
    closed_at: Option<DateTime<Utc>>,
    /// Asked for open ones only.
    checks: Option<PullRequestChecks>,
}

impl PullRequest {
    fn from(repository: &str, pr: &Value) -> Option<Self> {
        let merged = pr["merged"] == true;
        let state = match pr["state"].as_str() {
            _ if merged => "merged",
            Some("closed") => "closed",
            _ => "open",
        };
        let head = pr["head"]["ref"].as_str().unwrap_or("");
        // AGit pull requests point at refs/pull/N/head, not a branch.
        let agit = pr["flow"] == 1 || head.starts_with("refs/");
        Some(PullRequest {
            repository: repository.to_string(),
            number: pr["number"].as_i64()?,
            url: pr["html_url"].as_str()?.to_string(),
            title: pr["title"].as_str().unwrap_or("").to_string(),
            body: pr["body"].as_str().unwrap_or("").to_string(),
            branch: (!agit && !head.is_empty()).then(|| head.to_string()),
            author: pr["user"]["login"].as_str().map(str::to_string),
            state,
            draft: pr["draft"] == true,
            mergeable: pr["mergeable"] != false,
            closed_at: client::time(&pr["merged_at"]).or(client::time(&pr["closed_at"])),
            checks: None,
        })
    }

    fn blocker(&self) -> Option<PullRequestBlocker> {
        if !self.mergeable {
            return Some(PullRequestBlocker::Conflicts);
        }
        match self.checks {
            Some(PullRequestChecks::Failed) => Some(PullRequestBlocker::ChecksFailed),
            Some(PullRequestChecks::Running) => Some(PullRequestBlocker::ChecksRunning),
            _ => None,
        }
    }
}

/// The connection as stored.
#[derive(sqlx::FromRow)]
struct Connection {
    server: String,
    token: String,
    repositories: Vec<String>,
    connected_at: DateTime<Utc>,
    checked_at: Option<DateTime<Utc>>,
}

/// Checks a workspace's Forgejo connection once, if it has one.
pub async fn check(pool: &PgPool, workspace_id: Uuid) -> Result<(), AppError> {
    let Some(connection): Option<Connection> = sqlx::query_as(
        "SELECT server, token, repositories, connected_at, checked_at FROM forgejo_connections \
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let started = Utc::now();
    let pull_requests = match ask(pool, workspace_id, &connection).await {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%workspace_id, error, "couldn't check Forgejo");
            sqlx::query(
                "UPDATE forgejo_connections SET failed_at = now(), check_error = $3 \
                 WHERE workspace_id = $1 AND connected_at = $2",
            )
            .bind(workspace_id)
            .bind(connection.connected_at)
            .bind(error)
            .execute(pool)
            .await?;
            return Ok(());
        }
    };
    // Applied only if the connection asked with still stands: disconnected
    // or connected anew meanwhile, what it found is dropped. Held for share
    // until done, so disconnecting or changing it waits for this check
    // instead of being overtaken by it.
    let mut guard = pool.begin().await?;
    let stands: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM forgejo_connections WHERE workspace_id = $1 AND connected_at = $2 \
         FOR SHARE",
    )
    .bind(workspace_id)
    .bind(connection.connected_at)
    .fetch_optional(&mut *guard)
    .await?;
    if stands.is_none() {
        return Ok(());
    }
    apply(pool, workspace_id, connection.connected_at, pull_requests).await?;
    sqlx::query(
        "UPDATE forgejo_connections SET checked_at = $2, failed_at = NULL, check_error = NULL \
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .bind(started)
    .execute(&mut *guard)
    .await?;
    guard.commit().await?;
    Ok(())
}

/// Every open pull request of the watched repositories, with its checks;
/// those closed since the last check; and any Forge still has as open that
/// is neither.
async fn ask(
    pool: &PgPool,
    workspace_id: Uuid,
    connection: &Connection,
) -> Result<Vec<PullRequest>, String> {
    let client = Client::new(&connection.server, &connection.token);
    let since = connection
        .checked_at
        .unwrap_or(connection.connected_at - FIRST_LOOK_BACK)
        - OVERLAP;
    let mut found = Vec::new();
    for repository in &connection.repositories {
        for pr in client.open_pull_requests(repository).await? {
            let Some(mut pull_request) = PullRequest::from(repository, &pr) else {
                continue;
            };
            if let Some(sha) = pr["head"]["sha"].as_str() {
                pull_request.checks = Some(client.checks(repository, sha).await?);
            }
            found.push(pull_request);
        }
        for pr in client.closed_since(repository, since).await? {
            found.extend(PullRequest::from(repository, &pr));
        }
    }

    let seen: BTreeSet<(String, i64)> = found
        .iter()
        .map(|pr| (pr.repository.clone(), pr.number))
        .collect();
    let still_open: Vec<(String, i32)> = sqlx::query_as(
        "SELECT DISTINCT repository, number FROM issue_links WHERE workspace_id = $1 \
         AND provider = 'forgejo' AND kind = 'pull_request' AND state = 'open' \
         AND number IS NOT NULL AND repository = ANY($2)",
    )
    .bind(workspace_id)
    .bind(&connection.repositories)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    for (repository, number) in still_open {
        let number = i64::from(number);
        if seen.contains(&(repository.clone(), number)) {
            continue;
        }
        if let Some(pr) = client.pull_request(&repository, number).await? {
            let Some(mut pull_request) = PullRequest::from(&repository, &pr) else {
                continue;
            };
            if pull_request.state == "open"
                && let Some(sha) = pr["head"]["sha"].as_str()
            {
                pull_request.checks = Some(client.checks(&repository, sha).await?);
            }
            found.push(pull_request);
        }
    }
    Ok(found)
}

/// What a link said before this check.
#[derive(sqlx::FromRow)]
struct Before {
    state: String,
    draft: bool,
    works_on: bool,
}

async fn apply(
    pool: &PgPool,
    workspace_id: Uuid,
    connected_at: DateTime<Utc>,
    pull_requests: Vec<PullRequest>,
) -> Result<(), AppError> {
    // Open pull requests to settle, and whether each was just asked for
    // review (opened, reopened, marked ready, or newly working on an issue).
    let mut to_settle: BTreeMap<(String, i64), (PullRequestNow, bool)> = BTreeMap::new();
    // Issues whose pull requests closed, merged or turned into drafts.
    let mut to_complete = BTreeSet::new();

    let mut db = pool.begin().await?;
    let agent = forgejo_agent(&mut db, workspace_id).await?;
    for pr in &pull_requests {
        let mut named = identifiers(&pr.title, &IDENTIFIER);
        named.extend(identifiers(&pr.body, &IDENTIFIER));
        let mut works_on = pr
            .branch
            .as_deref()
            .map(|branch| identifiers(branch, &IDENTIFIER))
            .unwrap_or_default();
        works_on.extend(identifiers(&pr.title, &CLOSING));
        works_on.extend(identifiers(&pr.body, &CLOSING));
        let mentions = mentioned(pool, workspace_id, named, &works_on).await?;
        let open = pr.state == "open";
        // Closed or merged before Forgejo was connected: only linked.
        let old = pr.closed_at.is_some_and(|at| at < connected_at);

        for mention in mentions {
            let before: Option<Before> = sqlx::query_as(
                "SELECT state, draft, works_on FROM issue_links WHERE issue_id = $1 AND url = $2",
            )
            .bind(mention.issue)
            .bind(&pr.url)
            .fetch_optional(&mut *db)
            .await?;
            let mut link = json!({
                "kind": "pull_request",
                "url": pr.url,
                "title": pr.title,
                "repository": pr.repository,
                "number": pr.number,
                "state": pr.state,
                "author": pr.author,
                "works_on": mention.works_on,
                "provider": "forgejo",
                "draft": pr.draft,
            });
            if open {
                link["checks"] = json!(pr.checks);
            }
            upsert_link(&mut db, agent, mention.issue, link).await?;
            if !mention.works_on || old {
                continue;
            }

            // What changed for this issue since the last check.
            let was_open = before
                .as_ref()
                .is_some_and(|b| b.state == "open" && b.works_on);
            let was_ready = was_open && before.as_ref().is_some_and(|b| !b.draft);
            let state_type = mention.state_type.as_str();
            let issue_open = !["completed", "canceled"].contains(&state_type);
            if open && pr.draft && !was_open && issue_open && state_type != "started" {
                // Opened (or reopened) as a draft: the work has started.
                move_issue(&mut db, agent, mention.issue, mention.team, Target::Started).await?;
            }
            if open && !pr.draft {
                let entry = to_settle
                    .entry((pr.repository.clone(), pr.number))
                    .or_insert((
                        PullRequestNow {
                            open: true,
                            draft: false,
                            blocker: pr.blocker(),
                            awaiting_checks: false,
                        },
                        false,
                    ));
                entry.1 |= !was_ready;
            }
            if was_open && (!open || (pr.draft && was_ready)) || before.is_none() && !open {
                to_complete.insert(mention.issue);
            }
        }
    }
    db.commit().await?;

    for ((repository, number), (now, asked_for_review)) in to_settle {
        settle(
            pool,
            workspace_id,
            LinkProvider::Forgejo,
            &repository,
            number,
            now,
            asked_for_review,
        )
        .await?;
    }
    for issue in to_complete {
        let mut db = pool.begin().await?;
        complete(&mut db, workspace_id, issue).await?;
        db.commit().await?;
    }
    Ok(())
}

/// Moves an issue whose Forgejo pull requests closed, merged or turned
/// into drafts: to Done once at least one that works on it is merged and
/// none is still open; back from review to work once none is left to
/// review. Closed without merging counts for neither. Completed and
/// canceled issues stay.
async fn complete(db: &mut PgConnection, workspace_id: Uuid, issue: Uuid) -> Result<(), AppError> {
    let Some((team, state, state_type)): Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT i.team_id, i.state_id, s.type FROM issues i \
         JOIN workflow_states s ON s.id = i.state_id WHERE i.id = $1",
    )
    .bind(issue)
    .fetch_optional(&mut *db)
    .await?
    else {
        return Ok(());
    };
    if ["completed", "canceled"].contains(&state_type.as_str()) {
        return Ok(());
    }
    let links: Vec<(String, bool)> = sqlx::query_as(
        "SELECT state, draft FROM issue_links WHERE issue_id = $1 AND provider = 'forgejo' \
         AND kind = 'pull_request' AND works_on",
    )
    .bind(issue)
    .fetch_all(&mut *db)
    .await?;
    let any_open = links.iter().any(|(state, _)| state == "open");
    let any_merged = links.iter().any(|(state, _)| state == "merged");
    let any_ready = links.iter().any(|(state, draft)| state == "open" && !draft);
    let target = if !any_open && any_merged {
        Some(Target::Completed)
    } else if !any_ready && status_for(db, team, &Target::Review).await? == Some(state) {
        Some(Target::Started)
    } else {
        None
    };
    if let Some(target) = target {
        let agent = forgejo_agent(db, workspace_id).await?;
        move_issue(db, agent, issue, team, target).await?;
    }
    Ok(())
}
