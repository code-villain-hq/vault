use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub use code_villain_sync::{Actor, ValidationError, Write};

use crate::app::{AppCreate, AppPatch};

/// Every kind of record that clients sync: a workspace's settings. What
/// Raven hears from the apps is read live and never syncs.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
/// See "Adding a synced model" in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    App,
    User,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[ModelKind::App, ModelKind::User];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::App => "apps",
            ModelKind::User => "workspace_users",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::App => "App",
            ModelKind::User => "User",
        }
    }

    pub fn from_name(name: &str) -> Option<ModelKind> {
        ModelKind::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync. Users are
    /// read-only: membership changes go through the account API.
    pub fn client_may(self, _write: Write) -> bool {
        match self {
            ModelKind::App => true,
            ModelKind::User => false,
        }
    }

    /// Records only their owner (`user_id`) syncs, rather than the whole
    /// workspace. Raven has none yet; the engine supports them as Forge's does.
    pub fn private_to_user(self) -> bool {
        false
    }

    /// Checks the fields sent to create a record.
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::App => check::<AppCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::App => check::<AppPatch>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }
}

/// How the sync engine knows Raven's models.
impl code_villain_sync::Model for ModelKind {
    const ALL: &'static [ModelKind] = ModelKind::ALL;

    fn table(self) -> &'static str {
        ModelKind::table(self)
    }

    fn name(self) -> &'static str {
        ModelKind::name(self)
    }

    fn client_may(self, write: Write) -> bool {
        ModelKind::client_may(self, write)
    }

    fn private_to_user(self) -> bool {
        ModelKind::private_to_user(self)
    }

    fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_create(self, data)
    }

    fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_patch(self, data)
    }
}

/// Business rules a create or patch input must satisfy, beyond its types.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}

/// Parses `data` as `T`, which rejects unknown fields and wrong types, then
/// applies `T`'s rules.
fn check<T: DeserializeOwned + Validate>(data: &Map<String, Value>) -> Result<(), ValidationError> {
    let input: T = serde_json::from_value(Value::Object(data.clone()))
        .map_err(|e| ValidationError(e.to_string()))?;
    input.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn names_round_trip() {
        for model in ModelKind::ALL {
            assert_eq!(ModelKind::from_name(model.name()), Some(*model));
        }
    }

    #[test]
    fn an_app_cannot_say_it_is_raven_itself() {
        let ok = map(json!({ "key": "forge", "name": "Forge" }));
        assert!(ModelKind::App.validate_create(&ok).is_ok());
        let self_ = map(json!({ "key": "forge", "name": "Forge", "is_self": true }));
        assert!(ModelKind::App.validate_create(&self_).is_err());
        let rekey = map(json!({ "key": "glyph" }));
        assert!(ModelKind::App.validate_patch(&rekey).is_err());
        let stop = map(json!({ "health_url": null }));
        assert!(ModelKind::App.validate_patch(&stop).is_ok());
    }

    #[test]
    fn users_are_read_only() {
        for write in [Write::Create, Write::Update, Write::Delete] {
            assert!(!ModelKind::User.client_may(write));
        }
    }
}
