use std::future::{Future, IntoFuture};
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::{AppState, app};

/// How long open connections (live feeds above all) get to finish once
/// shutdown starts.
const GRACE: Duration = Duration::from_secs(3);

/// Runs Postrider on `listener`, with its workers, until `shutdown`
/// resolves: the sync feed and a watcher for every account's Inbox. The
/// dev server and the desktop app both start it this way.
pub async fn serve(
    state: AppState,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let mut workers = JoinSet::new();
    workers.spawn(code_villain_sync::listen(
        state.pool.clone(),
        postrider_sync::NOTIFY_CHANNEL,
        state.wake.clone(),
    ));
    let mail = state.mail.clone();
    if let Err(e) = mail.watch_all().await {
        tracing::error!(error = %e, "couldn't start reading the Inboxes");
    }

    let (stopping, stopped) = oneshot::channel();
    let server = axum::serve(listener, app(state, None))
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
    mail.stop();
    workers.abort_all();
    result
}
