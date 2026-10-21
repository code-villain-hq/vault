//! The WebSocket clients follow the change log on.
//!
//! The client connects with the last sync id and epoch it has. The server
//! sends everything after that sync id, then each new change as it happens.
//! If the client's copy can't be caught up, the server asks it to resync.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use glyph_domain::{Actor, ServerMessage};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::auth::{Member, SYNC_PROTOCOL, still_member};
use crate::state::AppState;

/// How many log entries to read at a time.
const PAGE: i64 = 500;
/// How often to ping, check the session is still valid, and look for changes
/// in case a notification was missed.
const HEARTBEAT: Duration = Duration::from_secs(25);

#[derive(Deserialize)]
pub struct Params {
    after: i64,
    epoch: Option<Uuid>,
}

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<Params>,
) -> Response {
    // Selecting the protocol the desktop app offers next to its token, as
    // browsers require; the browser app offers none.
    ws.protocols([SYNC_PROTOCOL])
        .on_upgrade(move |socket| async move {
            if let Err(e) = run(socket, state, member, params).await {
                tracing::debug!(error = %e, "sync socket closed");
            }
        })
}

type Sender = SplitSink<WebSocket, Message>;

async fn run(
    socket: WebSocket,
    state: AppState,
    member: Member,
    params: Params,
) -> anyhow::Result<()> {
    let actor = member.actor;
    // Subscribe before catching up, so no change slips between the two.
    let mut wake = state.wake.subscribe();
    let (mut sender, mut receiver) = socket.split();

    let epoch = glyph_sync::workspace_epoch(&state.pool, actor.workspace_id).await?;
    let current = glyph_sync::current_sync_id(&state.pool, actor.workspace_id).await?;
    if params.epoch != Some(epoch) || params.after > current {
        send(&mut sender, &ServerMessage::Resync).await?;
        return Ok(());
    }

    let mut last = params.after;
    send_new(&mut sender, &state, actor, &mut last).await?;

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
        // Before every change sent, not only on the heartbeat: once signed out
        // or removed from the workspace, nothing more of it reaches them.
        if !still_member(&state.pool, &member.token_hash, actor)
            .await
            .unwrap_or(false)
        {
            sender.send(Message::Close(None)).await?;
            return Ok(());
        }
        send_new(&mut sender, &state, actor, &mut last).await?;
    }
}

/// Sends every change after `last`, page by page, and advances `last`.
async fn send_new(
    sender: &mut Sender,
    state: &AppState,
    actor: Actor,
    last: &mut i64,
) -> anyhow::Result<()> {
    loop {
        // `upto` can pass entries meant for others; moving the client past
        // them is what confirms its transactions that created them.
        let (entries, upto) = glyph_sync::entries_after(&state.pool, actor, *last, PAGE).await?;
        if upto == *last {
            return Ok(());
        }
        *last = upto;
        send(
            sender,
            &ServerMessage::Delta {
                entries,
                sync_id: upto,
            },
        )
        .await?;
    }
}

async fn send(sender: &mut Sender, message: &ServerMessage) -> anyhow::Result<()> {
    let text = serde_json::to_string(message)?;
    sender.send(Message::Text(text.into())).await?;
    Ok(())
}
