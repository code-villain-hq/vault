//! GitHub Issues as the workspace's tracker: searched in GitHub's own
//! language, only ever in the workspace's repositories (TKT-17, TKT-19).

use std::cmp::Reverse;

use crate::config::AppConfig;
use crate::error::{Error, Result};
use crate::integrations::github::{GitHub, IssueRef};

use super::{Find, NewTicket, Ticket, TicketPage, TicketType};

/// Where the workspace's issues are.
pub struct Issues {
    /// `owner/name` of each of the workspace's repositories on this GitHub.
    pub repos: Vec<String>,
}

/// How many repositories one search names. GitHub caps a query's length,
/// so a workspace with more is searched in parts.
const REPOS_PER_SEARCH: usize = 25;

impl Issues {
    pub fn of(cfg: &AppConfig, web_url: &str) -> Self {
        let host = host_of(web_url);
        let mut repos: Vec<String> = cfg
            .projects
            .iter()
            .filter_map(|p| p.remote.as_deref())
            .filter_map(|remote| {
                let parts: Vec<&str> = remote.split('/').collect();
                match parts.as_slice() {
                    [h, owner, name] if host_of(h).eq_ignore_ascii_case(&host) => Some(format!("{owner}/{name}")),
                    _ => None,
                }
            })
            .collect();
        repos.sort_by_key(|r| r.to_lowercase());
        repos.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Self { repos }
    }
}

/// A host without its scheme, credentials or port.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split('/').next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    host.split(':').next().unwrap_or_default().to_string()
}

/// `owner/name#12`, as GitHub writes a reference to an issue. Owner and name
/// are held to GitHub's own characters: they go into a URL and a query.
pub fn parse_key(key: &str) -> Option<IssueRef> {
    let (repo, number) = key.trim().split_once('#')?;
    let (owner, name) = repo.split_once('/')?;
    let fine = |s: &str| !s.is_empty() && !s.starts_with(['-', '.']) && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    let number: u64 = number.parse().ok().filter(|n| *n > 0)?;
    (fine(owner) && fine(name)).then(|| (owner.to_string(), name.to_string(), number))
}

pub fn url_of(web_url: &str, key: &str) -> Option<String> {
    let (owner, name, number) = parse_key(key)?;
    Some(format!("{}/{owner}/{name}/issues/{number}", web_url.trim_end_matches('/')))
}

/// What an issue is, by TKT-17: without the organisation's issue types, an
/// issue with sub-issues is an epic, and any other is just an issue.
pub(super) fn types() -> Vec<TicketType> {
    let t = |id: &str, level| TicketType { id: id.into(), name: id.into(), subtask: false, hierarchy_level: level, icon: None };
    vec![t("Issue", 0), t("Epic", 1)]
}

pub(super) async fn mine(client: &GitHub, issues: &Issues) -> Result<TicketPage> {
    search(client, issues, "is:issue is:open assignee:@me archived:false sort:updated-desc", 500).await
}

pub(super) async fn find(client: &GitHub, issues: &Issues, find: Find) -> Result<TicketPage> {
    const CAP: usize = 100;
    let text = find.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
    // A key, or a bare number that names that issue in each repository (TKT-19).
    if let Some(text) = text {
        let wanted: Vec<IssueRef> = match (parse_key(text), text.parse::<u64>()) {
            (Some(one), _) => vec![one],
            (None, Ok(n)) if n > 0 => issues
                .repos
                .iter()
                .filter_map(|r| r.split_once('/').map(|(o, n2)| (o.to_string(), n2.to_string(), n)))
                .collect(),
            _ => Vec::new(),
        };
        if !wanted.is_empty() {
            let found = client.issues(&wanted).await?;
            if !found.is_empty() {
                return Ok(TicketPage { issues: found, more: false });
            }
        }
    }
    let query = find_query(text, &find.whose, find.include_done, &find.order);
    let mut page = search(client, issues, &query, CAP).await?;
    if !find.types.is_empty() {
        page.issues.retain(|t| find.types.iter().any(|ty| ty.eq_ignore_ascii_case(&t.issue_type)));
    }
    Ok(page)
}

/// The search for Find work, before the repositories are named.
fn find_query(text: Option<&str>, whose: &str, include_done: bool, order: &str) -> String {
    let mut q = vec!["is:issue".to_string(), "archived:false".into()];
    if !include_done {
        q.push("is:open".into());
    }
    match whose {
        "unassigned" => q.push("no:assignee".into()),
        "anyone" => {}
        // Anyone but you, nobody included: `-assignee:@me` keeps unassigned.
        _ => q.push("-assignee:@me".into()),
    }
    if let Some(text) = text {
        // Typed text is searched for, never read as qualifiers (TKT-4).
        let words: String = text.chars().filter(|c| *c != '"').collect();
        if !words.trim().is_empty() {
            q.push(format!("\"{}\"", words.trim()));
        }
    }
    q.push(
        match order {
            "newest" => "sort:created-desc",
            "oldest" => "sort:created-asc",
            // Issues have no priority: priority falls back to recent work.
            _ => "sort:updated-desc",
        }
        .into(),
    );
    q.join(" ")
}

