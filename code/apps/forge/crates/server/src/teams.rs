//! Creating teams. A team needs its workflow from the start, so teams are
//! created here rather than through sync; everything else about them syncs.
//! Admins only, like changing a team's settings through sync.

use axum::Json;
use axum::extract::State;
use forge_domain::{
    Actor, ModelKind, Operation, TeamCreate, Transaction, Validate, default_workflow,
};
use forge_sync::{Origin, apply_in};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct CreatedTeam {
    pub id: Uuid,
}

pub async fn create(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<TeamCreate>,
) -> Result<Json<CreatedTeam>, AppError> {
    // Teams and their workflows are team settings, which admins manage.
    member.require_admin()?;
    request.validate()?;
    let mut db = state.pool.begin().await?;
    let id = create_team(&mut db, member.actor, &request.name, &request.key).await?;
    db.commit().await?;
    Ok(Json(CreatedTeam { id }))
}

/// Creates a team with the default workflow, through the sync engine so
/// every client hears about it.
pub async fn create_team(
    db: &mut PgConnection,
    actor: Actor,
    name: &str,
    key: &str,
) -> Result<Uuid, AppError> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM teams WHERE workspace_id = $1 AND key = $2)",
    )
    .bind(actor.workspace_id)
    .bind(key)
    .fetch_one(&mut *db)
    .await?;
    if taken {
        return Err(AppError::conflict(format!(
            "another team already uses the key {key}"
        )));
    }

    let team = Uuid::now_v7();
    let mut operations = vec![Operation::Create {
        model: ModelKind::Team,
        id: team,
        data: object(json!({ "name": name, "key": key })),
    }];
    for (position, (state_name, kind, color)) in default_workflow().into_iter().enumerate() {
        operations.push(Operation::Create {
            model: ModelKind::WorkflowState,
            id: Uuid::now_v7(),
            data: object(json!({
                "team_id": team, "name": state_name, "type": kind, "color": color, "position": position,
            })),
        });
    }
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    apply_in(db, actor, &transaction, Origin::Server).await?;
    forge_sync::add_everyone(db, actor, team).await?;
    Ok(team)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
