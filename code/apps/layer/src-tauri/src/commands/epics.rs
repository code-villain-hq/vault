//! Epics (§26): an epic's spec, approved once, makes its sub-tasks. Each is
//! filed in Jira under the epic and made a task of its own with its share of
//! the spec (EPIC-7), and started from the epic when what it waits for has
//! merged (EPIC-8).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::config::{SubTaskOf, Task};
use crate::error::{Error, Result};
use crate::integrations::jira::{Issue, Jira};
use crate::pty::PaneInfo;
use crate::spec::{self, Part, SubTask};

use super::jira::{default_issue_type, jira_client, project_of};
use super::panes::{agent_file_dir, start_agent, Resume};
use super::spec::{and_list, approve, spec_of, Slot, DRAFTS};
use super::task_context::{keep_ticket, write_task_context};
use super::tasks::{new_task, NewTask};
use super::AppState;

/// Under the epic's `DRAFTS`: tickets filed for sub-tasks, by title, so a
/// sub-task whose task could not be made is not filed twice (EPIC-7).
const FILED: &str = "sub-tasks.json";

/// What the Sub-tasks tab shows (EPIC-7, EPIC-8).
#[derive(Debug, Serialize)]
pub struct EpicView {
    /// Some repository's tasks are approved: there is a plan to file.
    pub planned: bool,
    pub sub_tasks: Vec<SubTaskView>,
}

#[derive(Debug, Serialize)]
pub struct SubTaskView {
    pub title: String,
    /// Its ticket: the one filed for it, or the one its heading names.
    pub key: Option<String>,
    /// Its heading named a ticket that was there before.
    pub existing: bool,
    /// Its repositories, by folder.
    pub repos: Vec<String>,
    /// The requirements its steps serve.
    pub serves: Vec<String>,
    pub steps: usize,
    pub done: usize,
    /// The titles of the sub-tasks it waits for (EPIC-5).
    pub waits_for: Vec<String>,
    /// Its task, once filed.
    pub task_id: Option<String>,
}

/// The epic's name without its ticket key: what a sub-task under no
/// heading is named after (EPIC-4).
fn title_of(epic: &Task) -> String {
    match &epic.issue_key {
        Some(key) => epic.name.strip_prefix(key.as_str()).unwrap_or(&epic.name).trim().to_string(),
        None => epic.name.clone(),
    }
}

fn an_epic(state: &AppState, epic_id: &str) -> Result<Task> {
    let epic = state.config.task(epic_id)?;
    if !epic.epic {
        return Err(Error::Other(format!("{} is not an epic", epic.name)));
    }
    Ok(epic)
}

/// The sub-tasks the epic's approved tasks make, and whether there are any
/// approved tasks to make them from.
pub(crate) fn plan(state: &AppState, epic: &Task) -> (bool, Vec<SubTask>) {
    let spec = spec_of(state, epic);
    let files: Vec<(String, String)> = spec
        .homes
        .iter()
        .filter_map(|h| Some((h.folder.clone(), spec.approved(&Slot::tasks(&h.folder))?)))
        .collect();
    (!files.is_empty(), spec::sub_tasks(&title_of(epic), &files))
}

/// The epic's sub-tasks filed so far.
pub(crate) fn filed_of(state: &AppState, epic_id: &str) -> Vec<Task> {
    state.config.with(|c| {
        c.tasks.iter().filter(|t| t.sub_task.as_ref().is_some_and(|s| s.epic == epic_id)).cloned().collect()
    })
}

fn is(task: &Task, title: &str) -> bool {
    task.sub_task.as_ref().is_some_and(|s| s.title.eq_ignore_ascii_case(title))
}

