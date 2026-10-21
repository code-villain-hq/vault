use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::routes::Web;
use crate::{AppState, app};

/// How long open connections (live feeds above all) get to finish once
/// shutdown starts.
const GRACE: Duration = Duration::from_secs(3);

/// Runs Layer's server on `listener`, with the sync feed, until `shutdown`
/// resolves. The server binary and the desktop app both start it this way.
pub async fn serve(
    state: AppState,
    web: Web<'_>,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    workers.spawn(code_villain_sync::listen(
        state.pool.clone(),
        layer_sync::NOTIFY_CHANNEL,
        state.wake.clone(),
    ));

    // With each connection's address, for limits per client.
    let service = app(state, web).into_make_service_with_connect_info::<SocketAddr>();
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
