use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules;

/// A language the workspace translates into, or the source language keys are
/// written in first. Clients show its name from the code (`Intl.DisplayNames`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Language {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// `en`, `sv`, `pt-BR`. It can't change: translations refer to it.
    pub code: String,
    /// One language per workspace is the source.
    pub is_source: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LanguageCreate {
    pub code: String,
    pub is_source: Option<bool>,
}

/// To make another language the source, clear `is_source` on the old one
/// first, in the same transaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LanguagePatch {
    pub is_source: Option<bool>,
}

impl Validate for LanguageCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.code)
    }
}

impl Validate for LanguagePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}
