//! Workspaces on this Mac: the app runs Almanac's server on its own Postgres,
//! both on loopback, and hands the web app the address and a session. It
//! starts on launch once someone has chosen "Start on this Mac", and stops
//! when the app quits. Reminders that come due there become notifications.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use almanac_server::claude::Claude;
use almanac_server::{AppState, local};
use code_villain_desktop::{LocalPostgres, LocalServer};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Mutex;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

/// The ports the app's server and Postgres take when they're free.
const SERVER_PORT: u16 = 7785;
const POSTGRES_PORT: u16 = 7786;

/// Where the web app finds the workspaces on this Mac.
#[derive(Clone, Serialize)]
pub struct Place {
    pub origin: String,
    pub token: String,
}

struct Running {
    postgres: LocalPostgres,
    server: LocalServer,
    pool: PgPool,
    /// The person and this launch's session, once they've started here.
    session: Option<(Uuid, String)>,
}

/// Held by the app. Commands wait on the lock while Almanac starts.
pub struct ThisMac {
    dir: PathBuf,
    app: AppHandle,
    running: Arc<Mutex<Result<Option<Running>, String>>>,
}

impl ThisMac {
    pub fn new(dir: PathBuf, app: AppHandle) -> Self {
        Self {
            dir,
            app,
            running: Arc::new(Mutex::new(Ok(None))),
        }
    }

    /// On launch: start Almanac in the background if this Mac has workspaces.
    /// The lock is taken here, before the window can ask, so it waits.
    pub fn resume(&self) {
        let mut running = Arc::clone(&self.running)
            .try_lock_owned()
            .expect("nothing else has it at launch");
        let dir = self.dir.clone();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            if dir.join("postgres").join("data").exists() {
                *running = start(&dir, &app).await.map(Some);
            }
        });
    }

    /// The workspaces on this Mac, or `None` before "Start on this Mac".
    pub async fn place(&self) -> Result<Option<Place>, String> {
        let running = self.running.lock().await;
        let Some(running) = running.as_ref().map_err(Clone::clone)? else {
            return Ok(None);
        };
        Ok(running.session.as_ref().map(|(_, token)| Place {
            origin: running.server.origin().to_string(),
            token: token.clone(),
        }))
    }

    /// "Start on this Mac": Postgres and the server the first time, the
    /// person (named once) and their first workspace.
    pub async fn start_here(&self, name: &str, workspace: &str) -> Result<Place, String> {
        let mut guard = self.running.lock().await;
        if !matches!(*guard, Ok(Some(_))) {
            *guard = start(&self.dir, &self.app).await.map(Some);
        }
        let running = guard
            .as_mut()
            .map_err(|e| e.clone())?
            .as_mut()
            .expect("started above");
        match &running.session {
            Some((person, _)) => {
                local::new_workspace(&running.pool, *person, workspace)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            None => {
                let person = local::start(&running.pool, name, workspace)
                    .await
                    .map_err(|e| e.to_string())?;
                running.session = Some((person, new_session(&running.pool, person).await?));
            }
        }
        let (_, token) = running.session.as_ref().expect("set above");
        Ok(Place {
            origin: running.server.origin().to_string(),
            token: token.clone(),
        })
    }

    /// On quit: the server first, so nothing writes while Postgres stops.
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        if let Ok(Some(Running {
            postgres,
            server,
            pool,
            ..
        })) = std::mem::replace(&mut *running, Ok(None))
        {
            server.stop().await;
            pool.close().await;
            if let Err(error) = postgres.stop().await {
                tracing::warn!("Postgres didn't stop cleanly: {error}");
            }
        }
    }
}

async fn start(dir: &Path, app: &AppHandle) -> Result<Running, String> {
    let postgres = LocalPostgres::start(&dir.join("postgres"), POSTGRES_PORT)
        .await
        .map_err(|e| e.to_string())?;
    let url = postgres
        .database("almanac")
        .await
        .map_err(|e| e.to_string())?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;
    almanac_sync::MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| e.to_string())?;

    // The key is set in Settings → Claude; ANTHROPIC_API_KEY stands in.
    let claude = Claude::new(std::env::var("ANTHROPIC_API_KEY").ok());
    let mut state = AppState::new(pool.clone(), false, claude).on_this_mac();
    notify(app.clone(), &state);
    if cfg!(debug_assertions) {
        // `bun run dev:desktop` shows the web app from Vite.
        state = state.allowing_origin("http://localhost:5177");
    }
    let server = LocalServer::spawn(SERVER_PORT, |listener, shutdown| {
        almanac_server::serve(state, None, listener, shutdown)
    })
    .await
    .map_err(|e| e.to_string())?;
    tracing::info!("Almanac on this Mac at {}", server.origin());

    let session = match local::person(&pool).await.map_err(|e| e.to_string())? {
        Some(person) => Some((person, new_session(&pool, person).await?)),
        None => None,
    };
    Ok(Running {
        postgres,
        server,
        pool,
        session,
    })
}

/// Reminders as they come due, as notifications: the topic's name over
/// what to do.
fn notify(app: AppHandle, state: &AppState) {
    let mut due = state.due.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match due.recv().await {
                Ok(reminder) => {
                    let shown = app
                        .notification()
                        .builder()
                        .title(&reminder.topic)
                        .body(&reminder.title)
                        .show();
                    if let Err(error) = shown {
                        tracing::warn!("couldn't show a reminder: {error}");
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
}

async fn new_session(pool: &PgPool, person: Uuid) -> Result<String, String> {
    local::session(pool, person)
        .await
        .map_err(|e| e.to_string())
}

/// The Mac account's full name, to suggest as yours.
pub fn account_name() -> String {
    std::process::Command::new("id")
        .arg("-F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default()
}
