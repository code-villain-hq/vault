//! Working to a task's spec (§19): what agents are told of it, the steps
//! they tick, what the pull requests say of it, and telling them it changed.

use std::path::Path;

use tauri::State;

use crate::config::Task;
use crate::error::{Error, Result};
use crate::git;
use crate::spec::{self, Kind, Part, Step};

use super::spec::{and_list, spec_of, Slot};
use super::AppState;

/// Tick (or untick) step `number` of a repository's `tasks.md`, for an
/// agent's `spec_task` (SPEC-13). Committed later, with the next approval
/// or before the pull requests are opened.
pub(crate) fn tick_step(state: &AppState, task_id: &str, repo: &str, number: u32, done: bool) -> Result<Step> {
    let task = state.config.task(task_id)?;
    let spec = spec_of(state, &task);
    let home = spec
        .homes
        .iter()
        .find(|h| h.folder == repo || h.project.name == repo)
        .or_else(|| (spec.homes.len() == 1).then(|| &spec.homes[0]))
        .ok_or_else(|| Error::NotFound(format!("a spec for `{repo}` in this task")))?;
    tick_file(&home.dir.join(Part::Tasks.file(Kind::Feature)), repo, number, done)
}

/// Tick step `number` in the `tasks.md` at `path`. One tick at a time, from
/// reading the file to writing it back: two agents ticking at once each
/// wrote back what they had read, and one tick was lost. Written whole, as
/// the file is in the user's repository and approved: a crash part-way
/// through `fs::write` left it cut short.
fn tick_file(path: &Path, repo: &str, number: u32, done: bool) -> Result<Step> {
    static TICKING: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let _one = TICKING.lock();
    let text = std::fs::read_to_string(path)
        .map_err(|_| Error::NotFound(format!("an approved tasks.md for `{repo}`")))?;
    let ticked = spec::tick(&text, number, done).ok_or_else(|| Error::NotFound(format!("step {number}")))?;
    crate::agents::replace_file(path, ticked.as_bytes(), None)?;
    spec::steps(&ticked)
        .into_iter()
        .find(|s| s.number == number)
        .ok_or_else(|| Error::NotFound(format!("step {number}")))
}

/// Commit what agents ticked since the last approval, before a push takes
/// the branch to review (SPEC-13): `tasks.md` alone, and only while it
/// differs from what was approved in its ticks. A spec changed outside the
/// app is the user's to approve or put back (SPEC-22), not to be committed
/// as ticks.
pub(crate) fn commit_ticks(state: &AppState, task: &Task, checkout_id: &str) -> Result<()> {
    let spec = spec_of(state, task);
    let Some(home) = spec.homes.iter().find(|h| h.checkout.id == checkout_id) else { return Ok(()) };
    let Some(rel) = &home.rel else { return Ok(()) };
    let file = Part::Tasks.file(Kind::Feature);
    if !home.dir.join(file).is_file() || spec.has_outside(&Slot::tasks(&home.folder)) {
        return Ok(());
    }
    let message = format!("Tick the done steps of the spec for {}.", task.issue_key.as_deref().unwrap_or(&task.branch));
    git::commit_only(Path::new(&home.checkout.path), &[format!("{rel}/{file}").as_str()], &message)?;
    Ok(())
}

/// The line an opening prompt gains when the task has a spec (SPEC-11):
/// every repository's spec folder by its full path.
pub(crate) fn prompt_line(state: &AppState, task: &Task) -> Option<String> {
    let spec = spec_of(state, task);
    spec.approved(&Slot::shared(Part::Requirements))?;
    let found: Vec<String> = spec.homes.iter().map(|h| format!("`{}` for {}", h.shown(), h.folder)).collect();
    (!found.is_empty()).then(|| {
        format!(
            "The user's spec for this work is in {}: the same requirements and design in each, and \
             each repository's own tasks. Its requirements are what done means; read it before you start.",
            and_list(&found.iter().map(String::as_str).collect::<Vec<_>>())
        )
    })
}

/// The approved requirements, for the context file (SPEC-10): the text, and
/// each repository's folder and where its copy is. None without any.
pub(crate) fn approved_requirements(state: &AppState, task: &Task) -> Option<(String, Vec<(String, String)>)> {
    let spec = spec_of(state, task);
    let text = spec.approved(&Slot::shared(Part::Requirements))?;
    Some((text, spec.homes.iter().map(|h| (h.folder.clone(), h.shown())).collect()))
}

