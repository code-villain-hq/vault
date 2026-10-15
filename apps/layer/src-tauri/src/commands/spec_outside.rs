//! A spec changed outside the Spec tab (§19): a copy in a repository that
//! is not what was approved (SPEC-22), put back; and a change an agent
//! proposed with `spec_propose` (SPEC-24), turned down or (in `spec.rs`)
//! approved. An agent that edited a spec file used to be believed: the tab
//! showed its text as approved, nothing after it went out of date, and it
//! was committed with the code.

use std::path::Path;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::config::Task;
use crate::error::{Error, Result};
use crate::git;
use crate::mcp::{arg, required, str_prop, tool};
use crate::spec::{self, Kind, Part};

use super::spec::{slot_of, spec_of, write, Proposal, Slot};
use super::spec_view::{view, TaskSpec};
use super::task_context::write_task_context;
use super::AppState;

/// Write what was approved back over a copy changed outside the app
/// (SPEC-22), in one repository, keeping the steps it has ticked, and
/// commit it there.
#[tauri::command]
pub async fn put_back_spec(app: tauri::AppHandle, task_id: String, part: Part, checkout_id: String) -> Result<TaskSpec> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let kind = spec.kind();
        let slot = slot_of(&spec, part, Some(&checkout_id))?;
        let home = spec.homes.iter().find(|h| h.checkout.id == checkout_id).ok_or_else(|| Error::NotFound(format!("checkout {checkout_id}")))?;
        let record = spec.record(&slot, kind).ok_or_else(|| Error::Other("nothing was approved to put back".into()))?;
        let text = match (part, home.copy(part, kind)) {
            (Part::Tasks, Some(copy)) => spec::keep_ticks(&record, &copy),
            _ => record,
        };
        let path = home.dir.join(part.file(kind));
        write(&path, &format!("{}\n", text.trim()))?;
        if let Some(rel) = &home.rel {
            let message = format!("Put back the approved {} for {}.", part.word(kind), task.issue_key.as_deref().unwrap_or(&task.branch));
            git::commit_only(Path::new(&home.checkout.path), &[format!("{rel}/{}", part.file(kind)).as_str()], &message)?;
        }
        write_task_context(state, &task)?;
        Ok(view(state, &task))
    })
    .await
}

/// Turn down a change an agent proposed (SPEC-24): its draft goes, and the
/// agent is told.
#[tauri::command]
pub async fn turn_down_spec_proposal(app: tauri::AppHandle, task_id: String, part: Part, checkout_id: Option<String>) -> Result<TaskSpec> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let slot = slot_of(&spec, part, checkout_id.as_deref())?;
        spec.save_draft(&slot, None)?;
        if let Some(p) = spec.settle(&slot)? {
            tell_proposer(state, &task, &p, "turned down: work to the spec as it was approved");
        }
        Ok(view(state, &task))
    })
    .await
}

/// An agent's `spec_propose` (SPEC-24): the whole new text of one file, kept
/// as its draft for the user to approve or turn down. Refused while the
/// user has a draft of that file of their own.
pub(crate) fn propose(state: &AppState, task_id: &str, pane_id: Option<&str>, part: Part, repo: Option<&str>, text: &str, why: &str) -> Result<String> {
    let task = state.config.task(task_id)?;
    let spec = spec_of(state, &task);
    let slot = match part {
        Part::Tasks => {
            let home = match repo {
                Some(r) => spec.homes.iter().find(|h| h.folder == r || h.project.name == r),
                None => (spec.homes.len() == 1).then(|| &spec.homes[0]),
            }
            .ok_or_else(|| Error::NotFound(format!("a repository `{}` in this task", repo.unwrap_or(""))))?;
            Slot::tasks(&home.folder)
        }
        _ => Slot::shared(part),
    };
    if text.trim().is_empty() || why.trim().is_empty() {
        return Err(Error::Other("give the file's whole new text, and why".into()));
    }
    if spec.draft(&slot).is_some() {
        return Err(Error::Other(format!(
            "the user has a draft of the {} open in the Spec tab; ask them about the change instead",
            part.word(spec.kind())
        )));
    }
    let by = pane_id
        .and_then(|p| state.ptys.info(p).ok())
        .and_then(|p| p.agent_id)
        .and_then(|id| crate::agents::find(&id))
        .map(|a| a.name.to_string())
        .unwrap_or_else(|| "An agent".into());
    spec.save_draft(&slot, Some(text))?;
    spec.wait_on(Proposal { slot, why: why.trim().to_string(), by, pane_id: pane_id.map(str::to_string), at: chrono::Utc::now().timestamp_millis() })?;
    Ok(format!(
        "Proposed. The user sees it on the Spec tab and approves it or turns it down; you will be told which. \
         Until then work to the {} as it was approved, or wait.",
        part.word(spec.kind())
    ))
}

