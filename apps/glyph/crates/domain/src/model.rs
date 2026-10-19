use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;
use uuid::Uuid;

use crate::glossary::{GlossaryEntryCreate, GlossaryEntryPatch};
use crate::language::{LanguageCreate, LanguagePatch};
use crate::pair::{LanguagePairCreate, LanguagePairPatch};
use crate::translation::{TranslationCreate, TranslationPatch};

/// Who is making a change, and in which workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
}

/// Every kind of record that clients sync.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
/// See "Adding a synced model" in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    Language,
    Translation,
    GlossaryEntry,
    LanguagePair,
    User,
}

/// The three kinds of write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Write {
    Create,
    Update,
    Delete,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[
        ModelKind::Language,
        ModelKind::Translation,
        ModelKind::GlossaryEntry,
        ModelKind::LanguagePair,
        ModelKind::User,
    ];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::Language => "languages",
            ModelKind::Translation => "translations",
            ModelKind::GlossaryEntry => "glossary_entries",
            ModelKind::LanguagePair => "language_pairs",
            ModelKind::User => "workspace_users",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::Language => "Language",
            ModelKind::Translation => "Translation",
            ModelKind::GlossaryEntry => "GlossaryEntry",
            ModelKind::LanguagePair => "LanguagePair",
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
    /// workspace. Glyph has none yet; the engine supports them as Forge's does.
    pub fn private_to_user(self) -> bool {
        false
    }

    /// Checks the fields sent to create a record.
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Language => check::<LanguageCreate>(data),
            ModelKind::Translation => check::<TranslationCreate>(data),
            ModelKind::GlossaryEntry => check::<GlossaryEntryCreate>(data),
            ModelKind::LanguagePair => check::<LanguagePairCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Language => check::<LanguagePatch>(data),
            ModelKind::Translation => check::<TranslationPatch>(data),
            ModelKind::GlossaryEntry => check::<GlossaryEntryPatch>(data),
            ModelKind::LanguagePair => check::<LanguagePairPatch>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }
}

/// Business rules a create or patch input must satisfy, beyond its types.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValidationError(pub String);

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
    fn unknown_fields_and_bad_values_are_refused() {
        let ok = map(json!({ "lang": "sv", "namespace": "common", "key": "hi", "value": "Hej" }));
        assert!(ModelKind::Translation.validate_create(&ok).is_ok());
        let extra = map(
            json!({ "lang": "sv", "namespace": "common", "key": "hi", "value": "Hej", "id": "x" }),
        );
        assert!(ModelKind::Translation.validate_create(&extra).is_err());
        let sure = map(json!({ "confidence": 1.5 }));
        assert!(ModelKind::Translation.validate_patch(&sure).is_err());
        let cleared = map(json!({ "confidence": null }));
        assert!(ModelKind::Translation.validate_patch(&cleared).is_ok());
        let same = map(json!({ "source_lang": "en", "target_lang": "en" }));
        assert!(ModelKind::LanguagePair.validate_create(&same).is_err());
    }
}
