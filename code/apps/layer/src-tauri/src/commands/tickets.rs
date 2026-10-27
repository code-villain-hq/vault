//! Tickets from the workspace's tracker (TKT-15): your queue, finding,
//! filing, and starting work on one. Whatever the tracker, through `Tracker`.

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::config::{Task, TrackerKind};
use crate::error::{Error, Result};
use crate::tickets::{Find, NewTicket, RequiredField, Ticket, TicketPage, TicketType, Tracker};

use super::jira::{learn_epic_field, task_repos, ticket_prompt};
use super::panes::{start_agent, Resume};
use super::task_context::keep_ticket;
use super::tasks::{new_task, NewTask};
use super::AppState;

pub(crate) fn tracker(state: &AppState) -> Result<Tracker> {
    state.config.with(Tracker::connect)
}

/// Every type of ticket the tracker defines, with its own icon. Nothing about
/// types is hardcoded — a site with custom types renders exactly as it does
/// in its tracker.
#[tauri::command]
pub async fn ticket_types(state: State<'_, AppState>, refresh: Option<bool>) -> Result<Vec<TicketType>> {
    ticket_types_inner(&state, refresh == Some(true)).await
}

pub(crate) async fn ticket_types_inner(state: &AppState, refresh: bool) -> Result<Vec<TicketType>> {
    if !refresh {
        if let Some(cached) = state.ticket_types.lock().clone() {
            return Ok(cached);
        }
    }
    let types = tracker(state)?.types().await?;
    *state.ticket_types.lock() = Some(types.clone());
    Ok(types)
}

/// The tracker's priorities, highest first, for sorting your list by them
/// (TKT-13). The UI asks once per connection.
#[tauri::command]
pub async fn ticket_priorities(state: State<'_, AppState>) -> Result<Vec<String>> {
    tracker(&state)?.priorities().await
}

#[tauri::command]
pub async fn tickets_mine(app: AppHandle, state: State<'_, AppState>) -> Result<TicketPage> {
    let page = my_tickets(&state).await?;
    crate::news::saw_tickets(&app, &page.issues);
    Ok(page)
}

/// Your queue, for the UI and for the watch that banners what is new.
pub(crate) async fn my_tickets(state: &AppState) -> Result<TicketPage> {
    if state.config.with(|c| c.tracker_in_use()) == Some(TrackerKind::Jira) {
        learn_epic_field(state).await;
    }
    tracker(state)?.mine().await
}

/// Look past your own queue: unassigned work, or anyone else's.
#[tauri::command]
pub async fn tickets_find(
    state: State<'_, AppState>,
    text: Option<String>,
    whose: String,
    include_done: Option<bool>,
    types: Option<Vec<String>>,
    order: Option<String>,
) -> Result<TicketPage> {
    let find = Find {
        text,
        whose,
        include_done: include_done.unwrap_or(false),
        types: types.unwrap_or_default(),
        order: order.unwrap_or_default(),
    };
    tracker(&state)?.find(find).await
}

/// These tickets, wherever they are, for the epic each belongs to (TASK-17,
/// REV-11). A task's ticket or a PR's is often not on your own list: done,
/// given to someone else, or never yours.
#[tauri::command]
pub async fn tickets_by_key(state: State<'_, AppState>, keys: Vec<String>) -> Result<Vec<Ticket>> {
    tracker(&state)?.by_key(&keys).await
}

/// Whatever this tracker calls an ordinary ticket.
///
/// Not "Task": that is one site's name for it. Hierarchy level means the same
/// everywhere — 0 is a standard ticket — so the plain type is found rather
/// than assumed, preferring one actually called Task when there is one.
pub(crate) async fn default_ticket_type(state: &AppState) -> Result<String> {
    let types = ticket_types_inner(state, false).await?;
    let plain: Vec<&TicketType> = types
        .iter()
        .filter(|t| t.hierarchy_level == 0 && !t.subtask)
        .collect();
    plain
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case("task"))
        .or_else(|| plain.first())
        .map(|t| t.name.clone())
        .ok_or_else(|| Error::Other("this tracker defines no ordinary type of ticket".into()))
}

/// What a project demands before it will accept a new ticket of this type.
#[tauri::command]
pub async fn ticket_required_fields(
    state: State<'_, AppState>,
    project_key: String,
    issue_type_id: String,
) -> Result<Vec<RequiredField>> {
    required_fields_for(&state, &project_key, &issue_type_id).await
}

