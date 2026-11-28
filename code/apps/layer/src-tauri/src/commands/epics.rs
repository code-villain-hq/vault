//! Epics (§26): an epic's spec, written once, makes its sub-tasks. Each is
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
use crate::pty::PaneInfo;
use crate::spec::owned::{own_part, owned_steps};
use crate::spec::{self, glob, Requirement, SubTask};
use crate::tickets::{NewTicket, Ticket, TicketPage, Tracker};

use super::panes::{agent_file_dir, start_agent, Resume};
use super::spec_commit::repositories;
use super::spec_copy::{list_files, not_text, read_file, spec_dir, toolset_of};
use super::spec_old::DRAFTS;
use super::spec_view::step_texts;
use super::spec_work::write_spec;
use super::task_context::{entry_file, keep_ticket, write_task_context};
use super::tasks::{new_task, NewTask};
use super::tickets::{default_ticket_type, tracker};
use super::AppState;

/// Under the epic's `DRAFTS`: tickets filed for sub-tasks, by title, so a
/// sub-task whose task could not be made is not filed twice (EPIC-7).
const FILED: &str = "sub-tasks.json";

/// What the Sub-tasks tab shows (EPIC-7, EPIC-8).
#[derive(Debug, Serialize)]
pub struct EpicView {
    /// The spec has steps: there is a plan to file.
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

/// The sub-tasks the epic's steps make, and whether it has any steps to make
/// them from. Each repository's share of the epic's step files (its own
/// sections and steps, and those that belong to everyone) is read as that
/// repository's tasks, and a `##` heading shared across repositories is one
/// sub-task (EPIC-4).
pub(crate) fn plan(state: &AppState, epic: &Task) -> (bool, Vec<SubTask>) {
    let folders: Vec<String> = repositories(state, epic).into_iter().map(|r| r.folder).collect();
    let texts = step_texts(state, epic);
    let files: Vec<(String, String)> = folders
        .iter()
        .filter_map(|folder| {
            let own: Vec<String> = texts.iter().map(|(_, text)| own_part(text, folder, &folders)).collect();
            let own = own.join("\n");
            (!spec::steps(&own).is_empty()).then(|| (folder.clone(), own))
        })
        .collect();
    (!files.is_empty(), spec::sub_tasks(&title_of(epic), &files))
}

/// The requirements the epic's spec names, from its entry file.
fn requirements_of(state: &AppState, epic: &Task) -> Vec<Requirement> {
    entry_file(&toolset_of(state, epic), &spec_dir(state, epic))
        .map(|(_, text)| spec::requirements(&text))
        .unwrap_or_default()
}

/// How many steps a task's spec has, and how many are ticked.
fn counts(state: &AppState, task: &Task) -> (usize, usize) {
    step_texts(state, task)
        .iter()
        .flat_map(|(_, text)| owned_steps(text, &[]))
        .fold((0, 0), |(n, d), s| (n + 1, d + usize::from(s.done)))
}

/// The epic's sub-tasks filed so far.
pub(crate) fn filed_of(state: &AppState, epic_id: &str) -> Vec<Task> {
    state
        .config
        .with(|c| c.tasks.iter().filter(|t| t.sub_task.as_ref().is_some_and(|s| s.epic == epic_id)).cloned().collect())
}

fn is(task: &Task, title: &str) -> bool {
    task.sub_task.as_ref().is_some_and(|s| s.title.eq_ignore_ascii_case(title))
}

pub(crate) fn view(state: &AppState, epic: &Task) -> EpicView {
    let (planned, subs) = plan(state, epic);
    let filed = filed_of(state, &epic.id);
    let remembered = remembered(state, epic);
    let mut out: Vec<SubTaskView> = subs
        .iter()
        .map(|sub| {
            let task = filed.iter().find(|t| is(t, &sub.title));
            let (steps, done) = match task {
                Some(t) => counts(state, t),
                None => sub
                    .repos
                    .iter()
                    .fold((0, 0), |(n, d), r| (n + r.steps.len(), d + r.steps.iter().filter(|s| s.done).count())),
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
        let (steps, done) = counts(state, task);
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

/// The tickets under the epic's own that are not done (EPIC-14), and whether
/// its tracker had more. None for an epic with no ticket.
pub(crate) async fn tickets_under(app: &AppHandle, epic: &Task) -> Result<TicketPage> {
    let Some(key) = epic.issue_key.as_deref() else {
        return Ok(TicketPage { issues: Vec::new(), more: false });
    };
    let mut page = tracker(&app.state::<AppState>())?.children(key).await?;
    page.issues.retain(|t| t.status_category != "done");
    Ok(page)
}

/// The tickets under the epic's that are not its sub-tasks here yet
/// (EPIC-14): no task has one, and no heading of its plan names one.
pub(crate) async fn tickets_not_here(app: &AppHandle, epic: &Task) -> Result<TicketPage> {
    let mut page = tickets_under(app, epic).await?;
    let e = epic.clone();
    let here = super::blocking(app.clone(), move |state| Ok(keys_here(state, &e))).await?;
    page.issues.retain(|t| !here.contains(&t.key));
    Ok(page)
}

#[tauri::command]
pub async fn epic_tickets(app: AppHandle, epic_id: String) -> Result<TicketPage> {
    let epic = an_epic(&app.state::<AppState>(), &epic_id)?;
    tickets_not_here(&app, &epic).await
}

/// Every ticket a sub-task of the epic has, or will have once filed: the
/// keys its plan's headings name, those filed for it, and every task's.
fn keys_here(state: &AppState, epic: &Task) -> Vec<String> {
    let (_, subs) = plan(state, epic);
    let mut keys: Vec<String> = subs.into_iter().filter_map(|s| s.key).collect();
    keys.extend(remembered(state, epic).into_values());
    keys.extend(state.config.with(|c| c.tasks.iter().filter_map(|t| t.issue_key.clone()).collect::<Vec<_>>()));
    keys
}

/// Make `task_ids` the epic's sub-tasks (EPIC-13): tasks whose tickets sit
/// under the epic's in Jira, which the window finds as it already asks Jira
/// for each task's epic (TASK-17). The epic's view, after.
#[tauri::command]
pub async fn adopt_sub_tasks(app: AppHandle, epic_id: String, task_ids: Vec<String>) -> Result<EpicView> {
    super::blocking(app, move |state| {
        let epic = an_epic(state, &epic_id)?;
        let of = epic.clone();
        state.config.update(move |c| adopt(&mut c.tasks, &of, &task_ids))?;
        Ok(view(state, &epic))
    })
    .await
}

/// Links each of `ids` to `epic`, named after its ticket's summary, but
/// never an epic, a task already some epic's sub-task, or a task on the
/// epic's own ticket: one made before epics, which stays an ordinary task.
fn adopt(tasks: &mut [Task], epic: &Task, ids: &[String]) {
    for t in tasks.iter_mut().filter(|t| ids.contains(&t.id)) {
        if t.epic
            || t.id == epic.id
            || t.sub_task.is_some()
            || (epic.issue_key.is_some() && t.issue_key == epic.issue_key)
        {
            continue;
        }
        t.sub_task = Some(SubTaskOf { epic: epic.id.clone(), title: title_of(t) });
    }
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
/// task, and its share of the spec. No agent starts and no ticket
/// moves.
#[tauri::command]
pub async fn file_sub_tasks(
    app: AppHandle,
    epic_id: String,
    issue_type: Option<String>,
    fields: Option<Value>,
) -> Result<Filed> {
    let state = app.state::<AppState>();
    let epic = an_epic(&state, &epic_id)?;
    let e = epic.clone();
    let (planned, todo) = super::blocking(app.clone(), move |state| Ok(to_file(state, &e))).await?;
    if !planned {
        return Err(Error::Other("write the epic's tasks first: they say what its sub-tasks are".into()));
    }
    let tracker = match (&epic.issue_key, tracker(&state)) {
        (_, Ok(tracker)) => Some(tracker),
        (Some(_), Err(e)) => return Err(e),
        (None, Err(_)) => None,
    };
    let issue_type = match (&tracker, epic.issue_key.is_some(), issue_type.filter(|t| !t.trim().is_empty())) {
        (Some(_), true, Some(t)) => t,
        (Some(_), true, None) => default_ticket_type(&state).await?,
        _ => String::new(),
    };
    let fields = fields.unwrap_or(Value::Null);
    let mut failed = Vec::new();
    for one in todo {
        let title = one.sub.title.clone();
        if let Err(e) = file_one(&app, &epic, one, tracker.as_ref(), &issue_type, &fields).await {
            failed.push(SubTaskFailure { title, error: e.to_string() });
        }
    }
    let epic = super::blocking(app, move |state| Ok(view(state, &an_epic(state, &epic_id)?))).await?;
    Ok(Filed { epic, failed })
}

fn to_file(state: &AppState, epic: &Task) -> (bool, Vec<ToFile>) {
    let (planned, subs) = plan(state, epic);
    let filed = filed_of(state, &epic.id);
    let requirements = requirements_of(state, epic);
    let checkouts = state.config.checkouts_of(&epic.id);
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
                .filter_map(|r| {
                    checkouts
                        .iter()
                        .find(|c| folder(&c.path).as_deref() == Some(r.folder.as_str()))
                        .map(|c| c.project_id.clone())
                })
                .collect();
            let remembered = remembered.get(&sub.title.to_lowercase()).cloned();
            ToFile { sub, description: d, project_ids, remembered }
        })
        .collect();
    (planned, todo)
}

async fn file_one(
    app: &AppHandle,
    epic: &Task,
    one: ToFile,
    tracker: Option<&Tracker>,
    issue_type: &str,
    fields: &Value,
) -> Result<()> {
    let state = app.state::<AppState>();
    let ToFile { sub, description, project_ids, remembered } = one;
    if project_ids.is_empty() {
        return Err(Error::Other("none of its repositories is in the epic".into()));
    }
    let issue: Option<Ticket> = match (tracker, sub.key.clone().or(remembered), epic.issue_key.as_deref()) {
        (Some(client), Some(key), parent) => {
            if let (Some(parent), Some(_)) = (parent, &sub.key) {
                client
                    .put_under(&key, parent)
                    .await
                    .map_err(|e| Error::Other(format!("{key} could not be moved under the epic: {e}")))?;
            }
            Some(client.ticket(&key).await?)
        }
        (Some(client), None, Some(parent)) => {
            let new = NewTicket {
                summary: sub.title.clone(),
                description: description.clone(),
                issue_type: issue_type.to_string(),
                project_key: None,
                parent_key: Some(parent.to_string()),
                fields: Some(fields.clone()),
            };
            let key = client.file(&new).await?;
            let (e, t, k) = (epic.clone(), sub.title.clone(), key.clone());
            super::blocking(app.clone(), move |state| remember(state, &e, &t, &k)).await?;
            Some(client.ticket(&key).await?)
        }
        _ => None,
    };

    let (key, url) = (issue.as_ref().map(|i| i.key.clone()), issue.as_ref().map(|i| i.url.clone()));
    // A ticket that already has a task is that task: a second would check
    // the same branch out twice.
    let had = key
        .as_deref()
        .and_then(|k| state.config.with(|c| c.tasks.iter().find(|t| t.issue_key.as_deref() == Some(k)).cloned()));
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
        toolset: None,
    };
    let epic = epic.clone();
    let task = super::blocking(app.clone(), move |state| {
        let task = new_task(state, new)?;
        seed_sub_task(state, &epic, &task, &sub)?;
        write_task_context(state, &task)?;
        Ok(task)
    })
    .await?;
    if let Some(issue) = issue {
        keep_ticket(app.clone(), task, issue).await;
    }
    Ok(())
}

/// Give a sub-task its share of the epic's spec (EPIC-7): the epic's files
/// that are not steps (requirements, design), whole, and its steps, in the
/// epic's first step file, a `# <folder>` section for each repository it works
/// in. Nothing is committed: that is Commit spec's, when the user wants it.
fn seed_sub_task(state: &AppState, epic: &Task, task: &Task, sub: &SubTask) -> Result<()> {
    let (t, dir) = (toolset_of(state, epic), spec_dir(state, epic));
    let holds = |rel: &str| t.steps.iter().any(|g| glob::matches(g, rel));
    let files = list_files(&dir, &t)?;
    for rel in files.iter().filter(|rel| !holds(rel)) {
        match read_file(&dir, &t, rel) {
            Ok(text) => write_spec(state, task, rel, &text)?,
            // Not text: it cannot be written as one, and is not seeded.
            Err(e) if not_text(&e) => {}
            Err(e) => return Err(e),
        }
    }
    if let Some(rel) = files.iter().find(|rel| holds(rel)) {
        let sections: Vec<String> = sub
            .repos
            .iter()
            .map(|r| {
                // `## Tasks` is the old way of heading the list.
                let body = r.tasks.trim().strip_prefix("## Tasks").unwrap_or(&r.tasks).trim();
                format!("# {}\n\n{body}\n", r.folder)
            })
            .collect();
        write_spec(state, task, rel, &sections.join("\n"))?;
    }
    Ok(())
}

fn and_list(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The sub-tasks `task` waits for that have not merged. One not filed, or
/// whose pull requests cannot be read, has not.
pub(crate) async fn waiting_for(app: &AppHandle, task: &Task) -> Result<Vec<String>> {
    let Some(of) = task.sub_task.clone() else { return Ok(Vec::new()) };
    let state = app.state::<AppState>();
    let epic = state.config.task(&of.epic)?;
    let (waits, filed) = super::blocking(app.clone(), move |state| {
        let (_, subs) = plan(state, &epic);
        let waits =
            subs.into_iter().find(|s| s.title.eq_ignore_ascii_case(&of.title)).map(|s| s.waits_for).unwrap_or_default();
        Ok((waits, filed_of(state, &of.epic)))
    })
    .await?;
    let github = super::github::connected_github(&state).await.ok();
    let mut open = Vec::new();
    for title in waits {
        let merged = match (filed.iter().find(|t| is(t, &title)), &github) {
            (Some(t), Some(connected)) => super::github::task_prs(&state, connected, &t.id)
                .await
                .map(|rows| {
                    let live: Vec<_> =
                        rows.iter().filter_map(|r| r.pr.as_ref()).filter(|p| p.merged || p.state == "open").collect();
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

/// Start `agent_id` on a sub-task's spec, with the work prompt (SPEC-12),
/// and move its ticket, and the epic's while that is still to do, as Start
/// work does (EPIC-8). One that waits is refused unless `anyway`.
pub(crate) async fn start_sub_task_inner(
    app: &AppHandle,
    task_id: String,
    agent_id: String,
    anyway: bool,
) -> Result<StartedSubTask> {
    let state = app.state::<AppState>();
    let task = state.config.task(&task_id)?;
    let of =
        task.sub_task.clone().ok_or_else(|| Error::Other(format!("{} is not a sub-task of an epic", task.name)))?;
    if !anyway {
        let open = waiting_for(app, &task).await?;
        if !open.is_empty() {
            let open: Vec<&str> = open.iter().map(String::as_str).collect();
            return Err(Error::Other(format!("{} waits for {}, not merged yet", of.title, and_list(&open))));
        }
    }
    let opening = super::jira::bare_prompt(app.clone(), &app.state(), &task, None).await?;
    let id = task_id.clone();
    let work = super::blocking(app.clone(), move |state| super::spec_work::work_prompt(state, &id, None)).await?;
    // The spec's line goes after the work prompt, not between it and the opening.
    let prompt = super::jira::with_spec(app.clone(), task.clone(), sub_task_prompt(opening, work)).await?;
    let handle = app.clone();
    let pane = super::blocking(app.clone(), move |state| {
        start_agent(&handle, state, task_id, agent_id, None, Some(prompt), Resume::No, None, None, false)
    })
    .await?;

    let moved = match &task.issue_key {
        Some(key) => super::ticket_flow::started(&state, key).await,
        None => None,
    };
    if let (Some(key), Ok(tracker)) = (state.config.task(&of.epic)?.issue_key, tracker(&state)) {
        if tracker.ticket(&key).await.is_ok_and(|i| i.status_category == "new") {
            super::ticket_flow::started(&state, &key).await;
        }
    }
    Ok(StartedSubTask { pane, moved })
}

/// A sub-task's opening prompt: the briefing, then the steps to work through
/// when it has any.
pub(crate) fn sub_task_prompt(opening: String, work: String) -> String {
    [opening, work].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

#[tauri::command]
pub async fn start_sub_task(
    app: AppHandle,
    task_id: String,
    agent_id: String,
    anyway: Option<bool>,
) -> Result<StartedSubTask> {
    start_sub_task_inner(&app, task_id, agent_id, anyway.unwrap_or(false)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, key: &str) -> Task {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": format!("{key} Work on {id}"), "root": "/tmp", "branch": key,
            "issue_key": key, "created_at": "2026-10-27T00:00:00Z",
        }))
        .unwrap()
    }

    #[test]
    fn tasks_under_an_epics_ticket_become_its_sub_tasks_named_after_their_tickets() {
        let mut epic = task("e", "ACME-1");
        epic.epic = true;
        let mut tasks = vec![epic.clone(), task("a", "ACME-2"), task("b", "ACME-3")];
        adopt(&mut tasks, &epic, &["a".into()]);
        assert_eq!(tasks[1].sub_task, Some(SubTaskOf { epic: "e".into(), title: "Work on a".into() }));
        assert_eq!(tasks[2].sub_task, None);
    }

    #[test]
    fn adopting_leaves_epics_other_epics_sub_tasks_and_the_epics_own_old_task_alone() {
        let mut epic = task("e", "ACME-1");
        epic.epic = true;
        let mut other = task("o", "ACME-9");
        other.epic = true;
        let mut theirs = task("s", "ACME-4");
        theirs.sub_task = Some(SubTaskOf { epic: "o".into(), title: "Theirs".into() });
        let old = task("old", "ACME-1");
        let mut tasks = vec![epic.clone(), other, theirs, old];
        adopt(&mut tasks, &epic, &["e".into(), "o".into(), "s".into(), "old".into()]);
        assert_eq!(tasks[0].sub_task, None);
        assert_eq!(tasks[1].sub_task, None);
        assert_eq!(tasks[2].sub_task.as_ref().map(|s| s.epic.as_str()), Some("o"));
        assert_eq!(tasks[3].sub_task, None);
    }

    use crate::commands::spec_world::{get, put, world};

    const PLAN: &str = "# api\n\n## Session refresh\n- [ ] 1. Add the endpoint (R-1)\n\n## Audit\n- [x] 2. Log it (R-2)\n\n# web\n\n## Session refresh\n- [ ] 1. Call it (R-1; after api 1)\n";

    #[test]
    fn an_epics_steps_in_its_working_copy_are_planned_into_sub_tasks_per_repository() {
        let (root, state, epic) = world(&["api", "web"]);
        assert!(!plan(&state, &epic).0, "nothing to plan from yet");
        put(&root.join("tasks/T-1/spec/tasks.md"), PLAN);

        let (planned, subs) = plan(&state, &epic);

        assert!(planned);
        let titles: Vec<&str> = subs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Session refresh", "Audit"]);
        let refresh = &subs[0];
        let repos: Vec<&str> = refresh.repos.iter().map(|r| r.folder.as_str()).collect();
        assert_eq!(repos, ["api", "web"], "one sub-task across both");
        assert_eq!(subs[1].repos.len(), 1);
        assert_eq!(subs[1].repos[0].folder, "api");
        assert_eq!(counts(&state, &epic), (3, 1));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_sub_task_gets_the_epics_requirements_and_its_own_steps_in_its_own_spec() {
        let (root, state, epic) = world(&["api", "web"]);
        let spec = root.join("tasks/T-1/spec");
        put(&spec.join("requirements.md"), "## Requirements\n- R-1 [api, web]: x\n");
        put(&spec.join("design.md"), "## Approach\ny\n");
        put(&spec.join("tasks.md"), PLAN);
        std::fs::write(spec.join("bugfix.md"), [0xff, 0xfe]).unwrap();
        let sub_root = root.join("tasks/T-2");
        std::fs::create_dir_all(&sub_root).unwrap();
        let mut task = epic.clone();
        task.id = "t2".into();
        task.branch = "T-2".into();
        task.root = sub_root.to_string_lossy().to_string();
        task.toolset = Some("kiro".into());
        state.config.update(|c| c.tasks.push(task.clone())).unwrap();
        let (_, subs) = plan(&state, &epic);

        seed_sub_task(&state, &epic, &task, &subs[1]).unwrap();

        let mine = sub_root.join("spec");
        assert_eq!(get(&mine.join("requirements.md")), "## Requirements\n- R-1 [api, web]: x\n");
        assert_eq!(get(&mine.join("design.md")), "## Approach\ny\n");
        assert!(!mine.join("bugfix.md").exists(), "a file that is not text is not seeded, and does not fail the rest");
        let steps = get(&mine.join("tasks.md"));
        assert!(steps.contains("# api") && steps.contains("[x] 2. Log it") && !steps.contains("Call it"), "{steps}");
        assert!(!steps.contains("# web"), "{steps}");
        assert_eq!(counts(&state, &task), (1, 1));
        assert_eq!(get(&spec.join("tasks.md")), PLAN, "the epic's own is left as it was");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_list_of_titles_reads_as_a_sentence() {
        assert_eq!(and_list(&["a", "b", "c"]), "a, b and c");
        assert_eq!(and_list(&["a"]), "a");
    }
}
