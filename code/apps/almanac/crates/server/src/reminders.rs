//! Telling the Mac when a reminder comes due. Every 30 seconds Almanac
//! looks for reminders that are on, notify, and are due (at their time, or
//! at nine when they have none) but haven't been told for this due date.
//! It marks them told and sends them on [`AppState::due`]; the desktop app
//! turns them into notifications. A repeating one comes due again once
//! it's done and its date moves on. With no Mac listening, as on a
//! company's server, nothing is marked.

use std::time::Duration;

use almanac_domain::{Actor, ModelKind, Operation};
use chrono::{Local, NaiveDate};
use serde_json::{Map, json};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;
use crate::topics::write;

const DUE: &str = "SELECT r.id, r.workspace_id, r.topic_id, r.title, t.name, r.due_on \
     FROM reminders r JOIN topics t ON t.id = r.topic_id \
     WHERE r.status = 'on' AND r.notify \
     AND r.due_on + COALESCE(r.at, time '09:00') <= $1 \
     AND r.notified_for IS DISTINCT FROM r.due_on \
     AND EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = r.workspace_id) \
     ORDER BY r.due_on, r.at LIMIT 50";
const SOMEONE: &str = "SELECT user_id FROM workspace_members \
     WHERE workspace_id = $1 ORDER BY role <> 'admin', created_at LIMIT 1";

/// A reminder that just came due.
#[derive(Debug, Clone)]
pub struct Due {
    pub workspace_id: Uuid,
    pub id: Uuid,
    pub topic_id: Uuid,
    pub topic: String,
    pub title: String,
}

pub async fn watch(state: AppState) {
    loop {
        let listening = state.due.receiver_count() > 0;
        if listening && let Err(error) = tell(&state, &state.due).await {
            tracing::warn!(%error, "couldn't look for due reminders");
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// Marks what's due as told and sends it. The Mac's clock says what's due:
/// reminders are in the time where you are, and only the Mac's own server
/// has a Mac listening, so its clock is the Mac's. A company's server marks
/// nothing; the app rings its reminders by the device's clock.
pub async fn tell(state: &AppState, due: &broadcast::Sender<Due>) -> Result<usize, AppError> {
    let now = Local::now().naive_local();
    let rows: Vec<(Uuid, Uuid, Uuid, String, String, NaiveDate)> =
        sqlx::query_as(DUE).bind(now).fetch_all(&state.pool).await?;
    let count = rows.len();
    for (id, workspace_id, topic_id, title, topic, due_on) in rows {
        let Some(user_id): Option<Uuid> = sqlx::query_scalar(SOMEONE)
            .bind(workspace_id)
            .fetch_optional(&state.pool)
            .await?
        else {
            continue;
        };
        let actor = Actor {
            user_id,
            workspace_id,
        };
        let data: Map<_, _> = [("notified_for".to_string(), json!(due_on))]
            .into_iter()
            .collect();
        // One that can't be marked is left for the next look, not the rest.
        let marked = write(
            state,
            actor,
            vec![Operation::Update {
                model: ModelKind::Reminder,
                id,
                data,
            }],
        )
        .await;
        if let Err(error) = marked {
            tracing::warn!(%error, %id, "couldn't mark a reminder told");
            continue;
        }
        let _ = due.send(Due {
            workspace_id,
            id,
            topic_id,
            topic,
            title,
        });
    }
    Ok(count)
}
