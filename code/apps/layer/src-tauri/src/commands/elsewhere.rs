//! The workspace's repositories with no clone on this Mac (REPO-9): listed
//! beside the rest, and made a repository here by locating a clone or
//! cloning one. The clone takes the workspace's record, so the name, group
//! and settings are the ones everyone has, and nothing is sent.

use std::path::Path;

use tauri::{AppHandle, State};

use crate::config::shared::Elsewhere;
use crate::config::Project;
use crate::error::{Error, Result};
use crate::git;

use super::projects::{describe_project, ensure_store};
use super::AppState;

#[tauri::command]
pub fn list_elsewhere(state: State<AppState>) -> Vec<Elsewhere> {
    state.config.read().shared.elsewhere
}

/// A clone already on this Mac, wherever it is.
#[tauri::command]
pub async fn locate_elsewhere(app: AppHandle, repository_id: String, path: String) -> Result<Project> {
    let placed = super::blocking(app.clone(), move |state| place(state, &repository_id, &path)).await?;
    copy_in_background(app, placed.id.clone());
    Ok(placed)
}

/// A new clone, in a folder of the repository's name inside `folder`.
#[tauri::command]
pub async fn clone_elsewhere(app: AppHandle, repository_id: String, folder: String) -> Result<Project> {
    let placed = super::blocking(app.clone(), move |state| {
        let e = find(state, &repository_id)?;
        let into = Path::new(&folder).join(&e.name);
        git::clone_by_key(&e.remote_key, &into)?;
        place(state, &repository_id, &into.to_string_lossy())
    })
    .await?;
    copy_in_background(app, placed.id.clone());
    Ok(placed)
}

fn find(state: &AppState, id: &str) -> Result<Elsewhere> {
    state
        .config
        .read()
        .shared
        .elsewhere
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| Error::NotFound("that repository: it is on this Mac now, or gone from the workspace".into()))
}

fn place(state: &AppState, id: &str, path: &str) -> Result<Project> {
    let e = find(state, id)?;
    let project = describe_project(path, None)?;
    if project.remote.as_deref() != Some(e.remote_key.as_str()) {
        return Err(Error::Other(format!(
            "{} is a different repository: it fetches from {}, and {} from {}",
            project.path,
            project.remote.as_deref().unwrap_or("nowhere"),
            e.name,
            e.remote_key,
        )));
    }
    let project_id = project.id.clone();
    state.config.update(|c| {
        if let Some(other) = c.projects.iter().find(|p| p.path == project.path) {
            return Err(Error::Other(format!("{} is already registered, as {}", project.path, other.name)));
        }
        c.projects.push(project);
        Ok(())
    })??;
    // Its record was taken as the change was written (`shared::stamp`).
    state.config.project(&project_id)
}

/// The app's own copy (REPO-4), as for any repository added.
fn copy_in_background(app: AppHandle, id: String) {
    std::thread::spawn(move || {
        use tauri::Manager;
        if let Err(e) = ensure_store(&app.state::<AppState>(), &id) {
            eprintln!("villain-layer: no private copy of {id} yet: {e}");
        }
    });
}
