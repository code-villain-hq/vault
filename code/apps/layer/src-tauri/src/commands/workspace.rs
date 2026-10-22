//! The window's side of the workspace (§25): where it is, what is queued for
//! it here, and what it keeps, brought back (`workspace/mirror.rs`).

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Error, Result};
use crate::workspace::server::{self, SignedIn, Ways};
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

/// How the server at `address` signs people in (PROF-15).
#[tauri::command]
pub async fn server_ways(address: String) -> Result<Ways> {
    server::ways(&server::read_address(&address)?).await
}

/// Sign in to the server at `address` with its company's account, in the
/// browser (PROF-17).
#[tauri::command]
pub async fn server_sign_in_browser(app: AppHandle, address: String) -> Result<SignedIn> {
    server::sign_in_browser(&app, &server::read_address(&address)?).await
}

#[tauri::command]
pub fn server_sign_in_cancel() {
    server::cancel();
}

/// Open workspace `id` on the server signed in to, in its own window.
#[tauri::command]
pub async fn server_open(app: AppHandle, id: String) -> Result<()> {
    super::off_runtime(move || {
        server::open(&id)?;
        super::profiles::changed(&app);
        Ok(())
    })
    .await?
}

/// Signed in again (PROF-16): the new session becomes this workspace's,
/// and the window's sync client goes on with it.
#[tauri::command]
pub async fn server_resume(app: AppHandle, this_mac: State<'_, ThisMac>) -> Result<Place> {
    let place = super::blocking(app, |state| {
        let at = state.config.with(|c| c.shared.workspace.clone());
        server::resume(&at.ok_or_else(|| Error::Other("This workspace is not on a server.".into()))?)
    })
    .await?;
    this_mac.signed_in(place.clone());
    Ok(place)
}

/// The workspace's members page on its server, opened in the browser
/// (PROF-18).
#[tauri::command]
pub async fn open_members(app: AppHandle, this_mac: State<'_, ThisMac>) -> Result<()> {
    let place = this_mac.place().await.map_err(|_| Error::Other("Sign in to the server first.".into()))?;
    let page = server::members_page(&place).await?;
    app.opener().open_url(page, None::<&str>).map_err(|e| Error::Other(e.to_string()))
}
