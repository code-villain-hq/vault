//! The tracker's tools for agents (TKT-20): Jira's for a workspace whose
//! tickets are in Jira, GitHub's issue tools for one on GitHub Issues, each
//! in its own tracker's terms. Only the workspace's tracker's are listed,
//! and a call to the other's is refused: one set for both would have to call
//! a Jira transition and a GitHub close the same thing, and fit neither.

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::commands;
use crate::config::TrackerKind;
use crate::error::{Error, Result};
use crate::mcp::{arg, bool_prop, confirm_prop, required, str_prop, tool};
use crate::tickets::github::{in_workspace, search_for_agent};
use crate::tickets::{NewTicket, Tracker};

use super::tickets::tracker;
use super::AppState;

/// The tools of the tracker in use, or none without one.
pub fn tracker_tools(uses: Option<TrackerKind>) -> Vec<Value> {
    match uses {
        Some(TrackerKind::Jira) => jira_tools(),
        Some(TrackerKind::Github) => github_tools(),
        None => Vec::new(),
    }
}

fn jira_tools() -> Vec<Value> {
    vec![
        tool(
            "jira_search",
            "Search Jira with JQL, using the credentials stored in the app.",
            json!({
                "jql": str_prop("A JQL query, e.g. project = ACME AND statusCategory != Done"),
                "max": { "type": "integer", "description": "Maximum results, default 25" }
            }),
            vec!["jql"],
        ),
        tool(
            "jira_get_issue",
            "Read one Jira issue in full, including its description and epic.",
            json!({ "key": str_prop("Issue key, e.g. ACME-1234") }),
            vec!["key"],
        ),
        tool(
            "jira_issue_types",
            "The issue types this Jira defines, with their hierarchy level: 1 and \
             above is epic-level, 0 a standard issue, -1 a sub-task. Names differ \
             between sites, so ask rather than assuming one is called Task.",
            json!({}),
            vec![],
        ),
        tool(
            "jira_create_fields",
            "What a project demands before it will accept a new issue of a given \
             type: the required fields and the values each will take. Ask this \
             before jira_create_issue — many projects make components or a custom \
             field mandatory, and the create is refused without them.",
            json!({
                "project_key": str_prop("Project key, e.g. the ACME in ACME-1234"),
                "issue_type_id": str_prop("Issue type id; jira_issue_types lists them")
            }),
            vec!["project_key", "issue_type_id"],
        ),
        tool(
            "jira_create_issue",
            "File a new Jira issue. Returns the new key.",
            json!({
                "summary": str_prop("One-line title"),
                "description": str_prop("Body text; blank lines separate paragraphs"),
                "issue_type": str_prop("Type name as this site defines it, e.g. Bug or Task. Defaults to Task."),
                "project_key": str_prop("Defaults to the project configured in the app"),
                "parent_key": str_prop("Epic or parent key to file this under"),
                "fields": {
                    "type": "object",
                    "description": "The other fields jira_create_fields says this project \
                                    requires, by field id: {\"id\": \"…\"} for one allowed \
                                    value, a list of them for an array field, e.g. \
                                    {\"components\": [{\"id\": \"10001\"}]}"
                }
            }),
            vec!["summary"],
        ),
        tool(
            "jira_comment",
            "Add a comment to a Jira issue.",
            json!({ "key": str_prop("Issue key"), "text": str_prop("Comment body") }),
            vec!["key", "text"],
        ),
        tool(
            "jira_transition",
            "Move a Jira issue to another status by the transition's name, e.g. \
             'Start progress'. Call with no transition to list what is available. \
             Applying a transition needs confirm: true after the user agrees.",
            json!({
                "key": str_prop("Issue key"),
                "transition": str_prop("Transition name; omit to list the options"),
                "confirm": confirm_prop()
            }),
            vec!["key"],
        ),
    ]
}

