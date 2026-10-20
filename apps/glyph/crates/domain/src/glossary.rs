use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, invalid, optional};

/// A preferred term: "from English to Swedish, always translate email as
/// e-post". The model is told the terms that appear in a text, and its
/// translation is checked for them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GlossaryEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source_lang: String,
    pub target_lang: String,
    pub source_term: String,
    pub target_term: String,
    /// Other accepted forms of the target term, such as Swedish `e-posten`
    /// for `e-post`. Empty: only the term itself.
    pub target_variants: Vec<String>,
    /// For reviewers only; never sent to the model.
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct GlossaryEntryCreate {
    pub source_lang: String,
    pub target_lang: String,
    pub source_term: String,
    pub target_term: String,
    pub target_variants: Option<Vec<String>>,
    pub note: Option<String>,
}

/// The languages of an entry stay; the terms and note can change.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct GlossaryEntryPatch {
    pub source_term: Option<String>,
    pub target_term: Option<String>,
    pub target_variants: Option<Vec<String>>,
    pub note: Option<String>,
}

fn variants(values: &[String]) -> Result<(), ValidationError> {
    if values.len() > 50 {
        return invalid("a term can have at most 50 variants");
    }
    values
        .iter()
        .try_for_each(|v| rules::text(v, "variant", rules::TERM))
}

impl Validate for GlossaryEntryCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.source_lang)?;
        rules::language_code(&self.target_lang)?;
        if self.source_lang == self.target_lang {
            return invalid("a glossary entry goes from one language to another");
        }
        rules::text(&self.source_term, "source term", rules::TERM)?;
        rules::text(&self.target_term, "target term", rules::TERM)?;
        optional(&self.target_variants, |v| variants(v))?;
        optional(&self.note, |n| rules::long_text(n, "note", rules::NOTE))
    }
}

impl Validate for GlossaryEntryPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.source_term, |t| {
            rules::text(t, "source term", rules::TERM)
        })?;
        optional(&self.target_term, |t| {
            rules::text(t, "target term", rules::TERM)
        })?;
        optional(&self.target_variants, |v| variants(v))?;
        optional(&self.note, |n| rules::long_text(n, "note", rules::NOTE))
    }
}
