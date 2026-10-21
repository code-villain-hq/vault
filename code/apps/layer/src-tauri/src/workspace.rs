//! The workspace on this Mac (§25, PROF-11): the app runs Layer's server on
//! a Postgres of the workspace's own, both on loopback, and hands the window
//! the address and a session. They start in the background at launch and
//! stop when the app quits, after its agents.
//!
//! The rest of the app never waits for them. It reads and writes
//! `config.json` and `notes.json` as before; what changes in the shared
//! setup is queued there (`config/shared.rs`), and the window's sync client
//! takes it to the workspace (`src/lib/workspace-sync.ts`) and brings back
//! what the workspace keeps (`mirror.rs`).

mod mirror;
mod moving;
pub mod server;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use code_villain_desktop::{LocalPostgres, LocalServer};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Mutex};

use crate::commands::AppState;
use crate::config::shared::{Sent, THIS_MAC};

pub use mirror::{apply, Snapshot};

/// The ports the server and Postgres take when they are free. Each open
/// workspace runs its own, so the second takes any free port.
const SERVER_PORT: u16 = 7685;
const POSTGRES_PORT: u16 = 7686;

/// Where the window finds the workspace.
#[derive(Clone, Debug, Serialize)]
pub struct Place {
    pub origin: String,
    pub token: String,
    pub user_id: String,
    pub workspace_id: String,
}

struct Running {
    postgres: LocalPostgres,
    server: LocalServer,
    pool: PgPool,
}

/// Held by the app as its own state, beside `AppState`.
pub struct ThisMac {
    /// None while starting; then where it is, or why it is not.
    place: watch::Sender<Option<std::result::Result<Place, String>>>,
    running: Arc<Mutex<Option<Running>>>,
}

impl ThisMac {
    pub fn new() -> Self {
        Self { place: watch::Sender::new(None), running: Arc::new(Mutex::new(None)) }
    }

    /// At launch: start it in the background. The lock is taken here, before
    /// quitting can ask for it, so a quit during the start waits for it.
    pub fn resume(&self, app: AppHandle) {
        let Ok(mut slot) = Arc::clone(&self.running).try_lock_owned() else { return };
        let place = self.place.clone();
        tauri::async_runtime::spawn(async move {
            let result = match start(&app).await {
                Ok((running, at)) => {
                    *slot = running;
                    Ok(at)
                }
                // The window says so, and offers Sign in again (PROF-16).
                Err(e) if e == server::SIGNED_OUT => Err(e),
                Err(e) => {
                    crate::commands::notify(
                        &app,
                        "error",
                        format!(
                            "This workspace did not start ({e}). Layer works from its last copy of the setup, and keeps what changes here for it."
                        ),
                    );
                    Err(e)
                }
            };
            drop(slot);
            place.send_replace(Some(result));
        });
    }

    /// Where the workspace is, once it has started.
    pub async fn place(&self) -> std::result::Result<Place, String> {
        let mut seen = self.place.subscribe();
        let ready = seen.wait_for(Option::is_some).await.map_err(|e| e.to_string())?;
        ready.clone().unwrap_or_else(|| Err("not started".into()))
    }

    /// Signed in again (PROF-16): the window's sync client reads this.
    pub fn signed_in(&self, place: Place) {
        self.place.send_replace(Some(Ok(place)));
    }

    /// On quit, after the agents: the server first, so nothing writes while
    /// Postgres stops.
    pub async fn stop(&self) {
        let Some(Running { postgres, server, pool }) = self.running.lock().await.take() else { return };
        server.stop().await;
        pool.close().await;
        if let Err(e) = postgres.stop().await {
            eprintln!("the workspace's Postgres did not stop cleanly: {e}");
        }
    }
}

async fn start(app: &AppHandle) -> std::result::Result<(Option<Running>, Place), String> {
    let state = app.state::<AppState>();
    // A workspace on a server runs nothing here (PROF-15).
    if let Some(at) = state.config.with(|c| c.shared.workspace.clone()).filter(|at| at.place != THIS_MAC) {
        return server::place(at).await.map(|place| (None, place));
    }
    let folder = state.config.folder().join("postgres");
    let postgres = LocalPostgres::start(&folder, POSTGRES_PORT).await.map_err(|e| e.to_string())?;
    match serve(app, &postgres).await {
        Ok((server, pool, place)) => Ok((Some(Running { postgres, server, pool }), place)),
        Err(e) => {
            let _ = postgres.stop().await;
            Err(e)
        }
    }
}

async fn serve(app: &AppHandle, postgres: &LocalPostgres) -> std::result::Result<(LocalServer, PgPool, Place), String> {
    let url = postgres.database("layer").await.map_err(|e| e.to_string())?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;
    layer_sync::MIGRATOR.run(&pool).await.map_err(|e| e.to_string())?;

    let mut state = layer_server::AppState::new(pool.clone(), false).on_this_mac();
    if cfg!(debug_assertions) {
        // `bun run tauri dev` shows the window from Vite.
        state = state.allowing_origin("http://localhost:1420");
    }
    let server = LocalServer::spawn(SERVER_PORT, |listener, shutdown| {
        layer_server::serve(state, layer_server::Web::None, listener, shutdown)
    })
    .await
    .map_err(|e| e.to_string())?;

    let ready = async {
        let (user, workspace) = moving::ensure(app, &pool).await?;
        let token = layer_server::local::session(&pool, user).await.map_err(|e| e.to_string())?;
        Ok::<_, String>(Place {
            origin: server.origin().to_string(),
            token,
            user_id: user.to_string(),
            workspace_id: workspace.to_string(),
        })
    };
    match ready.await {
        Ok(place) => Ok((server, pool, place)),
        Err(e) => {
            server.stop().await;
            pool.close().await;
            Err(e)
        }
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// From launch on, changes to the shared setup are announced through `app`.
pub fn attach(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// A change to the shared setup was queued: the window's sync client takes
/// it (`setup:write`). What would be refused is said, and stays here. Nothing
/// in tests, which have no app.
pub fn wrote(sent: Sent) {
    let Some(app) = APP.get() else { return };
    if sent.queued > 0 {
        let _ = app.emit("setup:write", ());
    }
    for why in sent.refused {
        crate::commands::notify(app, "error", format!("{why} It stays on this Mac, and is not in the workspace."));
    }
}