fn github_tools() -> Vec<Value> {
    let key = || str_prop("The issue, as owner/repo#12");
    vec![
        tool(
            "github_search_issues",
            "Search the issues in the workspace's repositories with GitHub's issue search, \
             e.g. is:open label:bug assignee:@me. Only the workspace's repositories are \
             searched, and only their issues come back.",
            json!({
                "query": str_prop("GitHub search qualifiers and words; is:issue is added"),
                "max": { "type": "integer", "description": "Maximum results, default 25" }
            }),
            vec!["query"],
        ),
        tool(
            "github_get_issue",
            "Read one GitHub issue in full: its body, labels, assignee and parent issue.",
            json!({ "key": key() }),
            vec!["key"],
        ),
        tool(
            "github_create_issue",
            "File a GitHub issue in one of the workspace's repositories. With parent, it \
             becomes that issue's sub-issue. Returns the new key.",
            json!({
                "title": str_prop("One-line title"),
                "body": str_prop("Markdown body"),
                "repo": str_prop("owner/repo; by default the parent's, or the workspace's only one"),
                "parent": str_prop("The issue to file it under, as owner/repo#12")
            }),
            vec!["title"],
        ),
        tool(
            "github_comment",
            "Add a comment to a GitHub issue.",
            json!({ "key": key(), "text": str_prop("Comment body, in Markdown") }),
            vec!["key", "text"],
        ),
        tool(
            "github_close_issue",
            "Close a GitHub issue, as completed or as not planned. A pull request that says \
             Fixes closes its issue when it merges, so this is for issues closed otherwise. \
             Needs confirm: true after the user agrees.",
            json!({
                "key": key(),
                "not_planned": bool_prop("Close it as not planned rather than completed"),
                "confirm": confirm_prop()
            }),
            vec!["key"],
        ),
    ]
}

pub fn tracker_owns(name: &str) -> bool {
    name.starts_with("jira_") || name.starts_with("github_")
}

