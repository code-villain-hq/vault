//! Who is in which team. A guest sees only their teams' work; admins and
//! members see every team, and theirs are the ones in their sidebar.
//!
//! Only the server writes these, each change through [`record_change`], and
//! each one makes the guests it concerns start over from a fresh snapshot:
//! the person whose teams changed, and the guests who share a team with them
//! (they see the people in their teams).
//!
//! A team linked to a group of the Code Villain account (`teams.group_name`)
//! takes its people from the group instead ([`follow_groups`]): nobody joins
//! or leaves it by hand.

use forge_domain::{Actor, ModelKind, SyncAction};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{SyncError, lock_workspace, record_change};

/// Puts someone in exactly these teams, of those not linked to a group: the
/// linked ones stay as their groups have them.
pub async fn set_teams(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    teams: &[Uuid],
) -> Result<(), SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let linked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM teams WHERE workspace_id = $1 AND group_name IS NOT NULL",
    )
    .bind(actor.workspace_id)
    .fetch_all(&mut *db)
    .await?;
    let current = teams_of(db, actor.workspace_id, user_id).await?;
    let mut wanted: Vec<Uuid> = teams
        .iter()
        .filter(|t| !linked.contains(t))
        .copied()
        .collect();
    wanted.extend(
        current
            .iter()
            .map(|(_, t)| *t)
            .filter(|t| linked.contains(t)),
    );
    put_in(db, actor, user_id, &wanted).await
}

/// Puts someone in exactly these teams, linked or not.
async fn put_in(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    teams: &[Uuid],
) -> Result<(), SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let known: i64 =
        sqlx::query_scalar("SELECT count(*) FROM teams WHERE workspace_id = $1 AND id = ANY($2)")
            .bind(actor.workspace_id)
            .bind(teams)
            .fetch_one(&mut *db)
            .await?;
    let mut wanted = teams.to_vec();
    wanted.sort();
    wanted.dedup();
    if known != wanted.len() as i64 {
        return Err(SyncError::Invalid("team not found".into()));
    }
    let current = teams_of(db, actor.workspace_id, user_id).await?;
    let mut touched = Vec::new();
    for (id, team) in &current {
        if !wanted.contains(team) {
            remove(db, actor, *id).await?;
            touched.push(*team);
        }
    }
    for team in wanted {
        if !current.iter().any(|(_, t)| *t == team) {
            add(db, actor, team, user_id).await?;
            touched.push(team);
        }
    }
    if !touched.is_empty() {
        access_changed(db, actor.workspace_id, &[user_id], &touched).await?;
    }
    Ok(())
}

/// Puts a new admin or member in every team not linked to a group.
pub async fn join_every_team(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
) -> Result<(), SyncError> {
    let teams: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM teams WHERE workspace_id = $1")
        .bind(actor.workspace_id)
        .fetch_all(&mut *db)
        .await?;
    set_teams(db, actor, user_id, &teams).await
}

/// Puts every admin and member in a new team. Guests join it only when an
/// admin adds them. A team linked to a group takes nobody else.
pub async fn add_everyone(
    db: &mut PgConnection,
    actor: Actor,
    team_id: Uuid,
) -> Result<(), SyncError> {
    let people: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM workspace_members WHERE workspace_id = $1 AND role <> 'guest' \
         AND user_id NOT IN (SELECT user_id FROM team_members WHERE team_id = $2) \
         AND NOT EXISTS (SELECT 1 FROM teams WHERE id = $2 AND group_name IS NOT NULL)",
    )
    .bind(actor.workspace_id)
    .bind(team_id)
    .fetch_all(&mut *db)
    .await?;
    for person in &people {
        add(db, actor, team_id, *person).await?;
    }
    if !people.is_empty() {
        access_changed(db, actor.workspace_id, &people, &[team_id]).await?;
    }
    Ok(())
}

/// Takes someone out of all their teams, before they leave the workspace.
pub async fn leave_every_team(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
) -> Result<(), SyncError> {
    put_in(db, actor, user_id, &[]).await
}

