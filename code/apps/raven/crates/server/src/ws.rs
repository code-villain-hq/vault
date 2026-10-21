//! The WebSocket clients follow the change log on, served by the studio's
//! sync engine ([`code_villain_sync::follow`]).
//!
//! The client connects with the last sync id and epoch it has. The server
//! sends everything after that sync id, then each new change as it happens.
//! If the client's copy can't be caught up, the server asks it to resync.

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use code_villain_sync::Position;
use raven_domain::ModelKind;
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
    // Selecting the protocol the desktop app offers next to its token, as
    // browsers require; the browser app offers none.
    ws.protocols([SYNC_PROTOCOL])
        .on_upgrade(move |socket| async move {
            let actor = member.actor;
            // Subscribe before catching up, so no change slips between the two.
            let wake = state.wake.subscribe();
            let from = Position {
                after: params.after,
                epoch: params.epoch,
            };
            // Before every change sent, not only on the heartbeat: once signed
            // out or removed from the workspace, nothing more of it reaches them.
            let still_allowed = || async {
                still_member(&state.pool, &member.token_hash, actor)
                    .await
                    .unwrap_or(false)
            };
            let followed = code_villain_sync::follow::<ModelKind, _, _>(
                socket,
                &state.pool,
                wake,
                actor,
                from,
                still_allowed,
            )
            .await;
            if let Err(e) = followed {
                tracing::debug!(error = %e, "sync socket closed");
            }
        })
}