/// The confirm gate is `mcp.rs`'s: `jira_transition` and `github_close_issue`
/// are in its `DANGER` list.
pub async fn tracker_call(app: &AppHandle, name: &str, args: Value) -> Result<Value> {
    let state = app.state::<AppState>();
    let uses = state.config.read().tracker_in_use();
    let wants = if name.starts_with("github_") { TrackerKind::Github } else { TrackerKind::Jira };
    if uses != Some(wants) {
        return Err(Error::Other(format!(
            "{name} is not this workspace's: its tickets are {}",
            match uses {
                Some(TrackerKind::Jira) => "in Jira; use the jira_ tools",
                Some(TrackerKind::Github) => "GitHub issues; use the github_ tools",
                None => "in no tracker",
            }
        )));
    }
    if wants == TrackerKind::Github {
        return github_call(&state, name, &args).await;
    }

    match name {
        "jira_search" => {
            let jql = required(&args, "jql")?.to_string();
            // Clamped before the cast: a huge value wrapped to 0 and came back
            // empty, and one just under the wrap paged through the whole site.
            let max = args.get("max").and_then(|m| m.as_u64()).unwrap_or(25).clamp(1, 500) as u32;
            let (client, _) = commands::jira_client(&state)?;
            Ok(serde_json::to_value(client.search(&jql, max).await?.issues)?)
        }

        "jira_get_issue" => {
            let key = required(&args, "key")?.to_string();
            let (client, _) = commands::jira_client(&state)?;
            Ok(serde_json::to_value(client.issue(&key).await?)?)
        }

        "jira_issue_types" => Ok(serde_json::to_value(
            commands::ticket_types_inner(&state, false).await?,
        )?),

        "jira_create_fields" => {
            Ok(serde_json::to_value(
                commands::required_fields_for(
                    &state,
                    required(&args, "project_key")?,
                    required(&args, "issue_type_id")?,
                )
                .await?,
            )?)
        }

        "jira_create_issue" => {
            let (client, cfg) = commands::jira_client(&state)?;
            let project = arg(&args, "project_key")
                .map(str::to_string)
                .or_else(|| cfg.project_key.clone())
                .ok_or_else(|| {
                    crate::error::Error::Other(
                        "no project_key given and none configured in the app".into(),
                    )
                })?;

            let key = client
                .create_issue(
                    &project,
                    required(&args, "summary")?,
                    arg(&args, "description").unwrap_or_default(),
                    // Not "Task": that is one site's name for it.
                    &match arg(&args, "issue_type") {
                        Some(t) => t.to_string(),
                        None => commands::default_ticket_type(&state).await?,
                    },
                    arg(&args, "parent_key"),
                    // Without it jira_create_fields was advice nobody could
                    // act on: a project that demands a component refused
                    // every issue an agent filed.
                    // Some clients send an object argument as its JSON text.
                    &match args.get("fields") {
                        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                        Some(v) => v.clone(),
                        None => Value::Null,
                    },
                )
                .await?;
            Ok(json!({ "key": key, "url": format!("{}/browse/{key}", cfg.base_url) }))
        }

        "jira_comment" => {
            let (client, _) = commands::jira_client(&state)?;
            client
                .comment(required(&args, "key")?, required(&args, "text")?)
                .await?;
            Ok(json!({ "ok": true }))
        }

        "jira_transition" => {
            let key = required(&args, "key")?;
            let (client, _) = commands::jira_client(&state)?;
            let available = client.transitions(key).await?;

            let Some(wanted) = arg(&args, "transition") else {
                return Ok(serde_json::to_value(available)?);
            };
            let found = available
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(wanted))
                .ok_or_else(|| {
                    crate::error::Error::Other(format!(
                        "no transition named {wanted}; available: {}",
                        available
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
            client.transition(key, &found.id).await?;
            Ok(json!({ "ok": true, "status": found.to_status }))
        }

        other => Err(Error::NotFound(format!("tool {other}"))),
    }
}

async fn github_call(state: &AppState, name: &str, args: &Value) -> Result<Value> {
    let Tracker::Github(client, issues) = tracker(state)? else {
        return Err(Error::NotConfigured("GitHub"));
    };
    match name {
        "github_search_issues" => {
            let max = args.get("max").and_then(Value::as_u64).unwrap_or(25).clamp(1, 500) as usize;
            let query = required(args, "query")?;
            Ok(serde_json::to_value(search_for_agent(&client, &issues, &format!("is:issue {query}"), max).await?)?)
        }
        "github_get_issue" => {
            let one = in_workspace(&issues, required(args, "key")?)?;
            let found = client.issues(&[one]).await?;
            let issue = found.into_iter().next().ok_or_else(|| Error::NotFound(required(args, "key").unwrap_or_default().into()))?;
            Ok(serde_json::to_value(issue)?)
        }
        "github_create_issue" => {
            let parent = arg(args, "parent").map(|p| in_workspace(&issues, p).map(|_| p.to_string())).transpose()?;
            let repo = arg(args, "repo").map(str::trim).filter(|r| !r.is_empty());
            if let Some(repo) = repo {
                in_workspace(&issues, &format!("{repo}#1"))?;
            }
            let new = NewTicket {
                summary: required(args, "title")?.to_string(),
                description: arg(args, "body").unwrap_or_default().to_string(),
                issue_type: "Issue".into(),
                project_key: repo.map(str::to_string),
                parent_key: parent,
                fields: None,
            };
            let key = Tracker::Github(client, issues).file(&new).await?;
            Ok(json!({ "key": key }))
        }
        "github_comment" => {
            let one = in_workspace(&issues, required(args, "key")?)?;
            client.comment_issue(&one, required(args, "text")?).await?;
            Ok(json!({ "ok": true }))
        }
        "github_close_issue" => {
            let one = in_workspace(&issues, required(args, "key")?)?;
            let not_planned = args.get("not_planned").and_then(Value::as_bool).unwrap_or(false);
            client.close_issue(&one, not_planned).await?;
            Ok(json!({ "ok": true, "closed": if not_planned { "not planned" } else { "completed" } }))
        }
        other => Err(Error::NotFound(format!("tool {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: Vec<Value>) -> Vec<String> {
        tools.iter().filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string)).collect()
    }

    #[test]
    fn an_agent_is_offered_only_the_workspace_trackers_tools() {
        let jira = names(tracker_tools(Some(TrackerKind::Jira)));
        let github = names(tracker_tools(Some(TrackerKind::Github)));
        assert!(jira.iter().all(|n| n.starts_with("jira_")) && jira.contains(&"jira_transition".into()));
        assert!(github.iter().all(|n| n.starts_with("github_")) && github.contains(&"github_close_issue".into()));
        assert!(tracker_tools(None).is_empty());
        assert!(jira.iter().chain(&github).all(|n| tracker_owns(n)));
    }
}
