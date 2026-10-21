use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Validate, ValidationError};
use crate::rules;

/// A translation model the server knows, from its models file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelInfo {
    pub name: String,
    pub endpoint: String,
    /// Whether it answers.
    pub reachable: bool,
    /// Whether the server can start it itself.
    pub can_start: bool,
    /// The server's own process for it, when it started one.
    pub process: Option<ModelProcess>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelProcess {
    pub pid: Option<u32>,
    /// Seconds since 1970.
    #[ts(type = "number")]
    pub started_at: u64,
    /// How it ended, once it has.
    pub exited: Option<String>,
    /// Its last lines of output.
    pub recent_output: Vec<String>,
}

/// A model added in Settings, on a Glyph whose models file the app may
/// write (the desktop app's own). It comes from one place: a GGUF file, a
/// download from Hugging Face, or a server someone runs (`endpoint`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, default)]
#[ts(export)]
pub struct ModelCreate {
    pub name: String,
    pub model_path: String,
    pub hf_repo: String,
    pub hf_file: String,
    pub endpoint: String,
}

impl Validate for ModelCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "name", rules::NAME)?;
        let file = !self.model_path.trim().is_empty();
        let download = !self.hf_repo.trim().is_empty() || !self.hf_file.trim().is_empty();
        let server = !self.endpoint.trim().is_empty();
        match (file, download, server) {
            (true, false, false) => {
                if !self.model_path.trim().ends_with(".gguf") {
                    return Err(ValidationError("Choose a .gguf file.".into()));
                }
            }
            (false, true, false) => {
                rules::text(&self.hf_repo, "repository", rules::NAME * 2)?;
                rules::text(&self.hf_file, "file", rules::NAME * 2)?;
            }
            (false, false, true) => {
                let endpoint = self.endpoint.trim();
                if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
                    return Err(ValidationError(
                        "The address starts with http:// or https://.".into(),
                    ));
                }
            }
            _ => {
                return Err(ValidationError(
                    "Give a file, a download or a server, one of them.".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Asks a pair's model for the grammatical forms of a glossary term.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct VariantsRequest {
    pub source_lang: String,
    pub target_lang: String,
    pub source_term: String,
    pub target_term: String,
}

impl Validate for VariantsRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::language_code(&self.source_lang)?;
        rules::language_code(&self.target_lang)?;
        rules::text(&self.source_term, "source term", rules::TERM)?;
        rules::text(&self.target_term, "target term", rules::TERM)
    }
}

/// The forms, without the term itself; often none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Variants {
    pub variants: Vec<String>,
}
