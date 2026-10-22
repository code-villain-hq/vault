//! What a guest syncs: the records each model's [`GuestView`] lets through,
//! and the epoch that makes their client start over when that changes.

use std::collections::{BTreeMap, HashSet};

use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::{Actor, GuestView, Model, SyncError};

/// The epoch a viewer's copy belongs to. Members share the workspace's; a
/// guest's also depends on their access version, so a new version, or
/// becoming or ceasing to be a guest, sends their client a resync.
pub fn served_epoch(epoch: Uuid, guest_access: Option<i64>) -> Uuid {
    match guest_access {
        None => epoch,
        // Any mix that differs for each version will do.
        Some(version) => Uuid::from_u128(
            epoch.as_u128()
                ^ (u128::from(version as u64) + 1)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835),
        ),
    }
}

/// A [`GuestView::Where`] condition with the guest's user id as `$2`, the
/// parameter every scoped query binds it to.
pub(crate) fn condition(sql: &str) -> String {
    format!("({})", sql.replace("$viewer", "$2"))
}

/// Which of `ids` (records of `model`) the guest `actor` may see.
pub(crate) async fn guest_visible<M: Model>(
    db: &mut PgConnection,
    actor: Actor,
    model: M,
    ids: &[Uuid],
) -> Result<HashSet<Uuid>, SyncError> {
    match model.guest_view() {
        GuestView::All => Ok(ids.iter().copied().collect()),
        GuestView::Hidden => Ok(HashSet::new()),
        GuestView::Where(sql) => {
            let sql = format!(
                "SELECT t.id FROM {} AS t WHERE t.workspace_id = $1 AND t.id = ANY($3) AND {}",
                model.table(),
                condition(sql)
            );
            // Safe: the table name and the condition are constants from the app's `Model`.
            let visible: Vec<Uuid> = sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(actor.workspace_id)
                .bind(actor.user_id)
                .bind(ids)
                .fetch_all(&mut *db)
                .await?;
            Ok(visible.into_iter().collect())
        }
    }
}

/// Whether the guest `actor` may see one record as it is now.
pub(crate) async fn guest_sees<M: Model>(
    db: &mut PgConnection,
    actor: Actor,
    model: M,
    id: Uuid,
) -> Result<bool, SyncError> {
    Ok(guest_visible(db, actor, model, &[id]).await?.contains(&id))
}

/// Of the records named by `(model, id)`, the ones the guest may see now.
pub(crate) async fn guest_filter<M: Model>(
    db: &mut PgConnection,
    actor: Actor,
    records: impl IntoIterator<Item = (M, Uuid)>,
) -> Result<HashSet<(M, Uuid)>, SyncError> {
    let mut by_model: BTreeMap<M, Vec<Uuid>> = BTreeMap::new();
    for (model, id) in records {
        by_model.entry(model).or_default().push(id);
    }
    let mut visible = HashSet::new();
    for (model, ids) in by_model {
        for id in guest_visible(db, actor, model, &ids).await? {
            visible.insert((model, id));
        }
    }
    Ok(visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_keep_the_workspace_epoch_and_each_guest_version_gets_its_own() {
        let epoch = Uuid::now_v7();
        assert_eq!(served_epoch(epoch, None), epoch);
        let first = served_epoch(epoch, Some(0));
        let second = served_epoch(epoch, Some(1));
        assert_ne!(first, epoch);
        assert_ne!(first, second);
        assert_eq!(served_epoch(epoch, Some(1)), second);
    }

    #[test]
    fn the_condition_binds_the_viewer() {
        assert_eq!(
            condition("t.team_id IN (SELECT team_id FROM team_members WHERE user_id = $viewer)"),
            "(t.team_id IN (SELECT team_id FROM team_members WHERE user_id = $2))"
        );
    }
}