/// The spec's part of a drafted pull request description (SPEC-16): for
/// each repository with approved requirements, where its spec is in it
/// and the requirements it meets, with the latest check's answer. Runs git.
pub(crate) fn pr_section(state: &AppState, task: &Task) -> String {
    let spec = spec_of(state, task);
    let reqs = spec.requirements();
    let mut md = String::new();
    for h in &spec.homes {
        let mine: Vec<_> = reqs.iter().filter(|r| r.names(&h.folder)).collect();
        if mine.is_empty() {
            continue;
        }
        let place = match &h.rel {
            Some(rel) => format!("`{rel}/` in the repository"),
            None => "kept outside the repository, so not linked".into(),
        };
        md.push_str(&format!("\n## Spec of `{}`: {place}\n\n", h.project.name));
        let check = spec.check(h);
        let head = git::head_commit(Path::new(&h.checkout.path)).ok();
        let stale = check.as_ref().is_some_and(|c| head.as_deref() != Some(c.sha.as_str()));
        for r in mine {
            let answer = check
                .as_ref()
                .and_then(|c| c.results.iter().find(|v| v.id == r.id))
                .map(|v| format!("{}{}", v.verdict, if v.evidence.is_empty() { String::new() } else { format!(" ({})", v.evidence) }))
                .unwrap_or_else(|| "not checked".into());
            md.push_str(&format!("- {}: {} — {answer}\n", r.id, r.text));
        }
        match (&check, stale) {
            (None, _) => md.push_str("\nNo check against the spec was run.\n"),
            (Some(_), true) => md.push_str("\nThe check was run before the latest commits.\n"),
            _ => {}
        }
    }
    md
}

/// What Start agent asks of an agent on the spec (SPEC-12): every
/// repository's tasks, or one's, in order, each ticked as soon as it is done.
#[tauri::command]
pub async fn spec_work_prompt(app: tauri::AppHandle, task_id: String, checkout_id: Option<String>) -> Result<String> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let lists: Vec<String> = spec
            .homes
            .iter()
            .filter(|h| checkout_id.as_ref().is_none_or(|id| *id == h.checkout.id))
            .filter(|h| !spec.steps(h).is_empty())
            .map(|h| format!("- {} (repo `{}`)", h.dir.join("tasks.md").display(), h.folder))
            .collect();
        if lists.is_empty() {
            return Ok(String::new());
        }
        let alone = checkout_id.is_some() && spec.homes.len() > 1;
        Ok(format!(
            "Work through the steps in\n{}\nin order. A step marked `after api 2` waits for step 2 of \
             `api`: do that one first{}. As soon as a step is done, before you start the next, call \
             `spec_task` on the `villain-layer` MCP server with task id `{}`, the repo, and the step's \
             number, so it is ticked off: the user follows the work by these ticks. Do not save them up. \
             If a step turns out wrong or the spec does not hold, propose the change with \
             `spec_propose` and wait, rather than editing the spec or working around it.",
            lists.join("\n"),
            if alone { ", and if it is another repository's that is not ticked yet, stop and say which step you are waiting for" } else { "" },
            task.id
        ))
    })
    .await
}

/// Tell the task's running agents that the spec changed (SPEC-14): one line
/// each, naming the files. Returns how many were told.
#[tauri::command]
pub async fn tell_spec_change(app: tauri::AppHandle, task_id: String, parts: Vec<Part>) -> Result<usize> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let kind = spec.kind();
        let mut files: Vec<String> = Vec::new();
        for part in &parts {
            match spec.homes.first() {
                Some(first) if *part != Part::Tasks || spec.homes.len() == 1 => {
                    files.push(format!("{} (the same in every repository)", first.dir.join(part.file(kind)).display()));
                }
                _ => files.extend(spec.homes.iter().map(|h| h.dir.join(part.file(kind)).display().to_string())),
            }
        }
        let line = format!(
            "The user changed the spec for this work: {}. Read it again, and check what you are doing still meets it.",
            and_list(&files.iter().map(String::as_str).collect::<Vec<_>>())
        );
        let mut told = 0;
        for pane in state.ptys.list(Some(&task.id)) {
            if pane.running && pane.kind == crate::pty::PaneKind::Agent {
                super::hand_over(state, &task, &pane.id, "SPEC_CHANGED.md", &line)?;
                told += 1;
            }
        }
        Ok(told)
    })
    .await
}

/// Where a repository keeps specs (SPEC-1, SPEC-3): committed in it, under
/// `folder`, or kept by the app.
#[tauri::command]
pub fn set_project_specs(state: State<AppState>, project_id: String, in_app: bool, folder: String) -> Result<()> {
    let folder = spec::valid_folder(&folder).map_err(Error::Other)?;
    state.config.update(|c| {
        if let Some(p) = c.projects.iter_mut().find(|p| p.id == project_id) {
            p.specs_in_app = in_app;
            p.spec_folder = folder;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_ticking_steps_at_once_lose_none_of_them() {
        let root = std::env::temp_dir().join(format!("vl-spec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("tasks.md");
        let steps: String = (1..=16).map(|n| format!("- [ ] {n}. Step {n}\n")).collect();
        std::fs::write(&path, steps).unwrap();

        std::thread::scope(|s| {
            for n in 1..=16 {
                let path = &path;
                s.spawn(move || tick_file(path, "api", n, true).unwrap());
            }
        });

        let ticked = spec::steps(&std::fs::read_to_string(&path).unwrap());
        assert!(ticked.iter().all(|s| s.done), "{ticked:?}");
        std::fs::remove_dir_all(&root).ok();
    }
}
