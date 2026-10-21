use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, optional};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectStatus {
    Planned,
    Started,
    Paused,
    Completed,
    Canceled,
}

/// A set of issues working toward one outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// Markdown.
    pub description: String,
    pub status: ProjectStatus,
    pub lead_id: Option<Uuid>,
    pub initiative_id: Option<Uuid>,
    pub target_date: Option<NaiveDate>,
    pub sort_order: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ProjectCreate {
    pub name: String,
    pub description: Option<String>,
    pub status: Option<ProjectStatus>,
    pub lead_id: Option<Uuid>,
    pub initiative_id: Option<Uuid>,
    pub target_date: Option<NaiveDate>,
    pub sort_order: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectStatus>,
    pub lead_id: Option<Option<Uuid>>,
    pub initiative_id: Option<Option<Uuid>>,
    pub target_date: Option<Option<NaiveDate>>,
    pub sort_order: Option<f64>,
}

impl Validate for ProjectCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "project name", rules::NAME)?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })
    }
}

impl Validate for ProjectPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "project name", rules::NAME))?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum InitiativeStatus {
    Planned,
    Active,
    Completed,
}

/// A larger goal that groups projects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Initiative {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// Markdown.
    pub description: String,
    pub status: InitiativeStatus,
    pub owner_id: Option<Uuid>,
    pub target_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct InitiativeCreate {
    pub name: String,
    pub description: Option<String>,
    pub status: Option<InitiativeStatus>,
    pub owner_id: Option<Uuid>,
    pub target_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct InitiativePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<InitiativeStatus>,
    pub owner_id: Option<Option<Uuid>>,
    pub target_date: Option<Option<NaiveDate>>,
}

impl Validate for InitiativeCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "initiative name", rules::NAME)?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })
    }
}

impl Validate for InitiativePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| {
            rules::text(n, "initiative name", rules::NAME)
        })?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })
    }
}
