use std::future::{Future, IntoFuture};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

use crate::telemetry::{self, Batch};
use crate::{AppState, app, health};

/// How long open connections (live feeds above all) get to finish once
/// shutdown starts.
const GRACE: Duration = Duration::from_secs(3);

/// Runs Raven on `listener`, with its workers, until `shutdown` resolves:
/// the sync feed, the writer for the calls it records (`records`, from its
/// [`telemetry::Recorder`]) and the health checks. The server binary and
/// the desktop app both start it this way.
pub async fn serve(
    state: AppState,
    web_dir: Option<&Path>,
    listener: TcpListener,
    records: mpsc::Receiver<Batch>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    // Raven checks itself the way it checks the others: at its own address.
    let mut addr = listener.local_addr()?;
    if addr.ip().is_unspecified() {
        addr.set_ip(Ipv4Addr::LOCALHOST.into());
    }
    let self_url = format!("http://{addr}/api/health");

    let mut workers = JoinSet::new();
    workers.spawn(code_villain_sync::listen(
        state.pool.clone(),
        raven_sync::NOTIFY_CHANNEL,
        state.wake.clone(),
    ));
    workers.spawn(telemetry::write(state.pool.clone(), records));
    workers.spawn(health::watch(state.pool.clone(), self_url));

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
    result
}
