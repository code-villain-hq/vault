//! Postrider on this Mac: the app runs Postrider's server on its own
//! Postgres, both on loopback, for the person this Mac's account names, and
//! hands the web app the address and a session. It starts on launch and
//! stops when the app quits. The accounts' passwords stay in the keychain.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use code_villain_desktop::{LocalPostgres, LocalServer};
use postrider_server::secrets::Secrets;
use postrider_server::{AppState, local};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Mutex;

/// The ports the app's server and Postgres take when they're free.
const SERVER_PORT: u16 = 7885;
const POSTGRES_PORT: u16 = 7886;

/// Where the web app finds Postrider.
#[derive(Clone, Serialize)]
pub struct Place {
    pub origin: String,
    pub token: String,
}

struct Running {
    postgres: LocalPostgres,
    server: LocalServer,
    pool: PgPool,
    /// This launch's session.
    token: String,
}

/// Held by the app. Commands wait on the lock while Postrider starts.
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

    /// On launch: start Postrider in the background. The lock is taken
    /// here, before the window can ask, so it waits.
    pub fn resume(&self) {
        let mut running = Arc::clone(&self.running)
            .try_lock_owned()
            .expect("nothing else has it at launch");
        let dir = self.dir.clone();
        tauri::async_runtime::spawn(async move {
            *running = start(&dir).await.map(Some);
        });
    }

    /// Where Postrider is, once it has started. If it didn't start, this
    /// tries again: the web app's "Try again" asks here.
    pub async fn place(&self) -> Result<Option<Place>, String> {
        let mut running = self.running.lock().await;
        if running.is_err() {
            *running = start(&self.dir).await.map(Some);
        }
        let running = running.as_ref().map_err(Clone::clone)?;
        Ok(running.as_ref().map(|running| Place {
            origin: running.server.origin().to_string(),
            token: running.token.clone(),
        }))
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
    match start_server(&postgres).await {
        Ok((server, pool, token)) => Ok(Running {
            postgres,
            server,
            pool,
            token,
        }),
        Err(e) => {
            tracing::error!("Postrider didn't start: {e}");
            let _ = postgres.stop().await;
            Err(e)
        }
    }
}

async fn start_server(postgres: &LocalPostgres) -> Result<(LocalServer, PgPool, String), String> {
    let url = postgres
        .database("postrider")
        .await
        .map_err(|e| e.to_string())?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;
    postrider_sync::MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let person = local::person_or_start(&pool, &local::account_name())
        .await
        .map_err(|e| e.message)?;
    let token = local::session(&pool, person).await.map_err(|e| e.message)?;

    let mut state = AppState::new(pool.clone(), Secrets::Keychain).on_this_mac();
    if cfg!(debug_assertions) {
        // `bun run dev:desktop` shows the web app from Vite.
        state = state.allowing_origin("http://localhost:5178");
    }
    let server = LocalServer::spawn(SERVER_PORT, |listener, shutdown| {
        postrider_server::serve(state, listener, shutdown)
    })
    .await
    .map_err(|e| e.to_string())?;
    tracing::info!("Postrider on this Mac at {}", server.origin());
    Ok((server, pool, token))
}
