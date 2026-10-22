//! A server runs for one company, in one workspace: made on the first
//! start, named after the company, and filled by the company's account
//! (`crate::groups`). Raven itself comes with its first person, who is
//! the first one to stand for a change in the log.

use anyhow::bail;
use code_villain_scim::{Groups, follow_groups};
use raven_domain::Actor;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::apps;
use crate::groups::RavenDirectory;

/// The company's workspace, made if there's none yet, following `groups`.
pub async fn start(pool: &PgPool, company: &str, groups: &Groups) -> anyhow::Result<Uuid> {
    let mut db = pool.begin().await?;
    // One start at a time makes the workspace.
    sqlx::query("LOCK TABLE workspaces IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *db)
        .await?;
    let workspaces: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM workspaces ORDER BY created_at")
            .fetch_all(&mut *db)
            .await?;
    let workspace = match workspaces.as_slice() {
        [] => {
            let id = Uuid::now_v7();
            let name = raven_domain::clean_name(company, "company")?;
            sqlx::query(
                "INSERT INTO workspaces (id, name, url_key, set_up) VALUES ($1, $2, $3, false)",
            )
            .bind(id)
            .bind(&name)
            .bind(raven_domain::url_key_from(&name))
            .execute(&mut *db)
            .await?;
            tracing::info!("made {name}'s workspace");
            id
        }
        [(id, _)] => *id,
        more => bail!(
            "this database has {} workspaces ({}); a server runs one company's",
            more.len(),
            more.iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    follow_groups::<RavenDirectory>(&mut db, workspace, groups).await?;
    db.commit().await?;
    Ok(workspace)
}

/// Raven itself, in the workspace, once its first person is in.
pub async fn finish_setting_up(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<()> {
    let first: bool = sqlx::query_scalar(
        "UPDATE workspaces SET set_up = true WHERE id = $1 AND NOT set_up RETURNING true",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *db)
    .await?
    .unwrap_or(false);
    if first {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        apps::adopt_self(db, actor)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}
