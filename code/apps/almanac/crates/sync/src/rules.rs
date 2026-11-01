//! Almanac's rules for the shared sync engine ([`code_villain_sync`]): what
//! a talk said is the server's, a topic's records go with it, a block's
//! points with the block, and only the server marks a reminder as told.

use almanac_domain::{Actor, ModelKind, Operation};
use code_villain_sync::Rules;
use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{NOTIFY_CHANNEL, SyncError};

/// Almanac, as the sync engine sees it.
pub struct Almanac;

const TOPIC_NEXT: &str =
    "SELECT COALESCE(max(position), 0) + 1 FROM topics WHERE workspace_id = $1";
const POINT_NEXT: &str = "SELECT COALESCE(max(position), 0) + 1 FROM points \
     WHERE workspace_id = $1 AND topic_id = $2";
const BLOCK_NEXT: &str = "SELECT COALESCE(max(position), -1) + 1 FROM blocks \
     WHERE workspace_id = $1 AND topic_id = $2";
const TOPIC_BLOCKS: &str = "SELECT id FROM blocks WHERE topic_id = $1 AND workspace_id = $2";
const BLOCK_POINTS: &str = "SELECT id FROM points WHERE block_id = $1 AND workspace_id = $2";
const ROW_BLOCKS: &str = "SELECT id FROM blocks WHERE row_id = $1 AND workspace_id = $2";
const TOPIC_REMINDERS: &str = "SELECT id FROM reminders WHERE topic_id = $1 AND workspace_id = $2";
const TOPIC_QUESTIONS: &str = "SELECT id FROM questions WHERE topic_id = $1 AND workspace_id = $2";
const TOPIC_POINTS: &str = "SELECT id FROM points WHERE topic_id = $1 AND workspace_id = $2";
const TOPIC_MESSAGES: &str = "SELECT id FROM messages WHERE topic_id = $1 AND workspace_id = $2";
const TOPIC_TALKS: &str = "SELECT id FROM talks WHERE topic_id = $1 AND workspace_id = $2";
const TALK_MESSAGES: &str = "SELECT id FROM messages WHERE talk_id = $1 AND workspace_id = $2";
const TOPIC_CHILDREN: &str = "SELECT id FROM topics WHERE parent_id = $1 AND workspace_id = $2";
/// Whether a topic can be a parent: a plain topic of the workspace that
/// sits under none itself.
const CAN_PARENT: &str = "SELECT EXISTS (SELECT 1 FROM topics \
     WHERE id = $1 AND workspace_id = $2 AND kind = 'topic' AND parent_id IS NULL)";
const HAS_CHILDREN: &str =
    "SELECT EXISTS (SELECT 1 FROM topics WHERE parent_id = $1 AND workspace_id = $2)";
const OWN_TOPIC: &str = "SELECT EXISTS (SELECT 1 FROM topics \
     WHERE id = $1 AND workspace_id = $2 AND kind = 'almanac')";

