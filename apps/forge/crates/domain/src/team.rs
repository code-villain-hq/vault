use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, invalid, optional};

/// A team owns issues and has its own workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Team {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// Prefix of the team's issue identifiers: `FOR` in `FOR-12`.
    pub key: String,
    /// The last issue number given out. Maintained by the server.
    pub issue_counter: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Teams are created through `POST /api/w/{workspace}/teams`, which also sets
/// up their workflow; this is that request's body.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TeamCreate {
    pub name: String,
    pub key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TeamPatch {
    pub name: Option<String>,
    pub key: Option<String>,
}

fn check_key(key: &str) -> Result<(), ValidationError> {
    let mut chars = key.chars();
    let valid = key.len() <= 5
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        invalid("team key must be 1 to 5 capital letters or digits, starting with a letter")
    }
}

impl Validate for TeamCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "team name", rules::NAME)?;
        check_key(&self.key)
    }
}

impl Validate for TeamPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "team name", rules::NAME))?;
        optional(&self.key, |k| check_key(k))
    }
}

/// A team key from a name: its first three letters or digits, upper-cased.
pub fn team_key_from(name: &str) -> String {
    let key: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(3)
        .collect::<String>()
        .to_uppercase();
    if key.starts_with(|c: char| c.is_ascii_uppercase()) {
        key
    } else {
        "TEAM".into()
    }
}

/// What kind of step a workflow state is. Views, triage and progress depend
/// on the type, not the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StateType {
    /// New issues waiting for someone to accept them into the team.
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkflowState {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: StateType,
    pub color: String,
    /// Order within the team's workflow.
    pub position: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct WorkflowStateCreate {
    pub team_id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: StateType,
    pub color: Option<String>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct WorkflowStatePatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub position: Option<f64>,
}

impl Validate for WorkflowStateCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "state name", rules::NAME)?;
        optional(&self.color, |c| rules::color(c))
    }
}

impl Validate for WorkflowStatePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "state name", rules::NAME))?;
        optional(&self.color, |c| rules::color(c))
    }
}

/// The workflow every new team starts with: name, type and colour.
pub fn default_workflow() -> [(&'static str, StateType, &'static str); 7] {
    [
        ("Triage", StateType::Triage, "#f2994a"),
        ("Backlog", StateType::Backlog, "#8a8a94"),
        ("Todo", StateType::Unstarted, "#b5b5bd"),
        ("In Progress", StateType::Started, "#f2c94c"),
        ("In Review", StateType::Started, "#4cb782"),
        ("Done", StateType::Completed, "#5b5bd6"),
        ("Canceled", StateType::Canceled, "#8a8a94"),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Label {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Null for labels every team can use.
    pub team_id: Option<Uuid>,
    pub name: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LabelCreate {
    pub name: String,
    pub color: Option<String>,
    pub team_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct LabelPatch {
    pub name: Option<String>,
    pub color: Option<String>,
}

impl Validate for LabelCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "label name", rules::NAME)?;
        optional(&self.color, |c| rules::color(c))
    }
}

impl Validate for LabelPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "label name", rules::NAME))?;
        optional(&self.color, |c| rules::color(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_keys_come_from_names() {
        assert_eq!(team_key_from("Forge"), "FOR");
        assert_eq!(team_key_from("ui"), "UI");
        assert_eq!(team_key_from("3D Team"), "TEAM");
        assert_eq!(team_key_from("!!!"), "TEAM");
    }

    #[test]
    fn team_keys_are_short_capitals() {
        for good in ["F", "FOR", "ENG2", "ABCDE"] {
            assert!(check_key(good).is_ok(), "{good}");
        }
        for bad in ["", "for", "2FA", "ABCDEF", "FO-R"] {
            assert!(check_key(bad).is_err(), "{bad}");
        }
    }
}
