//! What the Spec tab is shown of a task's spec (SPEC-8), and the drafts it
//! keeps while the user edits (SPEC-6).

use std::path::Path;

use serde::Serialize;

use crate::config::Task;
use crate::error::Result;
use crate::git;
use crate::spec::{Kind, Part, Requirement, Step};

use super::panes::agent_file_dir;
use super::spec::{slot_of, spec_of, Check, Proposal, LEGACY_FILE};
use super::AppState;

/// A task's spec as the Spec tab shows it (SPEC-8).
#[derive(Debug, Serialize)]
pub struct TaskSpec {
    pub kind: Kind,
    /// Requirements, design, then each repository's tasks.
    pub parts: Vec<PartView>,
    pub requirements: Vec<Requirement>,
    pub repos: Vec<RepoSpec>,
    /// A `SPEC.md` from before, to start the requirements from (SPEC-17).
    pub legacy: Option<String>,
    /// The repositories have specs of their own from before, to merge (SPEC-25).
    pub split: bool,
}

#[derive(Debug, Serialize)]
pub struct PartView {
    pub part: Part,
    /// Whose tasks; None for the task's requirements and design.
    pub checkout_id: Option<String>,
    pub file: String,
    pub approved: Option<String>,
    pub draft: Option<String>,
    /// Approved, but an earlier file was approved since (SPEC-7).
    pub stale: bool,
    /// Copies changed outside the app (SPEC-22).
    pub outside: Vec<Outside>,
    /// A change an agent proposed, which `draft` holds (SPEC-24).
    pub proposal: Option<Proposal>,
}

#[derive(Debug, Serialize)]
pub struct Outside {
    pub checkout_id: String,
    pub folder: String,
    /// What the copy holds; empty when it is gone.
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct RepoSpec {
    pub checkout_id: String,
    pub repo: String,
    pub folder: String,
    /// Where its copy is kept, and whether that is in the repository (SPEC-3).
    pub home: String,
    pub in_repo: bool,
    pub steps: Vec<Step>,
    pub check: Option<CheckView>,
}

#[derive(Debug, Serialize)]
pub struct CheckView {
    #[serde(flatten)]
    pub check: Check,
    /// The branch has moved on since.
    pub stale: bool,
}

/// Runs git, for whether a check is still current.
pub(super) fn view(state: &AppState, task: &Task) -> TaskSpec {
    let spec = spec_of(state, task);
    let kind = spec.kind();
    let notes = spec.notes();
    let parts = spec
        .slots()
        .into_iter()
        .map(|slot| PartView {
            part: slot.part,
            checkout_id: slot.repo.as_ref().and_then(|f| spec.home(f)).map(|h| h.checkout.id.clone()),
            file: slot.part.file(kind).to_string(),
            approved: spec.approved(&slot),
            draft: spec.draft(&slot),
            stale: notes.stale.contains(&slot),
            outside: spec
                .outside(&slot, kind)
                .into_iter()
                .map(|(h, text)| Outside { checkout_id: h.checkout.id.clone(), folder: h.folder.clone(), text })
                .collect(),
            proposal: notes.proposals.iter().find(|p| p.slot == slot).cloned(),
        })
        .collect();
    let repos = spec
        .homes
        .iter()
        .map(|h| {
            let head = git::head_commit(Path::new(&h.checkout.path)).ok();
            RepoSpec {
                checkout_id: h.checkout.id.clone(),
                repo: h.project.name.clone(),
                folder: h.folder.clone(),
                home: h.rel.clone().map(|r| format!("{}/{r}", h.folder)).unwrap_or_else(|| h.shown()),
                in_repo: h.rel.is_some(),
                steps: spec.steps(h),
                check: spec.check(h).map(|check| CheckView { stale: head.as_deref() != Some(check.sha.as_str()), check }),
            }
        })
        .collect();
    TaskSpec { kind, parts, requirements: spec.requirements(), repos, legacy: legacy(state, task), split: spec.split() }
}

fn legacy(state: &AppState, task: &Task) -> Option<String> {
    let text = std::fs::read_to_string(agent_file_dir(state, task)?.join(LEGACY_FILE)).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

/// The task's spec.
#[tauri::command]
pub async fn read_spec(app: tauri::AppHandle, task_id: String) -> Result<TaskSpec> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        Ok(view(state, &task))
    })
    .await
}

/// Keep what the editor holds for one file, or with None let it go: kept in
/// the task folder, so quitting loses nothing, and seen by no agent (SPEC-6).
#[tauri::command]
pub async fn save_spec_draft(
    app: tauri::AppHandle,
    task_id: String,
    part: Part,
    checkout_id: Option<String>,
    text: Option<String>,
) -> Result<()> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let slot = slot_of(&spec, part, checkout_id.as_deref())?;
        spec.save_draft(&slot, text.as_deref())
    })
    .await
}

