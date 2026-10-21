//! Layer's rules for the shared sync engine ([`code_villain_sync`]):
//! everyone works on the repositories and their notes, and admins say
//! where the workspace's Jira, GitHub, Slack, Grafana and logs are.

use code_villain_sync::Rules;
use layer_domain::{Actor, ConnectionKind, ModelKind, NOTES_PER_REPO, Operation, remote_key};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Layer, as the sync engine sees it.
pub struct Layer;

impl Rules for Layer {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn check_allowed(
        db: &mut PgConnection,
        actor: Actor,
        operation: &Operation,
    ) -> Result<(), SyncError> {
        let (Operation::Create { model, .. }
        | Operation::Update { model, .. }
        | Operation::Delete { model, .. }) = operation;
        let message = match model {
            ModelKind::Connection => "Only admins can change the workspace's connections.",
            ModelKind::LogSource => "Only admins can change the workspace's log sources.",
            _ => return Ok(()),
        };
        if is_admin(db, actor).await? {
            Ok(())
        } else {
            Err(SyncError::Forbidden(message.into()))
        }
    }

    async fn prepare_create(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        row: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        match model {
            ModelKind::Repository => {
                // One spelling per repository, whichever way it was cloned,
                // so two Macs that cloned it differently share it.
                if let Some(url) = row.get("remote_key").and_then(Value::as_str) {
                    let key = remote_key(url);
                    row.insert("remote_key".into(), json!(key));
                }
                row.insert("added_by".into(), json!(actor.user_id));
            }
            ModelKind::Note => {
                let repo = row.get("repo").and_then(Value::as_str).unwrap_or_default();
                if note_count(db, actor, repo).await? >= NOTES_PER_REPO as i64 {
                    return Err(SyncError::Invalid(format!(
                        "This repository already has {NOTES_PER_REPO} notes; forget one that no \
                         longer holds before remembering another."
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn prepare_update(
        _db: &mut PgConnection,
        _actor: Actor,
        model: ModelKind,
        _id: Uuid,
        before: Option<&Value>,
        data: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        // The patch carries only the settings; which kind they must be is
        // the record's.
        if model == ModelKind::Connection
            && let (Some(settings), Some(kind)) = (data.get("settings"), before.and_then(kind_of))
        {
            kind.check_settings(settings)
                .map_err(|e| SyncError::Invalid(e.0))?;
        }
        Ok(())
    }

    async fn cascade(
        _db: &mut PgConnection,
        _actor: Actor,
        model: ModelKind,
        _id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        match model {
            // A repository's notes stay: they are kept by its remote, for
            // when it is added again.
            ModelKind::Repository
            | ModelKind::Connection
            | ModelKind::LogSource
            | ModelKind::AgentModel
            | ModelKind::Note => Ok(Vec::new()),
            ModelKind::User => Err(SyncError::Invalid(format!(
                "{} records can't be deleted this way",
                model.name()
            ))),
        }
    }

    fn constraint_words(constraint: &str) -> Option<&'static str> {
        plain_words(constraint)
    }
}

/// Whether the actor is an admin of the workspace.
async fn is_admin(db: &mut PgConnection, actor: Actor) -> Result<bool, SyncError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .fetch_optional(&mut *db)
    .await?;
    Ok(role.as_deref() == Some("admin"))
}

async fn note_count(db: &mut PgConnection, actor: Actor, repo: &str) -> Result<i64, SyncError> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM notes WHERE workspace_id = $1 AND repo = $2")
            .bind(actor.workspace_id)
            .bind(repo)
            .fetch_one(&mut *db)
            .await?,
    )
}

fn kind_of(record: &Value) -> Option<ConnectionKind> {
    serde_json::from_value(record.get("kind")?.clone()).ok()
}

/// What a broken constraint means, for the ones people run into.
fn plain_words(constraint: &str) -> Option<&'static str> {
    Some(match constraint {
        "repositories_workspace_remote_key" => "The workspace already has that repository.",
        "connections_workspace_kind" => "The workspace is already connected to that.",
        "agent_models_workspace_agent" => "That agent's models are already kept here.",
        _ => return None,
    })
}
