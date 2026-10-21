//! The window's side of the workspace (§25): where it is, what is queued for
//! it here, and what it keeps, brought back (`workspace/mirror.rs`).

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::error::Result;
use crate::workspace::{Place, Snapshot, ThisMac};

/// Where the window's sync client finds the workspace, once its server has
/// started. None when it did not start, which was said then: the window
/// works without it, and what changes waits here.
#[tauri::command]
pub async fn workspace_place(this_mac: State<'_, ThisMac>) -> Result<Option<Place>> {
    Ok(this_mac.place().await.ok())
}

/// The changes queued for the workspace, oldest first, handed over to the
/// window's sync client, which keeps them until they are sent.
#[tauri::command]
pub async fn take_setup_writes(app: AppHandle) -> Result<Vec<Value>> {
    super::blocking(app, |state| {
        let mut ops = Vec::new();
        state.config.mirror(|c| {
            ops = std::mem::take(&mut c.shared.outbox);
            !ops.is_empty()
        })?;
        ops.extend(state.notes.take_writes()?);
        Ok(ops)
    })
    .await
}

/// How `apply_setup` went.
#[derive(Debug, Serialize)]
pub struct Applied {
    /// False when changes made here are still on their way: the window
    /// sends them, then the snapshot again.
    pub applied: bool,
    /// The repositories or settings changed, so the window reads them again.
    pub setup: bool,
}

/// What the workspace keeps, brought into this Mac's setup and notes.
#[tauri::command]
pub async fn apply_setup(app: AppHandle, snapshot: Snapshot) -> Result<Applied> {
    let applied = super::blocking(app.clone(), move |state| crate::workspace::apply(state, snapshot)).await?;
    let Some((setup, notes)) = applied else { return Ok(Applied { applied: false, setup: false }) };
    if notes {
        crate::notes::changed(&app);
    }
    Ok(Applied { applied: true, setup })
}
