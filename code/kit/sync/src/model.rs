use std::fmt::Debug;
use std::hash::Hash;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use uuid::Uuid;

/// Who is making a change, and in which workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
}

/// What a guest syncs of a kind of record. Members and admins sync the whole
/// workspace; a guest only what they've been added to, such as their teams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestView {
    /// Every record, as for members: the source language, the people.
    All,
    /// The records this SQL condition holds for. It reads the record as `t`
    /// and the guest's user id as `$viewer`, for example
    /// `t.team_id IN (SELECT team_id FROM team_members WHERE user_id = $viewer)`.
    Where(&'static str),
    /// None of them: they're the workspace's, not a part a guest is in.
    Hidden,
}

/// The three kinds of write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Write {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValidationError(pub String);

/// An app's kinds of synced record, usually an enum with one variant per model.
pub trait Model:
    Debug + Copy + Eq + Ord + Hash + Serialize + DeserializeOwned + Send + Sync + 'static
{
    const ALL: &'static [Self];

    /// The table (or view) holding this kind of record. It has `id`,
    /// `workspace_id`, `created_at` and `updated_at` columns.
    fn table(self) -> &'static str;

    /// The name used on the wire and in `sync_log.model`.
    fn name(self) -> &'static str;

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync.
    fn client_may(self, write: Write) -> bool;

    /// Records only their owner (`user_id`) syncs, rather than the whole workspace.
    fn private_to_user(self) -> bool;

    /// What a guest syncs of this kind; see [`Rules::guest_access`](crate::Rules::guest_access).
    /// Required, so a new model can't reach guests by being forgotten.
    fn guest_view(self) -> GuestView;

    /// Checks the fields sent to create a record.
    fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError>;

    /// Checks the fields sent to update a record.
    fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError>;
}
