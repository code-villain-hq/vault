//! Workspaces on this Mac: the app runs Forge's server on its own Postgres,
//! both on loopback, and hands the web app the address and a session. It
//! starts on launch once someone has chosen "Start on this Mac", and stops
//! when the app quits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use code_villain_desktop::{LocalPostgres, LocalServer};
use forge_server::{AppState, local};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Mutex;
use uuid::Uuid;

/// The ports the app's server and Postgres take when they're free.
const SERVER_PORT: u16 = 7385;
const POSTGRES_PORT: u16 = 7386;

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

/// Held by the app. Commands wait on the lock while Forge starts.
pub struct ThisMac {
    dir: PathBuf,
    running: Arc<Mutex<Result<Option<Running>, String>>>,
}

impl ThisMac {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            running: Arc::new(Mutex::new(Ok(None))),
        }
    }

    /// On launch: start Forge in the background if this Mac has workspaces.
    /// The lock is taken here, before the window can ask, so it waits.
    pub fn resume(&self) {
        let mut running = Arc::clone(&self.running)
            .try_lock_owned()
            .expect("nothing else has it at launch");
        let dir = self.dir.clone();
        tauri::async_runtime::spawn(async move {
            if dir.join("postgres").join("data").exists() {
                *running = start(&dir).await.map(Some);
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
            *guard = start(&self.dir).await.map(Some);
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

async fn start(dir: &Path) -> Result<Running, String> {
    let postgres = LocalPostgres::start(&dir.join("postgres"), POSTGRES_PORT)
        .await
        .map_err(|e| e.to_string())?;
    let url = postgres
        .database("forge")
        .await
        .map_err(|e| e.to_string())?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;
    forge_sync::MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut state = AppState::new(pool.clone(), false)
        .on_this_mac()
        .with_data_dir(dir.join("files"));
    if cfg!(debug_assertions) {
        // `bun run dev:desktop` shows the web app from Vite.
        state = state.allowing_origin("http://localhost:5173");
    }
    let server = LocalServer::spawn(SERVER_PORT, |listener, shutdown| {
        forge_server::serve(state, None, listener, shutdown)
    })
    .await
    .map_err(|e| e.to_string())?;
    tracing::info!("Forge on this Mac at {}", server.origin());

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
