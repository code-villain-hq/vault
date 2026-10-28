//! The workspace's own topic, called Almanac: every workspace has one,
//! above Calendar and out of the groups. It knows the other topics,
//! proposes one when you say what you want to keep track of, and changes a
//! topic you name. Its page is the index of the workspace. It can be
//! renamed, not deleted (`crates/sync`'s rules).

use almanac_domain::{Actor, ModelKind, Operation, Transaction};
use almanac_sync::Origin;
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::AppError;

pub const NAME: &str = "Almanac";
const STICKS_TO: &str = "This workspace: which topics it has, what each one sticks to, \
starting new ones and changing them.";
const STARTERS: &[&str] = &[
    "I keep forgetting to water my plants",
    "I'm researching a pitch for a new role",
    "What can a topic keep?",
];

const HAS_ONE: &str =
    "SELECT EXISTS (SELECT 1 FROM topics WHERE workspace_id = $1 AND kind = 'almanac')";
/// Workspaces without their own topic, each with a member to make it as:
/// an admin when there is one.
const WITHOUT_ONE: &str = "SELECT w.id, m.user_id FROM workspaces w \
     CROSS JOIN LATERAL (SELECT user_id FROM workspace_members \
       WHERE workspace_id = w.id ORDER BY role = 'admin' DESC, created_at LIMIT 1) m \
     WHERE NOT EXISTS (SELECT 1 FROM topics t WHERE t.workspace_id = w.id AND t.kind = 'almanac')";

/// Makes the workspace's own topic if it has none, as `actor`, through the
/// sync engine so every client gets it.
pub async fn ensure(db: &mut PgConnection, actor: Actor) -> Result<(), AppError> {
    let has_one: bool = sqlx::query_scalar(HAS_ONE)
        .bind(actor.workspace_id)
        .fetch_one(&mut *db)
        .await?;
    if has_one {
        return Ok(());
    }
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations: vec![Operation::Create {
            model: ModelKind::Topic,
            id: Uuid::now_v7(),
            data: [
                ("name", json!(NAME)),
                ("sticks_to", json!(STICKS_TO)),
                ("icon", json!("library")),
                ("starters", json!(STARTERS)),
                ("position", json!(0)),
                ("kind", json!("almanac")),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        }],
    };
    almanac_sync::apply_in(db, actor, &transaction, Origin::Server).await?;
    Ok(())
}

/// [`ensure`] in a transaction of its own. Two clients starting at once may
/// both try; the one that loses finds it made.
pub async fn ensure_alone(pool: &PgPool, actor: Actor) {
    let made = async {
        let mut db = pool.begin().await?;
        ensure(&mut db, actor).await?;
        db.commit().await?;
        Ok::<(), AppError>(())
    };
    if let Err(error) = made.await {
        tracing::warn!(%error, workspace = %actor.workspace_id, "couldn't make the workspace's own topic");
    }
}

/// Gives every workspace with a member its own topic, for workspaces from
/// before there was one.
pub async fn ensure_everywhere(pool: &PgPool) {
    let without: Vec<(Uuid, Uuid)> = match sqlx::query_as(WITHOUT_ONE).fetch_all(pool).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "couldn't look for workspaces without their own topic");
            return;
        }
    };
    for (workspace_id, user_id) in without {
        ensure_alone(
            pool,
            Actor {
                user_id,
                workspace_id,
            },
        )
        .await;
    }
}
