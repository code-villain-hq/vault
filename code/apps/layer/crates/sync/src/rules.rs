//! Layer's rules for the shared sync engine ([`code_villain_sync`]):
//! everyone works on the repositories and their notes, and admins say
//! where the workspace's Jira, GitHub, Slack, Grafana and logs are, and
//! what a repository's check command is.

use code_villain_sync::Rules;
use layer_domain::{
    Actor, ConnectionKind, ModelKind, NOTES_PER_REPO, Operation, remote_key, specs_in_app,
};
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Layer, as the sync engine sees it.
pub struct Layer;

impl Rules for Layer {
    type Model = ModelKind;
    const NOTIFY_CHANNEL: &'static str = NOTIFY_CHANNEL;

    async fn guest_access(db: &mut PgConnection, actor: Actor) -> Result<Option<i64>, SyncError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT role, access_version FROM workspace_members \
             WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(actor.workspace_id)
        .bind(actor.user_id)
        .fetch_optional(&mut *db)
        .await?;
        let (role, version) = row.ok_or_else(|| SyncError::NotFound("workspace".into()))?;
        Ok((role == "guest").then_some(version))
    }

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
            // A loop runs it on every member's Mac, as them (LOOP-1). Before,
            // any member could make everyone's Mac run a command of theirs.
            ModelKind::Repository if sets_check(db, actor, operation).await? => {
                "Only admins can change a repository's check command."
            }
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
                specs_in_app(row);
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
            ModelKind::Connection => {
                if let (Some(kind), Some(settings)) = (kind_of(row), row.get("settings")) {
                    check_github_hosts(db, actor, kind, settings).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn prepare_update(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        _id: Uuid,
        before: Option<&Value>,
        data: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if model == ModelKind::Repository {
            specs_in_app(data);
            // An older Layer's `specs_in_app: false` alone leaves nothing to
            // write, and an UPDATE that sets nothing is not SQL.
            if data.is_empty()
                && let Some(now) = before.and_then(|b| b.get("spec_commit"))
            {
                data.insert("spec_commit".into(), now.clone());
            }
        }
        // The patch carries only the settings; which kind they must be is
        // the record's.
        if model == ModelKind::Connection
            && let (Some(settings), Some(kind)) = (
                data.get("settings"),
                before.and_then(Value::as_object).and_then(kind_of),
            )
        {
            kind.check_settings(settings)
                .map_err(|e| SyncError::Invalid(e.0))?;
            check_github_hosts(db, actor, kind, settings).await?;
        }
        Ok(())
    }

    async fn cascade(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        match model {
            // A repository's notes stay: they are kept by its remote, for
            // when it is added again.
            ModelKind::Repository => {
                crate::access::repository_going(db, actor, id).await?;
                Ok(Vec::new())
            }
            ModelKind::Connection
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

/// Whether `operation` gives a repository a check command it hasn't got.
/// Clearing one is anyone's, and so is sending it unchanged, as the web
/// app's settings form does with every save.
async fn sets_check(
    db: &mut PgConnection,
    actor: Actor,
    operation: &Operation,
) -> Result<bool, SyncError> {
    let (id, data) = match operation {
        Operation::Create { data, .. } => (None, data),
        Operation::Update { id, data, .. } => (Some(*id), data),
        Operation::Delete { .. } => return Ok(false),
    };
    let Some(check) = data.get("check_command").filter(|c| !c.is_null()) else {
        return Ok(false);
    };
    let Some(id) = id else { return Ok(true) };
    let now: Option<Option<String>> = sqlx::query_scalar(
        "SELECT check_command FROM repositories WHERE workspace_id = $1 AND id = $2",
    )
    .bind(actor.workspace_id)
    .bind(id)
    .fetch_optional(&mut *db)
    .await?;
    Ok(now.flatten().as_deref() != check.as_str())
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

fn kind_of(record: &Map<String, Value>) -> Option<ConnectionKind> {
    serde_json::from_value(record.get("kind")?.clone()).ok()
}

/// The sync engine locks the workspace for the transaction, so two updates
/// cannot claim the same host in the two different record kinds.
async fn check_github_hosts(
    db: &mut PgConnection,
    actor: Actor,
    kind: ConnectionKind,
    settings: &Value,
) -> Result<(), SyncError> {
    let (other_kind, other_name) = match kind {
        ConnectionKind::Github => (ConnectionKind::GithubSources, "github_sources"),
        ConnectionKind::GithubSources => (ConnectionKind::Github, "github"),
        _ => return Ok(()),
    };
    let other: Option<Value> = sqlx::query_scalar(
        "SELECT settings FROM connections WHERE workspace_id = $1 AND kind = $2",
    )
    .bind(actor.workspace_id)
    .bind(other_name)
    .fetch_optional(&mut *db)
    .await?;
    if let Some(other) = other {
        let hosts = kind
            .github_hosts(settings)
            .map_err(|e| SyncError::Invalid(e.0))?;
        let taken = other_kind
            .github_hosts(&other)
            .map_err(|e| SyncError::Invalid(e.0))?;
        if hosts.iter().any(|host| taken.contains(host)) {
            return Err(SyncError::Invalid(
                "The workspace already has a GitHub source for that host.".into(),
            ));
        }
    }
    Ok(())
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
