use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub use code_villain_sync::{Actor, ValidationError, Write};

use crate::agent_model::{AgentModelCreate, AgentModelPatch};
use crate::connection::{ConnectionCreate, ConnectionPatch};
use crate::log_source::{LogSourceCreate, LogSourcePatch};
use crate::note::{NoteCreate, NotePatch};
use crate::repository::{RepositoryCreate, RepositoryPatch};

/// Every kind of record that clients sync: a workspace's shared setup.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
/// See "Adding a synced model" in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    Repository,
    Connection,
    LogSource,
    AgentModel,
    Note,
    User,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[
        ModelKind::Repository,
        ModelKind::Connection,
        ModelKind::LogSource,
        ModelKind::AgentModel,
        ModelKind::Note,
        ModelKind::User,
    ];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::Repository => "repositories",
            ModelKind::Connection => "connections",
            ModelKind::LogSource => "log_sources",
            ModelKind::AgentModel => "agent_models",
            ModelKind::Note => "notes",
            ModelKind::User => "workspace_users",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::Repository => "Repository",
            ModelKind::Connection => "Connection",
            ModelKind::LogSource => "LogSource",
            ModelKind::AgentModel => "AgentModel",
            ModelKind::Note => "Note",
            ModelKind::User => "User",
        }
    }

    pub fn from_name(name: &str) -> Option<ModelKind> {
        ModelKind::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync. Users are
    /// read-only: membership changes go through the account API.
    pub fn client_may(self, _write: Write) -> bool {
        self != ModelKind::User
    }

    /// Records only their owner (`user_id`) syncs, rather than the whole
    /// workspace. Layer has none: what is one person's own stays on their Mac.
    pub fn private_to_user(self) -> bool {
        false
    }

    /// Checks the fields sent to create a record.
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Repository => check::<RepositoryCreate>(data),
            ModelKind::Connection => check::<ConnectionCreate>(data),
            ModelKind::LogSource => check::<LogSourceCreate>(data),
            ModelKind::AgentModel => check::<AgentModelCreate>(data),
            ModelKind::Note => check::<NoteCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Repository => check::<RepositoryPatch>(data),
            ModelKind::Connection => check::<ConnectionPatch>(data),
            ModelKind::LogSource => check::<LogSourcePatch>(data),
            ModelKind::AgentModel => check::<AgentModelPatch>(data),
            ModelKind::Note => check::<NotePatch>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }
}

/// How the sync engine knows Layer's models.
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
pub(crate) fn check<T: DeserializeOwned + Validate>(
    data: &Map<String, Value>,
) -> Result<(), ValidationError> {
    let input: T = parse(Value::Object(data.clone()))?;
    input.validate()
}

pub(crate) fn parse<T: DeserializeOwned>(value: Value) -> Result<T, ValidationError> {
    serde_json::from_value(value).map_err(|e| ValidationError(e.to_string()))
}

pub(crate) fn invalid(message: impl Into<String>) -> Result<(), ValidationError> {
    Err(ValidationError(message.into()))
}

/// Text that has to say something, in at most `max` characters.
pub(crate) fn text(value: &str, what: &str, max: usize) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid(format!("{what} can't be empty."));
    }
    let n = value.chars().count();
    if n > max {
        return invalid(format!("{what} is at most {max} characters; this is {n}."));
    }
    Ok(())
}

/// An http or https address.
pub(crate) fn web_address(value: &str, what: &str) -> Result<(), ValidationError> {
    let valid = value.len() <= 500
        && !value.contains(char::is_whitespace)
        && value
            .strip_prefix("https://")
            .or_else(|| value.strip_prefix("http://"))
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'));
    if valid {
        Ok(())
    } else {
        invalid(format!("{what} starts with https:// or http://."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for model in ModelKind::ALL {
            assert_eq!(ModelKind::from_name(model.name()), Some(*model));
        }
    }

    #[test]
    fn users_are_read_only() {
        for write in [Write::Create, Write::Update, Write::Delete] {
            assert!(!ModelKind::User.client_may(write));
        }
    }

    #[test]
    fn web_addresses_are_http() {
        assert!(web_address("https://acme.atlassian.net", "A site").is_ok());
        for bad in [
            "",
            "acme.atlassian.net",
            "ftp://x",
            "https://",
            "https:///x",
            "https://a b",
        ] {
            assert!(web_address(bad, "A site").is_err(), "{bad}");
        }
    }
}
