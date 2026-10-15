use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Turns Postgres notifications about new log entries into wake-ups for the
/// WebSocket connections. Runs for the life of the server.
pub async fn listen(pool: PgPool, wake: broadcast::Sender<Uuid>) {
    loop {
        match PgListener::connect_with(&pool).await {
            Ok(mut listener) => {
                if let Err(e) = listener.listen(forge_sync::NOTIFY_CHANNEL).await {
                    tracing::warn!(error = %e, "couldn't listen for sync notifications");
                } else {
                    // Anything could have happened while we weren't listening.
                    let _ = wake.send(Uuid::nil());
                    relay(&mut listener, &wake).await;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "couldn't connect to listen for sync notifications")
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn relay(listener: &mut PgListener, wake: &broadcast::Sender<Uuid>) {
    loop {
        match listener.try_recv().await {
            Ok(Some(notification)) => {
                if let Ok(workspace_id) = notification.payload().parse::<Uuid>() {
                    let _ = wake.send(workspace_id);
                }
            }
            // The connection dropped and will reconnect; notifications in
            // between are lost, so wake everyone.
            Ok(None) => {
                tracing::warn!("sync notification connection lost; reconnecting");
                let _ = wake.send(Uuid::nil());
            }
            Err(e) => {
                tracing::warn!(error = %e, "sync notification listener failed");
                return;
            }
        }
    }
}