/// `required_fields`, for a type id that may belong to another project.
///
/// The type list is one entry per name, but a team-managed project has its
/// own "Task" with an id of its own, so the id picked from that list is often
/// some other project's — and Jira answers it with a 404. When it does, the
/// same name is looked up in the project being filed into.
pub(crate) async fn required_fields_for(
    state: &AppState,
    project_key: &str,
    issue_type_id: &str,
) -> Result<Vec<RequiredField>> {
    let tracker = tracker(state)?;
    let first = tracker.required_fields(project_key, issue_type_id).await;
    if first.is_ok() {
        return first;
    }
    let name = ticket_types_inner(state, false)
        .await
        .ok()
        .and_then(|types| types.into_iter().find(|t| t.id == issue_type_id).map(|t| t.name));
    let Some(name) = name else {
        return first;
    };
    match tracker.type_in_project(project_key, &name, issue_type_id).await {
        Some(id) => tracker.required_fields(project_key, &id).await,
        None => first,
    }
}

/// File a ticket without starting work on it.
///
/// Filing under an epic and opening worktrees are different intentions:
/// adding a ticket to a plan is not saying you will start it now. Start work
/// on the ticket does that (`start_work_on_ticket`).
#[tauri::command]
pub async fn file_ticket(state: State<'_, AppState>, req: NewTicket) -> Result<Ticket> {
    if req.summary.trim().is_empty() {
        return Err(Error::Other("the ticket needs a summary".into()));
    }
    let tracker = tracker(&state)?;
    let key = tracker.file(&req).await?;
    tracker.ticket(&key).await
}

/// A task, and the status its ticket was moved to on the way.
#[derive(Debug, Serialize)]
pub struct Started {
    #[serde(flatten)]
    pub task: Task,
    /// The status the ticket was moved to, when it was moved.
    pub moved: Option<String>,
}

/// The one-click path: ticket -> worktree per repo -> agent primed with both
/// the ticket and the layout.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_work_on_ticket(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    project_ids: Vec<String>,
    agent_id: Option<String>,
    branch_suffix: Option<String>,
    base: Option<String>,
    epic: Option<bool>,
) -> Result<Started> {
    let epic = epic.unwrap_or(false);
    // A second task for the same ticket would try to check the same branch out
    // twice and fail deep inside git. Unless a suffix asks for a distinct
    // branch, point at what already exists.
    if branch_suffix.as_deref().unwrap_or_default().trim().is_empty() {
        if let Some(existing) = state
            .config
            .read()
            .tasks
            .iter()
            .find(|t| t.issue_key.as_deref() == Some(key.as_str()))
        {
            return Err(Error::Other(format!(
                "{key} already has a task on branch {}. Open that one, or pass a \
                 branch_suffix to work on the ticket a second time.",
                existing.branch
            )));
        }
    }

    let issue = tracker(&state)?.ticket(&key).await?;

    let new = NewTask {
        name: format!("{} {}", issue.key, issue.summary),
        project_ids,
        branch: None,
        branch_suffix,
        base,
        issue_key: Some(issue.key.clone()),
        issue_url: Some(issue.url.clone()),
        epic_key: issue.epic_key.clone(),
        epic,
        sub_task: None,
    };
    // Worktrees on the blocking pool, as in `create_task`.
    let task = super::blocking(app.clone(), move |state| new_task(state, new)).await?;
    keep_ticket(app.clone(), task.clone(), issue.clone()).await;

    // An epic's work is done in its sub-tasks: no agent starts in it, and
    // its ticket moves when the first of them starts (EPIC-2).
    if epic {
        return Ok(Started { task, moved: None });
    }
    if let Some(agent_id) = agent_id {
        let repos = task_repos(&state, &task);
        let prompt = ticket_prompt(&issue, &task, &repos, true);
        let task_id = task.id.clone();
        let handle = app.clone();
        super::blocking(app.clone(), move |state| {
            start_agent(&handle, state, task_id, agent_id, None, Some(prompt), Resume::No, None, None, false)
        })
        .await?;
    }

    let moved = super::ticket_flow::started(&state, &issue.key).await;
    Ok(Started { task, moved })
}