/// Brings the teams linked to a group in line with it: everyone in the
/// workspace who is in the group (or its `-admins` or `-guests`), and nobody
/// else. Only `teams`, and only
/// for `people`, when given. A team that's no longer linked keeps its people.
/// Returns the last change it logged.
pub async fn follow_groups(
    db: &mut PgConnection,
    actor: Actor,
    teams: Option<&[Uuid]>,
    people: Option<&[Uuid]>,
) -> Result<Option<i64>, SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let wanted: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT t.id, w.user_id FROM teams t \
         JOIN workspace_members w ON w.workspace_id = t.workspace_id \
         WHERE t.workspace_id = $1 AND t.group_name IS NOT NULL \
         AND ($2::uuid[] IS NULL OR t.id = ANY($2)) AND ($3::uuid[] IS NULL OR w.user_id = ANY($3)) \
         AND EXISTS (SELECT 1 FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id \
                     JOIN scim_users s ON s.user_id = m.user_id AND s.active \
                     WHERE m.user_id = w.user_id \
                     AND g.name IN (t.group_name, t.group_name || '-admins', t.group_name || '-guests'))",
    )
    .bind(actor.workspace_id)
    .bind(teams)
    .bind(people)
    .fetch_all(&mut *db)
    .await?;
    let current: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT m.id, m.team_id, m.user_id FROM team_members m JOIN teams t ON t.id = m.team_id \
         WHERE m.workspace_id = $1 AND t.group_name IS NOT NULL \
         AND ($2::uuid[] IS NULL OR t.id = ANY($2)) AND ($3::uuid[] IS NULL OR m.user_id = ANY($3))",
    )
    .bind(actor.workspace_id)
    .bind(teams)
    .bind(people)
    .fetch_all(&mut *db)
    .await?;
    let mut last = None;
    let mut touched_people = Vec::new();
    let mut touched_teams = Vec::new();
    for (id, team, person) in &current {
        if !wanted.contains(&(*team, *person)) {
            last = Some(remove(db, actor, *id).await?);
            touched_people.push(*person);
            touched_teams.push(*team);
        }
    }
    for (team, person) in &wanted {
        if !current.iter().any(|(_, t, p)| t == team && p == person) {
            last = Some(add(db, actor, *team, *person).await?);
            touched_people.push(*person);
            touched_teams.push(*team);
        }
    }
    if !touched_people.is_empty() {
        access_changed(db, actor.workspace_id, &touched_people, &touched_teams).await?;
    }
    Ok(last)
}

/// Someone's role changed. Guests see the admins, so every guest starts
/// over, and so does the person, who may have become or stopped being one.
pub async fn role_changed(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND (user_id = $2 OR role = 'guest')",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *db)
    .await?;
    Ok(())
}

/// The ids and teams of someone's team memberships.
async fn teams_of(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<(Uuid, Uuid)>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id, team_id FROM team_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_all(&mut *db)
    .await?)
}

async fn add(
    db: &mut PgConnection,
    actor: Actor,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<i64, SyncError> {
    let id = Uuid::now_v7();
    let record: Value = sqlx::query_scalar(
        "INSERT INTO team_members (id, workspace_id, team_id, user_id) VALUES ($1, $2, $3, $4) \
         RETURNING to_jsonb(team_members.*)",
    )
    .bind(id)
    .bind(actor.workspace_id)
    .bind(team_id)
    .bind(user_id)
    .fetch_one(&mut *db)
    .await?;
    record_change(
        db,
        actor,
        ModelKind::TeamMember,
        id,
        SyncAction::Insert,
        Some(record),
    )
    .await
}

async fn remove(db: &mut PgConnection, actor: Actor, id: Uuid) -> Result<i64, SyncError> {
    let sync_id = record_change(
        db,
        actor,
        ModelKind::TeamMember,
        id,
        SyncAction::Delete,
        None,
    )
    .await?;
    sqlx::query("DELETE FROM team_members WHERE id = $1")
        .bind(id)
        .execute(&mut *db)
        .await?;
    Ok(sync_id)
}

/// Makes the people whose teams changed, and the guests in those teams,
/// start over.
async fn access_changed(
    db: &mut PgConnection,
    workspace_id: Uuid,
    people: &[Uuid],
    teams: &[Uuid],
) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND (user_id = ANY($2) OR (role = 'guest' AND user_id IN \
         (SELECT user_id FROM team_members WHERE team_id = ANY($3))))",
    )
    .bind(workspace_id)
    .bind(people)
    .bind(teams)
    .execute(&mut *db)
    .await?;
    Ok(())
}
