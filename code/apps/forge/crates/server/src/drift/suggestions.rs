//! Accepting or dismissing a suggestion. Accepting applies its changes as
//! the person who accepts, through the same rules as their own edits.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use forge_domain::{ModelKind, Operation, Transaction};
use forge_sync::{Origin, apply_in};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{object, write};
use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

pub async fn accept(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    resolve(&state, member, id, true).await
}

pub async fn dismiss(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    resolve(&state, member, id, false).await
}

async fn resolve(
    state: &AppState,
    member: Member,
    id: Uuid,
    accept: bool,
) -> Result<StatusCode, AppError> {
    let mut db = state.pool.begin().await?;
    // The workspace first, as every write does, so this can't deadlock with
    // someone editing the same issue.
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(member.actor.workspace_id)
        .execute(&mut *db)
        .await?;
    let found: Option<(String, Value, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, operations, issue_id FROM suggestions WHERE id = $1 AND workspace_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(member.actor.workspace_id)
    .fetch_optional(&mut *db)
    .await?;
    let not_found = || AppError::not_found("suggestion not found");
    let (status, operations, issue) = found.ok_or_else(not_found)?;
    // A guest sees only the suggestions on their teams' issues.
    let (workspace, user) = (member.actor.workspace_id, member.actor.user_id);
    if member.role == forge_domain::Role::Guest
        && (issue.is_none() || !forge_sync::sees_issue(&mut db, workspace, user, issue).await?)
    {
        return Err(not_found());
    }
    if status != "open" {
        return Err(AppError::conflict(format!(
            "this suggestion was already {status}"
        )));
    }

    if accept {
        let mut operations: Vec<Operation> =
            serde_json::from_value(operations).map_err(AppError::internal)?;
        add_labels(&mut db, member.actor.workspace_id, &mut operations).await?;
        if !operations.is_empty() {
            let transaction = Transaction {
                id: Uuid::now_v7(),
                operations,
            };
            apply_in(&mut db, member.actor, &transaction, Origin::Client)
                .await
                .map_err(|e| {
                    AppError::invalid(format!(
                        "the changes no longer apply ({e}); dismiss it instead"
                    ))
                })?;
        }
    }
    let data = object(json!({
        "status": if accept { "accepted" } else { "dismissed" },
        "resolved_by": member.actor.user_id,
        "resolved_at": Utc::now(),
    }));
    write(
        &mut db,
        member.actor,
        vec![Operation::Update {
            model: ModelKind::Suggestion,
            id,
            data,
        }],
    )
    .await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Turns "add these labels" into the issue's labels as they are now plus
/// those, so labels added since the suggestion was made are kept.
async fn add_labels(
    db: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    operations: &mut [Operation],
) -> Result<(), AppError> {
    for operation in operations.iter_mut() {
        let Operation::Update {
            model: ModelKind::Issue,
            id,
            data,
        } = operation
        else {
            continue;
        };
        let Some(add) = data.remove(super::triage::ADD_LABELS) else {
            continue;
        };
        let add: Vec<Uuid> = serde_json::from_value(add).map_err(AppError::internal)?;
        let mut labels: Vec<Uuid> =
            sqlx::query_scalar("SELECT label_ids FROM issues WHERE id = $1 AND workspace_id = $2")
                .bind(*id)
                .bind(workspace_id)
                .fetch_optional(&mut *db)
                .await?
                .unwrap_or_default();
        for label in add {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        data.insert("label_ids".into(), json!(labels));
    }
    Ok(())
}
