use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError, invalid, text};

/// The models an agent CLI offered as its last conversation opened, on
/// anyone's Mac: the choices for that agent's model everywhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentModel {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// The agent, as the app's catalogue names it: `claude`. Unique in the
    /// workspace.
    pub agent_id: String,
    pub models: Vec<ModelChoice>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ModelChoice {
    pub value: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AgentModelCreate {
    pub agent_id: String,
    pub models: Vec<ModelChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AgentModelPatch {
    pub models: Vec<ModelChoice>,
}

impl Validate for AgentModelCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        let valid = (1..=40).contains(&self.agent_id.len())
            && self
                .agent_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return invalid("An agent id is letters, digits and dashes, like claude.");
        }
        models(&self.models)
    }
}

impl Validate for AgentModelPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        models(&self.models)
    }
}

fn models(models: &[ModelChoice]) -> Result<(), ValidationError> {
    if models.len() > 100 {
        return invalid("An agent offers at most 100 models here.");
    }
    for model in models {
        text(&model.value, "A model", 200)?;
        text(&model.name, "A model's name", 200)?;
    }
    Ok(())
}
