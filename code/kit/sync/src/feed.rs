//! The WebSocket clients follow the change log on.
//!
//! The client connects with the last sync id and epoch it has. The server
//! sends everything after that sync id, then each new change as it happens.
//! If the client's copy can't be caught up, the server asks it to resync.

use std::future::Future;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use sqlx::PgPool;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::scope::served_epoch;
use crate::{Actor, Rules, ServerMessage, current_sync_id, entries_after, workspace_epoch};

/// How many log entries to read at a time.
const PAGE: i64 = 500;
/// How often to ping, check the session is still valid, and look for changes
/// in case a notification was missed.
const HEARTBEAT: Duration = Duration::from_secs(25);

/// Where a client's copy is: the last sync id it has seen, in which epoch.
pub struct Position {
    pub after: i64,
    pub epoch: Option<Uuid>,
}

/// Sends `actor` the workspace's changes after `from` on `socket` until the
/// client leaves or `still_allowed` says they may no longer see it. `wake`
/// carries workspace ids as [`listen`](crate::listen) hears of them (nil for
/// all); subscribe to it before calling, so no change slips in between.
/// When what they may see changes ([`Rules::guest_access`]), the client is
/// asked to resync.
pub async fn follow<R: Rules, F, Fut>(
    socket: WebSocket,
    pool: &PgPool,
    mut wake: broadcast::Receiver<Uuid>,
    actor: Actor,
    from: Position,
    mut still_allowed: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let (mut sender, mut receiver) = socket.split();

    let epoch = epoch_of::<R>(pool, actor).await?;
    let current = current_sync_id(pool, actor.workspace_id).await?;
    if from.epoch != Some(epoch) || from.after > current {
        send::<R>(&mut sender, &ServerMessage::Resync).await?;
        return Ok(());
    }

    let mut last = from.after;
    if !send_new::<R, _, _>(
        &mut sender,
        pool,
        actor,
        &mut last,
        epoch,
        &mut still_allowed,
    )
    .await?
    {
        return Ok(());
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            woken = wake.recv() => match woken {
                Ok(id) if id == actor.workspace_id || id.is_nil() => {}
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return Ok(()),
            },
            _ = heartbeat.tick() => {
                sender.send(Message::Ping(Default::default())).await?;
            }
            incoming = receiver.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(_)) => continue,
            },
        }
        if !send_new::<R, _, _>(
            &mut sender,
            pool,
            actor,
            &mut last,
            epoch,
            &mut still_allowed,
        )
        .await?
        {
            return Ok(());
        }
    }
}

/// The epoch `actor`'s copy belongs to now.
async fn epoch_of<R: Rules>(pool: &PgPool, actor: Actor) -> anyhow::Result<Uuid> {
    let epoch = workspace_epoch(pool, actor.workspace_id).await?;
    let mut db = pool.acquire().await?;
    Ok(served_epoch(epoch, R::guest_access(&mut db, actor).await?))
}

type Sender = SplitSink<WebSocket, Message>;

/// Sends every change after `last`, page by page, and advances `last`.
async fn send_new<R: Rules, F, Fut>(
    sender: &mut Sender,
    pool: &PgPool,
    actor: Actor,
    last: &mut i64,
    epoch: Uuid,
    still_allowed: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    loop {
        if !check_access::<R, _, _>(sender, pool, actor, epoch, still_allowed).await? {
            return Ok(false);
        }
        // `upto` can pass entries meant for others; moving the client past
        // them is what confirms its transactions that created them.
        let (entries, upto) = entries_after::<R>(pool, actor, *last, PAGE).await?;
        if upto == *last {
            return Ok(true);
        }
        // A removal or changed grant during the read must discard this page.
        if !check_access::<R, _, _>(sender, pool, actor, epoch, still_allowed).await? {
            return Ok(false);
        }
        *last = upto;
        send::<R>(
            sender,
            &ServerMessage::Delta {
                entries,
                sync_id: upto,
            },
        )
        .await?;
    }
}

async fn check_access<R: Rules, F, Fut>(
    sender: &mut Sender,
    pool: &PgPool,
    actor: Actor,
    epoch: Uuid,
    still_allowed: &mut F,
) -> anyhow::Result<bool>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    if !still_allowed().await {
        sender.send(Message::Close(None)).await?;
        return Ok(false);
    }
    if epoch_of::<R>(pool, actor).await? != epoch {
        send::<R>(sender, &ServerMessage::Resync).await?;
        return Ok(false);
    }
    Ok(true)
}

async fn send<R: Rules>(
    sender: &mut Sender,
    message: &ServerMessage<R::Model>,
) -> anyhow::Result<()> {
    let text = serde_json::to_string(message)?;
    sender.send(Message::Text(text.into())).await?;
    Ok(())
}
