use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules;

/// A glossary term a model's text left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GlossaryViolation {
    pub source_term: String,
    pub target_term: String,
}

/// A model's proposed text for one key, waiting for review. Accepting it
/// writes the translation and deletes the suggestion, in one transaction;
/// rejecting it deletes it. Only the server makes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Suggestion {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// The run that made it, while that run is kept.
    pub run_id: Option<Uuid>,
    pub lang: String,
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    /// The name of the model that proposed it.
    pub model: String,
    /// The source text it was made from.
    pub source_text: String,
    pub glossary_violations: Vec<GlossaryViolation>,
    /// Whether `{{ }}` placeholders from the source went missing.
    pub placeholders_broken: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Suggestion {
    /// Whether someone should read it before accepting: a glossary term or a
    /// placeholder went missing.
    pub fn needs_a_look(&self) -> bool {
        self.placeholders_broken || !self.glossary_violations.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct SuggestionCreate {
    pub run_id: Option<Uuid>,
    pub lang: String,
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub model: String,
    pub source_text: Option<String>,
    pub glossary_violations: Option<Vec<GlossaryViolation>>,
    pub placeholders_broken: Option<bool>,
}

/// A newer proposal for the same key replaces the text in place.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct SuggestionPatch {
    pub run_id: Option<Option<Uuid>>,
    pub value: Option<String>,
    pub confidence: Option<f64>,
    pub model: Option<String>,
    pub source_text: Option<String>,
    pub glossary_violations: Option<Vec<GlossaryViolation>>,
    pub placeholders_broken: Option<bool>,
}

impl Validate for SuggestionCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.lang)?;
        rules::namespace(&self.namespace)?;
        rules::key(&self.key)?;
        rules::long_text(&self.value, "suggestion", rules::VALUE)?;
        rules::unit(self.confidence, "confidence")?;
        rules::text(&self.model, "model", rules::NAME)
    }
}

impl Validate for SuggestionPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::optional(&self.value, |v| {
            rules::long_text(v, "suggestion", rules::VALUE)
        })?;
        rules::optional(&self.confidence, |c| rules::unit(*c, "confidence"))?;
        rules::optional(&self.model, |m| rules::text(m, "model", rules::NAME))
    }
}
