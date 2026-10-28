//! A server runs for one company, in one workspace: made on the first
//! start, named after the company, and filled by the company's account
//! (`crate::groups`). It's set up once its first person is in.

use anyhow::bail;
use code_villain_scim::{Groups, follow_groups};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::groups::AlmanacDirectory;

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
            let name = almanac_domain::clean_name(company, "company")?;
            sqlx::query(
                "INSERT INTO workspaces (id, name, url_key, set_up) VALUES ($1, $2, $3, false)",
            )
            .bind(id)
            .bind(&name)
            .bind(almanac_domain::url_key_from(&name))
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
    follow_groups::<AlmanacDirectory>(&mut db, workspace, groups).await?;
    db.commit().await?;
    Ok(workspace)
}

/// The workspace, once its first person is in.
pub async fn finish_setting_up(
    db: &mut PgConnection,
    workspace_id: Uuid,
    _user_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE workspaces SET set_up = true WHERE id = $1 AND NOT set_up")
        .bind(workspace_id)
        .execute(&mut *db)
        .await?;
    Ok(())
}
