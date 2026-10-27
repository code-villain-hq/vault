//! The epic chat's tools (EPIC-12): what its sub-tasks are and how far each
//! is, starting one, and passing a message to one's agents. The chat
//! orchestrates; the work is the sub-tasks' own agents'.

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::config::Task;
use crate::error::{Error, Result};
use crate::mcp::{arg, bool_prop, required, str_prop, tool};
use crate::pty::{Activity, PaneKind};

use super::epics::{filed_of, start_sub_task_inner, view, waiting_for};
use super::AppState;

pub fn epic_tools() -> Vec<Value> {
    let epic = || str_prop("The epic's task id; by default the task you are running in");
    let sub = || str_prop("The sub-task: its title, ticket key or task id");
    vec![
        tool(
            "epic_subtasks",
            "An epic's sub-tasks, in the order they can start: each one's title, ticket, \
             task id, repositories, steps ticked of the total, the sub-tasks it waits for \
             that have not merged, its pull requests, its agents (running, and which need \
             the person), and its state: merged, changes, approved, in review, asking, \
             working, waits, ready or not filed.",
            json!({ "epic_id": epic() }),
            vec![],
        ),
        tool(
            "subtask_start",
            "Start a filed sub-task: its default agent on its spec, as Start in the epic does, \
             and move its ticket (and the epic's, the first time) to in progress. Refused \
             while a sub-task it waits for has not merged. Needs confirm: true after the \
             person agrees.",
            json!({
                "epic_id": epic(),
                "sub_task": sub(),
                "agent_id": str_prop("The agent CLI to start, e.g. claude; by default yours"),
                "confirm": bool_prop(
                    "Set true only after the person agrees. Calls without it are refused — \
                     this tool starts an agent and moves tickets.",
                ),
            }),
            vec!["sub_task"],
        ),
        tool(
            "subtask_tell",
            "Send one message to a sub-task's running agents: typed into a terminal and \
             submitted, or sent as a prompt to a conversation. Says how many were told.",
            json!({ "epic_id": epic(), "sub_task": sub(), "text": str_prop("The message") }),
            vec!["sub_task", "text"],
        ),
    ]
}

pub fn epic_owns(name: &str) -> bool {
    matches!(name, "epic_subtasks" | "subtask_start" | "subtask_tell")
}

/// The confirm gate is `mcp.rs`'s: `subtask_start` is in its `DANGER` list.
pub async fn epic_call(app: &AppHandle, name: &str, args: Value, caller: Option<&str>) -> Result<Value> {
    let state = app.state::<AppState>();
    let pane = caller.and_then(|p| state.ptys.info(p).ok());
    let epic_id = match arg(&args, "epic_id") {
        Some(id) => id.to_string(),
        None => pane.as_ref().map(|p| p.task_id.clone()).ok_or_else(|| Error::Other("name the epic: epic_id".into()))?,
    };
    let epic = state.config.task(&epic_id)?;
    if !epic.epic {
        return Err(Error::Other(format!("{} is not an epic; give epic_id", epic.name)));
    }
    match name {
        "epic_subtasks" => subtasks(app, &epic).await,
        "subtask_start" => {
            let task = sub_task(&state, &epic, required(&args, "sub_task")?)?;
            let agent = arg(&args, "agent_id")
                .map(str::to_string)
                .or_else(|| pane.and_then(|p| p.agent_id))
                .unwrap_or_else(|| "claude".into());
            let started = start_sub_task_inner(app, task.id.clone(), agent, false).await?;
            Ok(json!({ "started": task.name, "pane_id": started.pane.id, "ticket_moved_to": started.moved }))
        }
        "subtask_tell" => {
            let task = sub_task(&state, &epic, required(&args, "sub_task")?)?;
            let told = super::tasks::tell_agents(&state, &task.id, required(&args, "text")?);
            Ok(json!({ "told": told, "task": task.name }))
        }
        other => Err(Error::Other(format!("unknown tool {other}"))),
    }
}

/// A filed sub-task of `epic`, by title, ticket key or task id.
fn sub_task(state: &AppState, epic: &Task, which: &str) -> Result<Task> {
    let which = which.trim();
    filed_of(state, &epic.id)
        .into_iter()
        .find(|t| {
            t.id == which
                || t.issue_key.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(which))
                || t.sub_task.as_ref().is_some_and(|s| s.title.eq_ignore_ascii_case(which))
        })
        .ok_or_else(|| Error::Other(format!("{} has no filed sub-task {which}; epic_subtasks lists them", epic.name)))
}

async fn subtasks(app: &AppHandle, epic: &Task) -> Result<Value> {
    let state = app.state::<AppState>();
    let e = epic.clone();
    let tab = super::blocking(app.clone(), move |state| Ok(view(state, &e))).await?;
    let github = super::github::github_client(&state).ok();
    let mut out = Vec::new();
    for sub in tab.sub_tasks {
        let Some(task) = sub.task_id.as_deref().and_then(|id| state.config.task(id).ok()) else {
            out.push(json!({ "title": sub.title, "ticket": sub.key, "steps": sub.steps, "waits_for": sub.waits_for, "state": "not filed" }));
            continue;
        };
        let panes: Vec<_> = state.ptys.list(Some(&task.id)).into_iter().filter(|p| p.kind == PaneKind::Agent).collect();
        let running = panes.iter().filter(|p| p.running).count();
        let asking = panes.iter().filter(|p| p.running && p.activity == Activity::Asking).count();
        let prs = match &github {
            Some((client, _)) => super::github::task_prs(&state, client, &task.id).await.unwrap_or_default(),
            None => Vec::new(),
        };
        let live: Vec<_> = prs.iter().filter(|r| r.pr.as_ref().is_some_and(|p| p.merged || p.state == "open")).collect();
        let waits = waiting_for(app, &task).await.unwrap_or_default();
        // Verdicts only from the open ones, as the task list reads them: a
        // merged pull request's reviews are not fetched.
        let open: Vec<_> = live.iter().filter(|r| r.pr.as_ref().is_some_and(|p| !p.merged)).collect();
        let state_word = if !live.is_empty() && open.is_empty() {
            "merged"
        } else if open.iter().any(|r| r.verdict == "changes_requested") {
            "changes"
        } else if open.iter().all(|r| r.verdict == "approved") && !open.is_empty() {
            "approved"
        } else if !live.is_empty() {
            "in review"
        } else if asking > 0 {
            "asking"
        } else if running > 0 {
            "working"
        } else if !waits.is_empty() {
            "waits"
        } else {
            "ready"
        };
        out.push(json!({
            "title": sub.title,
            "ticket": sub.key,
            "task_id": task.id,
            "repos": sub.repos,
            "steps": sub.steps,
            "done": sub.done,
            "waits_for": waits,
            "pull_requests": live.iter().filter_map(|r| r.pr.as_ref().map(|p| json!({ "repo": r.repo, "url": p.url, "merged": p.merged, "verdict": r.verdict }))).collect::<Vec<_>>(),
            "agents": { "running": running, "need_you": asking },
            "state": state_word,
        }));
    }
    Ok(json!({ "epic": epic.name, "planned": tab.planned, "sub_tasks": out }))
}
