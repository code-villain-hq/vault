//! GitHub Issues as the workspace's tracker (TKT-17): issues read as the
//! app's tickets, filed, commented on, assigned and put under another.
//!
//! An issue is only open or closed. Open and assigned is as close to "in
//! progress" as GitHub gets, and an issue with sub-issues is an epic: the
//! organisation's issue types are not read yet.

use serde_json::{json, Value};

use super::GitHub;
use crate::error::{Error, Result};
use crate::tickets::{Ticket, TicketPage};

/// An issue as a ticket. Only fields GitHub has had since sub-issues: one
/// unknown field fails the whole query.
const FIELDS: &str = "number title body state url createdAt updatedAt
  repository { nameWithOwner }
  assignees(first: 1) { nodes { login } }
  labels(first: 20) { nodes { name } }
  parent { number title repository { nameWithOwner } }
  subIssuesSummary { total }";

/// One page of a search, the most GitHub gives.
const PAGE: usize = 100;

/// An issue: its repository's owner and name, and its number.
pub type IssueRef = (String, String, u64);

impl GitHub {
    /// Issues matching a search, in the order the query sorts them, up to
    /// `cap`, and whether GitHub had more.
    pub async fn search_issues(&self, query: &str, cap: usize) -> Result<TicketPage> {
        let q = format!(
            "query($q: String!, $first: Int!, $after: String) {{
              search(query: $q, type: ISSUE, first: $first, after: $after) {{
                issueCount
                pageInfo {{ hasNextPage endCursor }}
                nodes {{ ... on Issue {{ {FIELDS} }} }}
              }}
            }}"
        );
        let mut issues = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let first = PAGE.min(cap - issues.len());
            let v = self.graphql(&q, json!({ "q": query, "first": first, "after": after })).await?;
            let search = answer(&v, "/data/search")?;
            let nodes = search.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
            issues.extend(nodes.iter().filter_map(to_ticket));
            let next = search.pointer("/pageInfo/hasNextPage").and_then(Value::as_bool).unwrap_or(false);
            after = search.pointer("/pageInfo/endCursor").and_then(Value::as_str).map(str::to_string);
            if !next || after.is_none() || nodes.is_empty() {
                return Ok(TicketPage { issues, more: false });
            }
            if issues.len() >= cap {
                return Ok(TicketPage { issues, more: true });
            }
        }
    }

    /// These issues, in one query. One that does not exist, or whose
    /// repository this token cannot see, is left out rather than failing the
    /// rest: a number typed into a search names it in every repository.
    pub async fn issues(&self, wanted: &[IssueRef]) -> Result<Vec<Ticket>> {
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        // Owner and name are checked by `parse_key` to be GitHub's own
        // characters, so they can sit in the query as they are.
        let parts: Vec<String> = wanted
            .iter()
            .enumerate()
            .map(|(i, (owner, name, number))| {
                format!("i{i}: repository(owner: \"{owner}\", name: \"{name}\") {{ issue(number: {number}) {{ {FIELDS} }} }}")
            })
            .collect();
        let v = self.graphql(&format!("query {{ {} }}", parts.join("\n")), Value::Null).await?;
        let Some(data) = v.get("data").filter(|d| !d.is_null()) else {
            return Err(graphql_error(&v));
        };
        Ok((0..wanted.len())
            .filter_map(|i| data.pointer(&format!("/i{i}/issue")).and_then(to_ticket))
            .collect())
    }

    /// File an issue, and say its number.
    pub async fn create_issue(&self, owner: &str, name: &str, title: &str, body: &str) -> Result<u64> {
        let v = self
            .json(
                self.req(reqwest::Method::POST, &format!("/repos/{owner}/{name}/issues"))
                    .json(&json!({ "title": title, "body": body })),
            )
            .await?;
        v.get("number").and_then(Value::as_u64).ok_or_else(|| Error::Other("GitHub filed the issue but did not say its number".into()))
    }

    /// Put an issue under another as its sub-issue, out from under any other.
    pub async fn put_under(&self, child: &IssueRef, parent: &IssueRef) -> Result<()> {
        // Sub-issues are named by their id, which a number is not.
        let (owner, name, number) = child;
        let v = self.json(self.req(reqwest::Method::GET, &format!("/repos/{owner}/{name}/issues/{number}"))).await?;
        let id = v.get("id").and_then(Value::as_u64).ok_or_else(|| Error::NotFound(format!("{owner}/{name}#{number}")))?;
        let (owner, name, number) = parent;
        self.json(
            self.req(reqwest::Method::POST, &format!("/repos/{owner}/{name}/issues/{number}/sub_issues"))
                .json(&json!({ "sub_issue_id": id, "replace_parent": true })),
        )
        .await?;
        Ok(())
    }

    pub async fn comment_issue(&self, (owner, name, number): &IssueRef, body: &str) -> Result<()> {
        self.json(
            self.req(reqwest::Method::POST, &format!("/repos/{owner}/{name}/issues/{number}/comments"))
                .json(&json!({ "body": body })),
        )
        .await?;
        Ok(())
    }

    /// Give an issue to this account, beside whoever has it.
    pub async fn assign(&self, (owner, name, number): &IssueRef, login: &str) -> Result<()> {
        self.json(
            self.req(reqwest::Method::POST, &format!("/repos/{owner}/{name}/issues/{number}/assignees"))
                .json(&json!({ "assignees": [login] })),
        )
        .await?;
        Ok(())
    }
}

