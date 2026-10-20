use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, optional};

/// Who wrote a translation by hand, as opposed to a model's name.
pub const HUMAN: &str = "human";

/// The default namespace, as in i18next and locize.
pub const DEFAULT_NAMESPACE: &str = "translation";

/// One key's text in one language. A key is the same across languages: the
/// pair (namespace, key) names it, and each language has its own record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Translation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub lang: String,
    pub namespace: String,
    pub key: String,
    pub value: String,
    /// How sure the model was, from 0 to 1; `None` when a person wrote it.
    pub confidence: Option<f64>,
    /// `human`, or the name of the model that translated it.
    pub translated_by: String,
    /// A note for translators about where the text appears.
    pub context: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TranslationCreate {
    pub lang: String,
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub confidence: Option<f64>,
    pub translated_by: Option<String>,
    pub context: Option<String>,
}

/// A key moves to another namespace or name in every language at once, so
/// both are editable here; the server's rename does that for a namespace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TranslationPatch {
    pub namespace: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub confidence: Option<Option<f64>>,
    pub translated_by: Option<String>,
    pub context: Option<String>,
}

fn translated_by(value: &str) -> Result<(), ValidationError> {
    rules::text(value, "translated_by", rules::NAME)
}

fn confidence(value: &f64) -> Result<(), ValidationError> {
    rules::unit(*value, "confidence")
}

impl Validate for TranslationCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.lang)?;
        rules::namespace(&self.namespace)?;
        rules::key(&self.key)?;
        rules::long_text(&self.value, "translation", rules::VALUE)?;
        optional(&self.confidence, confidence)?;
        optional(&self.translated_by, |t| translated_by(t))?;
        optional(&self.context, |c| {
            rules::long_text(c, "context", rules::NOTE)
        })
    }
}

impl Validate for TranslationPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.namespace, |n| rules::namespace(n))?;
        optional(&self.key, |k| rules::key(k))?;
        optional(&self.value, |v| {
            rules::long_text(v, "translation", rules::VALUE)
        })?;
        if let Some(Some(c)) = &self.confidence {
            confidence(c)?;
        }
        optional(&self.translated_by, |t| translated_by(t))?;
        optional(&self.context, |c| {
            rules::long_text(c, "context", rules::NOTE)
        })
    }
}