/// Tell the agent that proposed a change what became of it. Not being able
/// to (it has stopped) is no reason to fail what the user did.
pub(super) fn tell_proposer(state: &AppState, task: &Task, proposal: &Proposal, outcome: &str) {
    let Some(pane) = &proposal.pane_id else { return };
    if !state.ptys.info(pane).is_ok_and(|p| p.running) {
        return;
    }
    let what = match &proposal.slot.repo {
        Some(folder) => format!("the tasks of {folder}"),
        None => format!("the {}", proposal.slot.part.word(Kind::Feature)),
    };
    let line = format!("Your proposed change to {what} in the spec was {outcome}.");
    let _ = super::hand_over(state, task, pane, "SPEC_ANSWER.md", &line);
}

/// The MCP tools this file answers.
pub fn spec_tools() -> Vec<Value> {
    vec![tool(
        "spec_propose",
        "Propose a change to the user's spec for this task when a requirement, the design \
         or a step turns out wrong: never edit the spec files yourself. Give the file's \
         whole new text and why. The user approves or turns it down on the Spec tab, and \
         you are told which; until then work to the spec as it was approved, or wait.",
        json!({
            "task_id": str_prop("Your task's id, from the context file or list_tasks"),
            "file": str_prop("requirements, design or tasks"),
            "repo": str_prop("For tasks: the repository's folder in the task; may be left out when there is one"),
            "text": str_prop("The file's whole new text"),
            "why": str_prop("What made the change needed, for the user")
        }),
        vec!["file", "text", "why"],
    )]
}

pub fn spec_owns(name: &str) -> bool {
    name == "spec_propose"
}

pub async fn spec_call(app: &AppHandle, args: Value, caller: Option<&str>) -> Result<Value> {
    let state = app.state::<AppState>();
    let task_id = match arg(&args, "task_id") {
        Some(id) => id.to_string(),
        None => caller
            .and_then(|pane| state.ptys.info(pane).ok())
            .map(|p| p.task_id)
            .ok_or_else(|| Error::Other("give task_id".into()))?,
    };
    let part = match required(&args, "file")?.trim().to_ascii_lowercase().trim_end_matches(".md") {
        "requirements" | "bugfix" => Part::Requirements,
        "design" => Part::Design,
        "tasks" => Part::Tasks,
        other => return Err(Error::Other(format!("`{other}` is not a file of the spec: requirements, design or tasks"))),
    };
    let repo = arg(&args, "repo").map(str::to_string);
    let text = required(&args, "text")?.to_string();
    let why = required(&args, "why")?.to_string();
    let pane = caller.map(str::to_string);
    let id = task_id.clone();
    let said = super::blocking(app.clone(), move |state| propose(state, &id, pane.as_deref(), part, repo.as_deref(), &text, &why)).await?;
    {
        use tauri::Emitter;
        let _ = app.emit("spec:changed", &task_id);
    }
    Ok(json!({ "proposed": said }))
}