pub(crate) fn view(state: &AppState, epic: &Task) -> EpicView {
    let (planned, subs) = plan(state, epic);
    let filed = filed_of(state, &epic.id);
    let remembered = remembered(state, epic);
    let counts = |task: &Task| {
        let spec = spec_of(state, task);
        spec.homes.iter().map(|h| spec.steps(h)).fold((0, 0), |(n, d), s| (n + s.len(), d + s.iter().filter(|s| s.done).count()))
    };
    let mut out: Vec<SubTaskView> = subs
        .iter()
        .map(|sub| {
            let task = filed.iter().find(|t| is(t, &sub.title));
            let (steps, done) = match task {
                Some(t) => counts(t),
                None => sub.repos.iter().fold((0, 0), |(n, d), r| (n + r.steps.len(), d + r.steps.iter().filter(|s| s.done).count())),
            };
            SubTaskView {
                title: sub.title.clone(),
                key: task
                    .and_then(|t| t.issue_key.clone())
                    .or_else(|| sub.key.clone())
                    .or_else(|| remembered.get(&sub.title.to_lowercase()).cloned()),
                existing: sub.key.is_some(),
                repos: sub.repos.iter().map(|r| r.folder.clone()).collect(),
                serves: sub.serves(),
                steps,
                done,
                waits_for: sub.waits_for.clone(),
                task_id: task.map(|t| t.id.clone()),
            }
        })
        .collect();
    // Filed, and no longer in the plan: it stays filed (§26's gaps).
    for task in filed.iter().filter(|t| !subs.iter().any(|s| is(t, &s.title))) {
        let (steps, done) = counts(task);
        out.push(SubTaskView {
            title: task.sub_task.as_ref().map(|s| s.title.clone()).unwrap_or_default(),
            key: task.issue_key.clone(),
            existing: false,
            repos: state.config.checkouts_of(&task.id).into_iter().filter_map(|c| folder(&c.path)).collect(),
            serves: Vec::new(),
            steps,
            done,
            waits_for: Vec::new(),
            task_id: Some(task.id.clone()),
        });
    }
    EpicView { planned, sub_tasks: out }
}

