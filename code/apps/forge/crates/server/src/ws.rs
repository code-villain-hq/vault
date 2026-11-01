//! The WebSocket clients follow the change log on, served by the studio's
//! sync engine ([`code_villain_sync::follow_upgrade`]).
//!
//! The client connects with the last sync id and epoch it has. The server
//! sends everything after that sync id, then each new change as it happens.
//! If the client's copy can't be caught up, the server asks it to resync.

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use code_villain_sync::{Position, follow_upgrade};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{Member, SYNC_PROTOCOL, still_member};
use crate::state::AppState;

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
    let actor = member.actor;
    let token_hash = member.token_hash;
    let pool = state.pool.clone();
    follow_upgrade::<forge_sync::Forge, _, _>(
        ws,
        SYNC_PROTOCOL,
        pool.clone(),
        state.wake.clone(),
        actor,
        Position {
            after: params.after,
            epoch: params.epoch,
        },
        // Before every change sent, not only on the heartbeat: once signed
        // out or removed from the workspace, nothing more of it reaches them.
        move || {
            let pool = pool.clone();
            let token_hash = token_hash.clone();
            async move { still_member(&pool, &token_hash, actor).await.unwrap_or(false) }
        },
    )
}