/// What GraphQL answered at `pointer`, or the failure it answered 200 with.
fn answer<'a>(v: &'a Value, pointer: &str) -> Result<&'a Value> {
    if v.pointer("/errors/0").is_some() {
        return Err(graphql_error(v));
    }
    v.pointer(pointer).filter(|s| !s.is_null()).ok_or_else(|| Error::Other("GitHub did not answer the search".into()))
}

fn graphql_error(v: &Value) -> Error {
    let msg = v.pointer("/errors/0/message").and_then(Value::as_str).unwrap_or("no answer");
    Error::Other(format!("GitHub: {msg}"))
}

pub(crate) fn to_ticket(v: &Value) -> Option<Ticket> {
    let number = v.get("number").and_then(Value::as_u64).filter(|n| *n > 0)?;
    let repo = v.pointer("/repository/nameWithOwner").and_then(Value::as_str).filter(|r| !r.is_empty())?;
    let text = |p: &str| v.pointer(p).and_then(Value::as_str).map(str::to_string);
    let assignee = text("/assignees/nodes/0/login");
    let closed = text("/state").as_deref() == Some("CLOSED");
    let parent = v.get("parent").filter(|p| !p.is_null());
    let parent_key = parent.and_then(|p| {
        let repo = p.pointer("/repository/nameWithOwner").and_then(Value::as_str)?;
        Some(format!("{repo}#{}", p.get("number").and_then(Value::as_u64)?))
    });
    let has_subs = v.pointer("/subIssuesSummary/total").and_then(Value::as_u64).unwrap_or(0) > 0;
    Some(Ticket {
        key: format!("{repo}#{number}"),
        summary: text("/title").unwrap_or_default(),
        description: text("/body").unwrap_or_default(),
        status: if closed { "Closed" } else { "Open" }.into(),
        status_id: if closed { "closed" } else { "open" }.into(),
        status_category: match (closed, &assignee) {
            (true, _) => "done",
            (false, Some(_)) => "indeterminate",
            (false, None) => "new",
        }
        .into(),
        issue_type: if has_subs { "Epic" } else { "Issue" }.into(),
        priority: None,
        assignee,
        labels: v
            .pointer("/labels/nodes")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string)).collect())
            .unwrap_or_default(),
        components: Vec::new(),
        epic_key: parent_key,
        epic_summary: parent.and_then(|p| p.get("title").and_then(Value::as_str).map(str::to_string)),
        url: text("/url").unwrap_or_default(),
        created: text("/createdAt"),
        updated: text("/updatedAt"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_issue_reads_as_a_ticket_in_github_terms() {
        let v = json!({
            "number": 12, "title": "Fix the login", "body": "It loops.", "state": "OPEN",
            "url": "https://github.com/acme/web/issues/12",
            "createdAt": "2026-10-01T09:00:00Z", "updatedAt": "2026-10-02T09:00:00Z",
            "repository": { "nameWithOwner": "acme/web" },
            "assignees": { "nodes": [{ "login": "ada" }] },
            "labels": { "nodes": [{ "name": "bug" }] },
            "parent": { "number": 3, "title": "Sign-in", "repository": { "nameWithOwner": "acme/api" } },
            "subIssuesSummary": { "total": 0 }
        });
        let t = to_ticket(&v).unwrap();
        assert_eq!(t.key, "acme/web#12");
        assert_eq!(t.status_category, "indeterminate", "open and assigned is in progress");
        assert_eq!(t.issue_type, "Issue");
        assert_eq!(t.epic_key.as_deref(), Some("acme/api#3"));
        assert_eq!(t.labels, ["bug"]);
    }

    #[test]
    fn closed_is_done_and_unassigned_is_new_and_sub_issues_make_an_epic() {
        let base = |state: &str, assignees: Value, subs: u64| {
            json!({ "number": 1, "state": state, "repository": { "nameWithOwner": "acme/web" },
                    "assignees": { "nodes": assignees }, "parent": null, "subIssuesSummary": { "total": subs } })
        };
        assert_eq!(to_ticket(&base("CLOSED", json!([]), 0)).unwrap().status_category, "done");
        assert_eq!(to_ticket(&base("OPEN", json!([]), 0)).unwrap().status_category, "new");
        let epic = to_ticket(&base("OPEN", json!([]), 2)).unwrap();
        assert_eq!(epic.issue_type, "Epic");
        assert_eq!(epic.epic_key, None);
    }

    #[test]
    fn a_pull_request_in_the_results_is_not_a_ticket() {
        // `... on Issue` leaves a pull request's node empty.
        assert!(to_ticket(&json!({})).is_none());
    }
}