fn folder(path: &str) -> Option<String> {
    Some(PathBuf::from(path).file_name()?.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn epic_view(app: AppHandle, epic_id: String) -> Result<EpicView> {
    super::blocking(app, move |state| Ok(view(state, &an_epic(state, &epic_id)?))).await
}

fn filed_file(state: &AppState, epic: &Task) -> Option<PathBuf> {
    agent_file_dir(state, epic).map(|d| d.join(DRAFTS).join(FILED))
}

/// Tickets filed for sub-tasks, by lowercased title.
fn remembered(state: &AppState, epic: &Task) -> HashMap<String, String> {
    filed_file(state, epic)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn remember(state: &AppState, epic: &Task, title: &str, key: &str) -> Result<()> {
    let Some(path) = filed_file(state, epic) else { return Ok(()) };
    let mut all = remembered(state, epic);
    all.insert(title.to_lowercase(), key.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

/// What filing did: the tab as it is now, and each sub-task that could not
/// be filed, with why.
#[derive(Debug, Serialize)]
pub struct Filed {
    pub epic: EpicView,
    pub failed: Vec<SubTaskFailure>,
}

#[derive(Debug, Serialize)]
pub struct SubTaskFailure {
    pub title: String,
    pub error: String,
}

/// One sub-task to file, with what filing needs from the epic.
struct ToFile {
    sub: SubTask,
    description: String,
    project_ids: Vec<String>,
    remembered: Option<String>,
}

/// File the sub-tasks not filed yet (EPIC-7): a ticket under the epic, a
/// task, and its share of the spec, approved. No agent starts and no ticket
/// moves.
#[tauri::command]
pub async fn file_sub_tasks(app: AppHandle, epic_id: String, issue_type: Option<String>, fields: Option<Value>) -> Result<Filed> {
    let state = app.state::<AppState>();
    let epic = an_epic(&state, &epic_id)?;
    let e = epic.clone();
    let (planned, todo) = super::blocking(app.clone(), move |state| Ok(to_file(state, &e))).await?;
    if !planned {
        return Err(Error::Other("approve the epic's tasks first: they say what its sub-tasks are".into()));
    }
    let jira = match (&epic.issue_key, jira_client(&state)) {
        (_, Ok((client, _))) => Some(client),
        (Some(_), Err(e)) => return Err(e),
        (None, Err(_)) => None,
    };
    let issue_type = match (&jira, epic.issue_key.is_some(), issue_type.filter(|t| !t.trim().is_empty())) {
        (Some(_), true, Some(t)) => t,
        (Some(_), true, None) => default_issue_type(&state).await?,
        _ => String::new(),
    };
    let fields = fields.unwrap_or(Value::Null);
    let mut failed = Vec::new();
    for one in todo {
        let title = one.sub.title.clone();
        if let Err(e) = file_one(&app, &epic, one, jira.as_ref(), &issue_type, &fields).await {
            failed.push(SubTaskFailure { title, error: e.to_string() });
        }
    }
    let epic = super::blocking(app, move |state| Ok(view(state, &an_epic(state, &epic_id)?))).await?;
    Ok(Filed { epic, failed })
}

fn to_file(state: &AppState, epic: &Task) -> (bool, Vec<ToFile>) {
    let (planned, subs) = plan(state, epic);
    let filed = filed_of(state, &epic.id);
    let spec = spec_of(state, epic);
    let requirements = spec.requirements();
    let remembered = remembered(state, epic);
    let todo = subs
        .into_iter()
        .filter(|sub| !filed.iter().any(|t| is(t, &sub.title)))
        .map(|sub| {
            let mut d = format!("Part of {}.\n", epic.name);
            let serves = sub.serves();
            if !serves.is_empty() {
                d.push_str("\nRequirements:\n");
                for r in requirements.iter().filter(|r| serves.contains(&r.id)) {
                    d.push_str(&format!("- {}: {}\n", r.id, r.text));
                }
            }
            for repo in &sub.repos {
                d.push_str(&format!("\nSteps in {}:\n", repo.folder));
                for s in &repo.steps {
                    d.push_str(&format!("- {}. {}\n", s.number, s.text));
                }
            }
            let project_ids = sub
                .repos
                .iter()
                .filter_map(|r| spec.home(&r.folder).map(|h| h.checkout.project_id.clone()))
                .collect();
            let remembered = remembered.get(&sub.title.to_lowercase()).cloned();
            ToFile { sub, description: d, project_ids, remembered }
        })
        .collect();
    (planned, todo)
}

async fn file_one(app: &AppHandle, epic: &Task, one: ToFile, jira: Option<&Jira>, issue_type: &str, fields: &Value) -> Result<()> {
    let state = app.state::<AppState>();
    let ToFile { sub, description, project_ids, remembered } = one;
    if project_ids.is_empty() {
        return Err(Error::Other("none of its repositories is in the epic".into()));
    }
    let issue: Option<Issue> = match (jira, sub.key.clone().or(remembered), epic.issue_key.as_deref()) {
        (Some(client), Some(key), parent) => {
            if let (Some(parent), Some(_)) = (parent, &sub.key) {
                client.set_parent(&key, parent).await.map_err(|e| Error::Other(format!("{key} could not be moved under the epic: {e}")))?;
            }
            Some(client.issue(&key).await?)
        }
        (Some(client), None, Some(parent)) => {
            let project = project_of(parent).ok_or_else(|| Error::Other(format!("{parent} is not a ticket key")))?;
            let key = client.create_issue(&project, &sub.title, &description, issue_type, Some(parent), fields).await?;
            let (e, t, k) = (epic.clone(), sub.title.clone(), key.clone());
            super::blocking(app.clone(), move |state| remember(state, &e, &t, &k)).await?;
            Some(client.issue(&key).await?)
        }
        _ => None,
    };

    let (key, url) = (issue.as_ref().map(|i| i.key.clone()), issue.as_ref().map(|i| i.url.clone()));
    // A ticket that already has a task is that task: a second would check
    // the same branch out twice.
    let had = key.as_deref().and_then(|k| state.config.with(|c| c.tasks.iter().find(|t| t.issue_key.as_deref() == Some(k)).cloned()));
    let of = SubTaskOf { epic: epic.id.clone(), title: sub.title.clone() };
    if let Some(task) = had {
        let id = task.id.clone();
        state.config.update(move |c| {
            if let Some(t) = c.tasks.iter_mut().find(|t| t.id == id) {
                t.sub_task = Some(of);
            }
        })?;
        return Ok(());
    }

    let new = NewTask {
        name: match &key {
            Some(k) => format!("{k} {}", sub.title),
            None => sub.title.clone(),
        },
        project_ids,
        branch: None,
        branch_suffix: None,
        base: None,
        issue_key: key,
        issue_url: url,
        epic_key: epic.issue_key.clone(),
        epic: false,
        sub_task: Some(of),
    };
    let epic = epic.clone();
    let task = super::blocking(app.clone(), move |state| {
        let task = new_task(state, new)?;
        let from = spec_of(state, &epic);
        let mut texts: Vec<(Slot, String)> = [Part::Requirements, Part::Design]
            .into_iter()
            .filter_map(|p| Some((Slot::shared(p), from.approved(&Slot::shared(p))?)))
            .collect();
        texts.extend(sub.repos.iter().map(|r| (Slot::tasks(&r.folder), r.tasks.clone())));
        let failed = approve(&spec_of(state, &task), &task, &texts, Some(from.kind()))?;
        write_task_context(state, &task)?;
        if !failed.is_empty() {
            let where_: Vec<String> = failed.iter().map(|f| format!("{} ({})", f.folder, f.error)).collect();
            return Err(Error::Other(format!(
                "its task was made, but its spec could not be committed in {}",
                and_list(&where_.iter().map(String::as_str).collect::<Vec<_>>())
            )));
        }
        Ok(task)
    })
    .await?;
    if let Some(issue) = issue {
        keep_ticket(app.clone(), task, issue).await;
    }
    Ok(())
}

/// The sub-tasks `task` waits for that have not merged. One not filed, or
/// whose pull requests cannot be read, has not.
pub(crate) async fn waiting_for(app: &AppHandle, task: &Task) -> Result<Vec<String>> {
    let Some(of) = task.sub_task.clone() else { return Ok(Vec::new()) };
    let state = app.state::<AppState>();
    let epic = state.config.task(&of.epic)?;
    let (waits, filed) = super::blocking(app.clone(), move |state| {
        let (_, subs) = plan(state, &epic);
        let waits = subs.into_iter().find(|s| s.title.eq_ignore_ascii_case(&of.title)).map(|s| s.waits_for).unwrap_or_default();
        Ok((waits, filed_of(state, &of.epic)))
    })
    .await?;
    let github = super::github::github_client(&state).ok();
    let mut open = Vec::new();
    for title in waits {
        let merged = match (filed.iter().find(|t| is(t, &title)), &github) {
            (Some(t), Some((client, _))) => super::github::task_prs(&state, client, &t.id)
                .await
                .map(|rows| {
                    let live: Vec<_> = rows.iter().filter_map(|r| r.pr.as_ref()).filter(|p| p.merged || p.state == "open").collect();
                    !live.is_empty() && live.iter().all(|p| p.merged)
                })
                .unwrap_or(false),
            _ => false,
        };
        if !merged {
            open.push(title);
        }
    }
    Ok(open)
}

/// A sub-task started, and the status its ticket was moved to.
#[derive(Debug, Serialize)]
pub struct StartedSubTask {
    pub pane: PaneInfo,
    pub moved: Option<String>,
}

/// Start `agent_id` on a sub-task's spec, as Start agent does (SPEC-12),
/// and move its ticket, and the epic's while that is still to do, as Start
/// work does (EPIC-8). One that waits is refused unless `anyway`.
pub(crate) async fn start_sub_task_inner(app: &AppHandle, task_id: String, agent_id: String, anyway: bool) -> Result<StartedSubTask> {
    let state = app.state::<AppState>();
    let task = state.config.task(&task_id)?;
    let of = task.sub_task.clone().ok_or_else(|| Error::Other(format!("{} is not a sub-task of an epic", task.name)))?;
    if !anyway {
        let open = waiting_for(app, &task).await?;
        if !open.is_empty() {
            let open: Vec<&str> = open.iter().map(String::as_str).collect();
            return Err(Error::Other(format!("{} waits for {}, not merged yet", of.title, and_list(&open))));
        }
    }
    let prompt = super::jira::task_prompt(app.clone(), app.state(), task_id.clone(), None).await?;
    let work = super::spec_work::spec_work_prompt(app.clone(), task_id.clone(), None).await?;
    let prompt = [prompt, work].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n");
    let handle = app.clone();
    let pane = super::blocking(app.clone(), move |state| {
        start_agent(&handle, state, task_id, agent_id, None, Some(prompt), Resume::No, None, None, false)
    })
    .await?;

    let moved = match &task.issue_key {
        Some(key) => super::ticket_flow::started(&state, key).await,
        None => None,
    };
    if let (Some(key), Ok((client, _))) = (state.config.task(&of.epic)?.issue_key, jira_client(&state)) {
        if client.issue(&key).await.is_ok_and(|i| i.status_category == "new") {
            super::ticket_flow::started(&state, &key).await;
        }
    }
    Ok(StartedSubTask { pane, moved })
}

#[tauri::command]
pub async fn start_sub_task(app: AppHandle, task_id: String, agent_id: String, anyway: Option<bool>) -> Result<StartedSubTask> {
    start_sub_task_inner(&app, task_id, agent_id, anyway.unwrap_or(false)).await
}
