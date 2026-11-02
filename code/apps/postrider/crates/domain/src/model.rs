use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub use code_villain_sync::{Actor, GuestView, ValidationError, Write};

use crate::mail::{CalendarCreate, MailAccountCreate, MailAccountPatch};

/// Every kind of record that clients sync: Postrider's own. Mail and events
/// stay on their servers and never sync.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    MailAccount,
    Calendar,
    User,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[ModelKind::MailAccount, ModelKind::Calendar, ModelKind::User];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::MailAccount => "mail_accounts",
            ModelKind::Calendar => "calendars",
            ModelKind::User => "workspace_users",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::MailAccount => "MailAccount",
            ModelKind::Calendar => "Calendar",
            ModelKind::User => "User",
        }
    }

    pub fn from_name(name: &str) -> Option<ModelKind> {
        ModelKind::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync. Accounts
    /// and the calendar come and go through the server, which checks the
    /// password and keeps it in the keychain; a client may only rename an
    /// account or give it another hue.
    pub fn client_may(self, write: Write) -> bool {
        match self {
            ModelKind::MailAccount => write == Write::Update,
            ModelKind::Calendar | ModelKind::User => false,
        }
    }

    /// Records only their owner (`user_id`) syncs. Postrider has none.
    pub fn private_to_user(self) -> bool {
        false
    }

    /// Postrider has no guests; one would see none of it.
    pub fn guest_view(self) -> GuestView {
        GuestView::Hidden
    }

    /// Checks the fields sent to create a record (only the server does).
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::MailAccount => check::<MailAccountCreate>(data),
            ModelKind::Calendar => check::<CalendarCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::MailAccount => check::<MailAccountPatch>(data),
            ModelKind::Calendar | ModelKind::User => Err(ValidationError(format!(
                "{} records are read-only",
                self.name()
            ))),
        }
    }
}

/// How the sync engine knows Postrider's models.
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

    fn guest_view(self) -> GuestView {
        ModelKind::guest_view(self)
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
    fn a_client_only_renames_an_account_or_changes_its_hue() {
        assert!(!ModelKind::MailAccount.client_may(Write::Create));
        assert!(!ModelKind::MailAccount.client_may(Write::Delete));
        assert!(ModelKind::MailAccount.client_may(Write::Update));
        let ok = map(json!({ "name": "Lazarski", "hue": 2 }));
        assert!(ModelKind::MailAccount.validate_patch(&ok).is_ok());
        let host = map(json!({ "host": "imap.example.com" }));
        assert!(ModelKind::MailAccount.validate_patch(&host).is_err());
        let hue = map(json!({ "hue": 7 }));
        assert!(ModelKind::MailAccount.validate_patch(&hue).is_err());
    }

    #[test]
    fn the_calendar_and_users_are_read_only() {
        for write in [Write::Create, Write::Update, Write::Delete] {
            assert!(!ModelKind::Calendar.client_may(write));
            assert!(!ModelKind::User.client_may(write));
        }
    }
}