/// A search in every one of the workspace's repositories, in parts of
/// `REPOS_PER_SEARCH`, put back into the query's own order.
async fn search(client: &GitHub, issues: &Issues, query: &str, cap: usize) -> Result<TicketPage> {
    if issues.repos.is_empty() {
        return Ok(TicketPage { issues: Vec::new(), more: false });
    }
    let mut all = Vec::new();
    let mut more = false;
    for part in issues.repos.chunks(REPOS_PER_SEARCH) {
        let repos: Vec<String> = part.iter().map(|r| format!("repo:{r}")).collect();
        let page = client.search_issues(&format!("{query} {}", repos.join(" ")), cap).await?;
        more |= page.more;
        all.extend(page.issues);
    }
    if issues.repos.len() > REPOS_PER_SEARCH {
        sort_like(&mut all, query);
        if all.len() > cap {
            all.truncate(cap);
            more = true;
        }
    }
    Ok(TicketPage { issues: all, more })
}

fn sort_like(all: &mut [Ticket], query: &str) {
    if query.contains("sort:created-asc") {
        all.sort_by(|a, b| a.created.cmp(&b.created));
    } else if query.contains("sort:created-desc") {
        all.sort_by_key(|t| Reverse(t.created.clone()));
    } else {
        all.sort_by_key(|t| Reverse(t.updated.clone()));
    }
}

pub(super) async fn by_key(client: &GitHub, keys: &[String]) -> Result<Vec<Ticket>> {
    // As Jira's: the sidebar and both review lists, and no more.
    const CAP: usize = 100;
    let wanted: Vec<IssueRef> = keys.iter().filter_map(|k| parse_key(k)).take(CAP).collect();
    client.issues(&wanted).await
}

pub(super) async fn ticket(client: &GitHub, key: &str) -> Result<Ticket> {
    let one = parse_key(key).ok_or_else(|| Error::NotFound(format!("{key}: not a GitHub issue (owner/repo#12)")))?;
    client.issues(&[one]).await?.into_iter().next().ok_or_else(|| Error::NotFound(key.to_string()))
}

pub(super) fn issue(key: &str) -> Result<IssueRef> {
    parse_key(key).ok_or_else(|| Error::Other(format!("{key} is not a GitHub issue (owner/repo#12)")))
}

/// File into the repository asked for, else the parent's, else the
/// workspace's only one. Its key is the new issue's.
pub(super) async fn file(client: &GitHub, issues: &Issues, new: &NewTicket) -> Result<String> {
    let parent = new.parent_key.as_deref().map(issue).transpose()?;
    let asked = new.project_key.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let repo = match (asked, &parent, issues.repos.as_slice()) {
        (Some(repo), _, _) => repo.to_string(),
        (None, Some((owner, name, _)), _) => format!("{owner}/{name}"),
        (None, None, [only]) => only.clone(),
        _ => return Err(Error::Other("choose the repository to file the issue in".into())),
    };
    let (owner, name, _) = parse_key(&format!("{repo}#1")).ok_or_else(|| Error::Other(format!("{repo} is not a GitHub repository (owner/name)")))?;
    let number = client.create_issue(&owner, &name, new.summary.trim(), &new.description).await?;
    let child = (owner.clone(), name.clone(), number);
    if let Some(parent) = parent {
        client.put_under(&child, &parent).await?;
    }
    Ok(format!("{owner}/{name}#{number}"))
}

/// An issue has no state to move to, so starting work on one gives it to
/// you, unless someone has it (TKT-18).
pub(super) async fn take(client: &GitHub, ticket: &Ticket) -> Result<()> {
    if ticket.assignee.is_some() {
        return Ok(());
    }
    let login = client.login().await?;
    client.assign(&issue(&ticket.key)?, &login).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_names_an_owner_a_repository_and_a_number() {
        assert_eq!(parse_key("acme/web#12"), Some(("acme".into(), "web".into(), 12)));
        assert_eq!(parse_key("acme/web.js#1"), Some(("acme".into(), "web.js".into(), 1)));
        assert_eq!(parse_key("ACME-12"), None);
        assert_eq!(parse_key("acme/web#0"), None);
        assert_eq!(parse_key("acme/web#12x"), None);
        // Nothing that could leave the query string or a URL path.
        assert_eq!(parse_key("acme/we\"b#1"), None);
        assert_eq!(parse_key("acme/../web#1"), None);
        assert_eq!(parse_key("-acme/web#1"), None);
    }

    #[test]
    fn an_issue_is_seen_on_the_github_it_lives_on() {
        assert_eq!(url_of("https://github.com/", "acme/web#12").as_deref(), Some("https://github.com/acme/web/issues/12"));
        assert_eq!(url_of("https://github.com", "ACME-12"), None);
    }

    #[test]
    fn only_the_workspace_repositories_on_this_github_are_searched() {
        let mut cfg = AppConfig::default();
        for (i, remote) in ["github.com/acme/web", "github.com/acme/api", "ghe.example.com/acme/tools", "github.com/acme/Web"].iter().enumerate() {
            cfg.projects.push(crate::config::Project {
                id: i.to_string(),
                name: remote.to_string(),
                path: String::new(),
                default_branch: "main".into(),
                group: None,
                store: None,
                update_by: None,
                check: None,
                spec_folder: None,
                specs_in_app: false,
                remote: Some(remote.to_string()),
                repository_id: None,
            });
        }
        assert_eq!(Issues::of(&cfg, "https://github.com").repos, ["acme/api", "acme/web"]);
        assert_eq!(Issues::of(&cfg, "https://ghe.example.com:8443").repos, ["acme/tools"]);
    }

    #[test]
    fn typed_text_is_a_phrase_and_never_a_qualifier() {
        let q = find_query(Some("login \"is:closed\""), "unassigned", false, "newest");
        assert_eq!(q, "is:issue archived:false is:open no:assignee \"login is:closed\" sort:created-desc");
        let q = find_query(None, "not_mine", true, "priority");
        assert_eq!(q, "is:issue archived:false -assignee:@me sort:updated-desc");
    }
}
