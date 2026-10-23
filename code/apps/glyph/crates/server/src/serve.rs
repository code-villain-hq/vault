use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::{AppState, app, models};

/// How long open connections (live feeds above all) get to finish once
/// shutdown starts.
const GRACE: Duration = Duration::from_secs(3);

/// Runs Glyph on `listener`, with its workers, until `shutdown` resolves,
/// then stops the model servers it started. The server binary and the
/// desktop app both start it this way.
pub async fn serve(
    state: AppState,
    web_dir: Option<&Path>,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    if state.scim_token.is_some() {
        workers.spawn(code_villain_scim::watch_departures::<
            crate::groups::GlyphDirectory,
        >(state.pool.clone()));
    }
    workers.spawn(code_villain_sync::listen(
        state.pool.clone(),
        glyph_sync::NOTIFY_CHANNEL,
        state.wake.clone(),
    ));

    let processes = state.processes.clone();
    // With each connection's address, for limits per client.
    let service = app(state, web_dir).into_make_service_with_connect_info::<SocketAddr>();
    let (stopping, stopped) = oneshot::channel();
    let server = axum::serve(listener, service)
        .with_graceful_shutdown(async move {
            shutdown.await;
            let _ = stopping.send(());
        })
        .into_future();
    let result = tokio::select! {
        result = server => result,
        // A feed stays open as long as its app does: don't wait for it.
        () = async {
            let _ = stopped.await;
            tokio::time::sleep(GRACE).await;
        } => Ok(()),
    };
    workers.abort_all();
    processes.stop_all(models::GRACE).await;
    result
}