impl Rules for Almanac {
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
        match operation {
            Operation::Update {
                model: ModelKind::Reminder,
                data,
                ..
            } if data.contains_key("notified_for") => Err(SyncError::Forbidden(
                "Only Almanac says when it told the Mac.".into(),
            )),
            // A topic sits under another one level deep, and never under
            // itself or the workspace's own topic.
            Operation::Create {
                model: ModelKind::Topic,
                id,
                data,
            }
            | Operation::Update {
                model: ModelKind::Topic,
                id,
                data,
            } => match data.get("parent_id").and_then(Value::as_str) {
                None => Ok(()),
                Some(parent) => {
                    let parent = parent
                        .parse()
                        .map_err(|_| SyncError::Invalid("That parent isn't a topic.".into()))?;
                    may_sit_under(db, actor.workspace_id, *id, parent).await
                }
            },
            // The workspace's own topic stays: it's how topics start.
            Operation::Delete {
                model: ModelKind::Topic,
                id,
            } => {
                let own: bool = sqlx::query_scalar(OWN_TOPIC)
                    .bind(id)
                    .bind(actor.workspace_id)
                    .fetch_one(&mut *db)
                    .await?;
                if own {
                    return Err(SyncError::Forbidden(
                        "Almanac's own topic stays. You can rename it.".into(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn prepare_create(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        row: &mut Map<String, Value>,
    ) -> Result<(), SyncError> {
        if row.get("position").is_some_and(|p| !p.is_null()) {
            return Ok(());
        }
        let position: i32 = match model {
            ModelKind::Topic => {
                sqlx::query_scalar(TOPIC_NEXT)
                    .bind(actor.workspace_id)
                    .fetch_one(&mut *db)
                    .await?
            }
            ModelKind::Block | ModelKind::Point => {
                let Some(topic) = row
                    .get("topic_id")
                    .and_then(Value::as_str)
                    .and_then(|t| t.parse::<Uuid>().ok())
                else {
                    return Ok(());
                };
                let next = if model == ModelKind::Block {
                    BLOCK_NEXT
                } else {
                    POINT_NEXT
                };
                sqlx::query_scalar(next)
                    .bind(actor.workspace_id)
                    .bind(topic)
                    .fetch_one(&mut *db)
                    .await?
            }
            _ => return Ok(()),
        };
        row.insert("position".into(), json!(position));
        Ok(())
    }

    async fn cascade(
        db: &mut PgConnection,
        actor: Actor,
        model: ModelKind,
        id: Uuid,
    ) -> Result<Vec<Operation>, SyncError> {
        // What a topic kept goes with it, a block's points with the block,
        // a row's page with the row, and a talk's messages with the talk,
        // logged so every client lets go of them too.
        let mut operations = Vec::new();
        let children: &[(ModelKind, &'static str)] = match model {
            ModelKind::Topic => {
                // Its sub-topics stay, at the top.
                let children: Vec<Uuid> = sqlx::query_scalar(TOPIC_CHILDREN)
                    .bind(id)
                    .bind(actor.workspace_id)
                    .fetch_all(&mut *db)
                    .await?;
                operations.extend(children.into_iter().map(|id| Operation::Update {
                    model: ModelKind::Topic,
                    id,
                    data: Map::from_iter([("parent_id".into(), Value::Null)]),
                }));
                &[
                    (ModelKind::Reminder, TOPIC_REMINDERS),
                    (ModelKind::Question, TOPIC_QUESTIONS),
                    (ModelKind::Point, TOPIC_POINTS),
                    (ModelKind::Block, TOPIC_BLOCKS),
                    (ModelKind::Message, TOPIC_MESSAGES),
                    (ModelKind::Talk, TOPIC_TALKS),
                ]
            }
            ModelKind::Block => &[(ModelKind::Point, BLOCK_POINTS)],
            ModelKind::Point => &[(ModelKind::Block, ROW_BLOCKS)],
            ModelKind::Talk => &[(ModelKind::Message, TALK_MESSAGES)],
            ModelKind::User => {
                return Err(SyncError::Invalid(format!(
                    "{} records can't be deleted this way",
                    model.name()
                )));
            }
            _ => &[],
        };
        for (child, query) in children {
            let ids: Vec<Uuid> = sqlx::query_scalar(*query)
                .bind(id)
                .bind(actor.workspace_id)
                .fetch_all(&mut *db)
                .await?;
            operations.extend(
                ids.into_iter()
                    .map(|id| Operation::Delete { model: *child, id }),
            );
        }
        Ok(operations)
    }
}

/// Whether `topic` may sit under `parent`: one level deep, never under
/// itself or the workspace's own topic, and never the workspace's own
/// topic under another.
pub async fn may_sit_under(
    db: &mut PgConnection,
    workspace_id: Uuid,
    topic: Uuid,
    parent: Uuid,
) -> Result<(), SyncError> {
    let can: bool = sqlx::query_scalar(CAN_PARENT)
        .bind(parent)
        .bind(workspace_id)
        .fetch_one(&mut *db)
        .await?;
    let has: bool = sqlx::query_scalar(HAS_CHILDREN)
        .bind(topic)
        .bind(workspace_id)
        .fetch_one(&mut *db)
        .await?;
    let own: bool = sqlx::query_scalar(OWN_TOPIC)
        .bind(topic)
        .bind(workspace_id)
        .fetch_one(&mut *db)
        .await?;
    if parent == topic || !can || has || own {
        return Err(SyncError::Invalid(
            "A topic sits under another one level deep, not under itself or Almanac's own, \
             and Almanac's own sits under none."
                .into(),
        ));
    }
    Ok(())
}
