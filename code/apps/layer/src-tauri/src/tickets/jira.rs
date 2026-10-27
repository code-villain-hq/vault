//! Jira as the workspace's tracker: the searches and filing the app asks
//! of any tracker, done in JQL and Jira's own projects.

use serde_json::Value;

use crate::config::JiraConfig;
use crate::error::{Error, Result};
use crate::integrations::jira::{self, Jira};

use super::{Find, NewTicket, Ticket, TicketPage};

pub(super) async fn mine(client: &Jira, cfg: &JiraConfig) -> Result<TicketPage> {
    let jql = cfg
        .jql
        .clone()
        .unwrap_or_else(|| jira::default_jql(cfg.project_key.as_deref()));
    // Your own queue should be all of it: this is the list the app groups into
    // epics and reasons about, and a silent cut makes that grouping wrong.
    client.search(&jql, 500).await
}

/// The JQL is built here rather than in the UI so what is typed stays a search
/// term. Results are capped: this is for finding a ticket, not for paging
/// through a backlog.
pub(super) async fn find(client: &Jira, cfg: &JiraConfig, find: Find) -> Result<TicketPage> {
    let project = cfg.project_key.as_deref();
    let whose = jira::Whose::parse(&find.whose);
    let order = jira::Order::parse(&find.order);
    let Find { text, include_done, types, .. } = find;
    // A search is refined rather than read end to end, so it stays capped —
    // but it now says when there was more.
    const CAP: u32 = 100;

    // A bare number is only a guess at a key (TKT-12). Jira refuses a query
    // naming a key it does not have, and the filters may hide the one it
    // does, so either way the number is searched for as text after all.
    if let Some(key) = text.as_deref().and_then(|t| jira::number_as_key(project, t)) {
        let jql = jira::browse_jql(project, Some(&key), whose, include_done, &types, order);
        if let Ok(page) = client.search(&jql, CAP).await {
            if !page.issues.is_empty() {
                return Ok(page);
            }
        }
    }
    let jql = jira::browse_jql(project, text.as_deref(), whose, include_done, &types, order);
    client.search(&jql, CAP).await
}

/// Jira refuses the whole query when one key does not exist, and a key read
/// from a branch name may be a guess; then each is asked for on its own, and
/// the ones Jira does not have are left out.
pub(super) async fn by_key(client: &Jira, keys: &[String]) -> Result<Vec<Ticket>> {
    // The sidebar and both review lists together; past this, the rest show
    // under "No epic" rather than sending a query per ticket.
    const CAP: usize = 100;
    let Some(jql) = jira::keys_jql(&keys[..keys.len().min(CAP)]) else {
        return Ok(Vec::new());
    };
    match client.search(&jql, CAP as u32).await {
        Ok(page) => Ok(page.issues),
        Err(Error::Other(msg)) if msg.starts_with("Jira 400") => {
            let mut found = Vec::new();
            for key in keys.iter().take(CAP).filter(|k| jira::looks_like_a_key(k)) {
                if let Ok(issue) = client.issue(key).await {
                    found.push(issue);
                }
            }
            Ok(found)
        }
        Err(e) => Err(e),
    }
}

pub(super) async fn file(client: &Jira, cfg: &JiraConfig, new: &NewTicket) -> Result<String> {
    // A sub-task has to be filed in its parent's project, and the parent's key
    // says which that is — so an epic found by searching needs nothing else.
    let project_key = new
        .project_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .or_else(|| new.parent_key.as_deref().and_then(project_of))
        .or_else(|| cfg.project_key.clone())
        .ok_or_else(|| {
            Error::Other("no Jira project to file into — set a project key in Settings".into())
        })?;
    client
        .create_issue(
            &project_key,
            new.summary.trim(),
            &new.description,
            &new.issue_type,
            new.parent_key.as_deref(),
            new.fields.as_ref().unwrap_or(&Value::Null),
        )
        .await
}

/// The project a key belongs to: everything before the first dash.
pub(crate) fn project_of(key: &str) -> Option<String> {
    let (project, number) = key.split_once('-')?;
    (!project.is_empty() && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        .then(|| project.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_says_which_project_it_belongs_to() {
        // An epic found by searching carries its project in its key, which is
        // what lets a ticket be filed under it with nothing else configured.
        assert_eq!(project_of("ACME-19335").as_deref(), Some("ACME"));
        assert_eq!(project_of("sre-1189").as_deref(), Some("SRE"));
        assert_eq!(project_of("ACME-19335-suffix"), None);
        assert_eq!(project_of("ACME-"), None);
        assert_eq!(project_of("-1"), None);
        assert_eq!(project_of("nodash"), None);
    }
}
