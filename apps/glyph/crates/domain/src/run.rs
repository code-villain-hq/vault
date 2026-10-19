use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, invalid};

/// What a run does with the model's texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum RunKind {
    /// Proposes them as suggestions, for review.
    Suggest,
    /// Writes them as translations straight away.
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum RunStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn finished(self) -> bool {
        matches!(self, RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled)
    }
}

/// The server translating a language's missing keys with a model. Only the
/// server writes runs; clients follow their progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TranslationRun {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: RunKind,
    pub source_lang: String,
    pub target_lang: String,
    /// Only keys in this namespace; `None` for every namespace.
    pub namespace: Option<String>,
    pub status: RunStatus,
    /// Keys to translate.
    pub total: i32,
    /// Keys translated so far.
    pub done: i32,
    /// Keys the model failed on.
    pub failed: i32,
    /// Why the run stopped, when it failed.
    pub error: String,
    /// The name of the model it uses.
    pub model: String,
    pub started_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TranslationRunCreate {
    pub kind: RunKind,
    pub source_lang: String,
    pub target_lang: String,
    pub namespace: Option<String>,
    pub status: Option<RunStatus>,
    pub total: Option<i32>,
    pub model: Option<String>,
    pub started_by: Uuid,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TranslationRunPatch {
    pub status: Option<RunStatus>,
    pub total: Option<i32>,
    pub done: Option<i32>,
    pub failed: Option<i32>,
    pub error: Option<String>,
    pub model: Option<String>,
}

fn count(value: &i32, what: &str) -> Result<(), ValidationError> {
    if *value < 0 {
        return invalid(format!("{what} can't be negative"));
    }
    Ok(())
}

impl Validate for TranslationRunCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.source_lang)?;
        rules::language_code(&self.target_lang)?;
        if self.source_lang == self.target_lang {
            return invalid("A run translates between two different languages.");
        }
        rules::optional(&self.namespace, |n| rules::namespace(n))?;
        rules::optional(&self.total, |t| count(t, "total"))?;
        rules::optional(&self.model, |m| rules::text(m, "model", rules::NAME))
    }
}

impl Validate for TranslationRunPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::optional(&self.total, |t| count(t, "total"))?;
        rules::optional(&self.done, |d| count(d, "done"))?;
        rules::optional(&self.failed, |f| count(f, "failed"))?;
        rules::optional(&self.error, |e| rules::long_text(e, "error", rules::NOTE))?;
        rules::optional(&self.model, |m| rules::text(m, "model", rules::NAME))
    }
}

/// One key, in every language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct KeyRef {
    pub namespace: String,
    pub key: String,
}

/// Asks the server to translate keys from one language into another.
/// Without `keys`, it takes the keys the target language is missing (and,
/// for suggestions, that have none waiting), up to `limit`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct StartRunRequest {
    pub kind: RunKind,
    pub source_lang: String,
    pub target_lang: String,
    pub namespace: Option<String>,
    /// Just these keys, missing or not.
    pub keys: Option<Vec<KeyRef>>,
    /// At most this many keys: 50 for suggestions unless given, so a review
    /// stays a size people finish.
    pub limit: Option<u32>,
}

impl StartRunRequest {
    /// The most keys a run takes when the request names them.
    pub const MAX_KEYS: usize = 500;
    /// The most keys any run takes.
    pub const MAX_LIMIT: u32 = 10_000;
    /// How many keys a suggest run takes by default.
    pub const SUGGEST_LIMIT: u32 = 50;

    /// How many keys this run may take.
    pub fn limit(&self) -> u32 {
        let default = match self.kind {
            RunKind::Suggest => Self::SUGGEST_LIMIT,
            RunKind::Fill => Self::MAX_LIMIT,
        };
        self.limit.unwrap_or(default).clamp(1, Self::MAX_LIMIT)
    }
}

impl Validate for StartRunRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.source_lang)?;
        rules::language_code(&self.target_lang)?;
        if self.source_lang == self.target_lang {
            return invalid("Pick two different languages.");
        }
        rules::optional(&self.namespace, |n| rules::namespace(n))?;
        if let Some(keys) = &self.keys {
            if keys.is_empty() || keys.len() > Self::MAX_KEYS {
                return invalid(format!(
                    "Name between 1 and {} keys, or none for every missing key.",
                    Self::MAX_KEYS
                ));
            }
            for k in keys {
                rules::namespace(&k.namespace)?;
                rules::key(&k.key)?;
            }
        }
        Ok(())
    }
}
