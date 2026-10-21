use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, invalid, optional};

/// How to translate in one direction. Each direction has its own, since a
/// model can be better one way than the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LanguagePair {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source_lang: String,
    pub target_lang: String,
    /// A model's name from the server's models file; empty for the first one.
    pub model: String,
    /// From 0 to 2; lower is more literal.
    pub temperature: f64,
    /// Translations the model is less sure of than this need a look.
    pub confidence_threshold: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LanguagePairCreate {
    pub source_lang: String,
    pub target_lang: String,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub confidence_threshold: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LanguagePairPatch {
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub confidence_threshold: Option<f64>,
}

fn temperature(value: &f64) -> Result<(), ValidationError> {
    if (0.0..=2.0).contains(value) {
        Ok(())
    } else {
        invalid("temperature must be between 0 and 2")
    }
}

impl Validate for LanguagePairCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.source_lang)?;
        rules::language_code(&self.target_lang)?;
        if self.source_lang == self.target_lang {
            return invalid("a language pair goes from one language to another");
        }
        optional(&self.model, |m| rules::long_text(m, "model", rules::NAME))?;
        optional(&self.temperature, temperature)?;
        optional(&self.confidence_threshold, |t| {
            rules::unit(*t, "confidence threshold")
        })
    }
}

impl Validate for LanguagePairPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.model, |m| rules::long_text(m, "model", rules::NAME))?;
        optional(&self.temperature, temperature)?;
        optional(&self.confidence_threshold, |t| {
            rules::unit(*t, "confidence threshold")
        })
    }
}
