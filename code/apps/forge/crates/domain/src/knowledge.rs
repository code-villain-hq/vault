//! Knowledge that keeps the tracker honest about the code: services (what
//! exists, and where its code lives), specs (how it should work) and
//! decisions (what was decided, and whether it still holds).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, optional};

/// Something that exists in the system: a service, an app, a feature area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Service {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// The name of its icon (one of Lucide's), shown in its `color`, so it's
    /// recognisable wherever it appears.
    pub icon: String,
    /// `#rrggbb`.
    pub color: String,
    /// Markdown: what the service is for.
    pub description: String,
    pub team_id: Option<Uuid>,
    /// Where its code lives: `github:owner/repo`, or a path on the server.
    pub repository: Option<String>,
    /// Paths in the repository that belong to the service, as globs.
    pub paths: Vec<String>,
    /// Markdown: what the code actually does today.
    pub current_state: String,
    pub current_state_updated_at: Option<DateTime<Utc>>,
    /// When a push last changed the service's code, which makes the
    /// verdicts checked before it stale. Set by the server.
    pub code_changed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ServiceCreate {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub team_id: Option<Uuid>,
    pub repository: Option<String>,
    pub paths: Option<Vec<String>>,
    pub current_state: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ServicePatch {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub team_id: Option<Option<Uuid>>,
    pub repository: Option<Option<String>>,
    pub paths: Option<Vec<String>>,
    pub current_state: Option<String>,
    pub current_state_updated_at: Option<Option<DateTime<Utc>>>,
    pub code_changed_at: Option<Option<DateTime<Utc>>>,
}

/// An area of the product that specs are about, such as Accounts: specs sit
/// in one domain each, the way OpenSpec keeps them in domain folders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Domain {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// Markdown: what the area covers.
    pub description: String,
    /// An icon of the same set as a service's, shown in its `color`.
    pub icon: String,
    /// `#rrggbb`.
    pub color: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct DomainCreate {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct DomainPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

impl Validate for DomainCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "domain name", rules::NAME)?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        optional(&self.icon, |i| check_icon(i))?;
        optional(&self.color, |c| rules::color(c))
    }
}

impl Validate for DomainPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "domain name", rules::NAME))?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        optional(&self.icon, |i| check_icon(i))?;
        optional(&self.color, |c| rules::color(c))
    }
}

/// A service's icon is the name of one in the web app's icon set (Lucide's
/// icons), such as `key` or `git-branch`; the app also reads the Tabler names
/// stored before, and shows unknown names as `server`, so the server only
/// checks it looks like a name.
fn check_icon(icon: &str) -> Result<(), ValidationError> {
    let valid = !icon.is_empty()
        && icon.len() <= 40
        && icon.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    if valid {
        Ok(())
    } else {
        rules::invalid("a service's icon must be an icon name, such as key or git-branch")
    }
}

fn check_paths(paths: &[String]) -> Result<(), ValidationError> {
    if paths.len() > 100 {
        return rules::invalid("a service can have at most 100 paths");
    }
    for path in paths {
        rules::text(path, "path", 500)?;
    }
    Ok(())
}

impl Validate for ServiceCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.name, "service name", rules::NAME)?;
        optional(&self.icon, |i| check_icon(i))?;
        optional(&self.color, |c| rules::color(c))?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        optional(&self.repository, |r| rules::text(r, "repository", 500))?;
        optional(&self.paths, |p| check_paths(p))?;
        optional(&self.current_state, |c| {
            rules::long_text(c, "current state", rules::DESCRIPTION)
        })
    }
}

impl Validate for ServicePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.name, |n| rules::text(n, "service name", rules::NAME))?;
        optional(&self.icon, |i| check_icon(i))?;
        optional(&self.color, |c| rules::color(c))?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        if let Some(Some(repository)) = &self.repository {
            rules::text(repository, "repository", 500)?;
        }
        optional(&self.paths, |p| check_paths(p))?;
        optional(&self.current_state, |c| {
            rules::long_text(c, "current state", rules::DESCRIPTION)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SpecStatus {
    Draft,
    Active,
    Deprecated,
}

/// How something should work. Coding agents read specs from Forge instead
/// of keeping copies in the repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Spec {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    /// Markdown.
    pub body: String,
    pub status: SpecStatus,
    /// The services whose code it describes: none for one about the whole
    /// workspace, several when its behaviour spans them.
    pub service_ids: Vec<Uuid>,
    /// The area of the product it's about, such as Accounts: one per spec,
    /// like a folder; specs are browsed by it.
    pub domain_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// Set by the server.
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct SpecCreate {
    pub title: String,
    pub body: Option<String>,
    pub status: Option<SpecStatus>,
    pub service_ids: Option<Vec<Uuid>>,
    pub domain_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct SpecPatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<SpecStatus>,
    pub service_ids: Option<Vec<Uuid>>,
    pub domain_id: Option<Option<Uuid>>,
    pub project_id: Option<Option<Uuid>>,
}

fn check_services(ids: &[Uuid]) -> Result<(), ValidationError> {
    if ids.len() > 20 {
        return rules::invalid("a spec can belong to at most 20 services");
    }
    if ids.iter().enumerate().any(|(i, id)| ids[..i].contains(id)) {
        return rules::invalid("a spec lists each of its services once");
    }
    Ok(())
}

impl Validate for SpecCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.title, "title", rules::TITLE)?;
        optional(&self.service_ids, |s| check_services(s))?;
        optional(&self.body, |b| {
            rules::long_text(b, "spec", rules::DESCRIPTION)
        })
    }
}

impl Validate for SpecPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.title, |t| rules::text(t, "title", rules::TITLE))?;
        optional(&self.service_ids, |s| check_services(s))?;
        optional(&self.body, |b| {
            rules::long_text(b, "spec", rules::DESCRIPTION)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    /// Replaced on purpose by a newer decision.
    Superseded,
    /// No longer true in the code or the plans, without a decision replacing it.
    Overridden,
}

/// Something decided, in the usual decision-record shape. Forge's AI checks
/// whether the code and newer issues still respect it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Decision {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    /// Markdown: the situation that called for a decision.
    pub context: String,
    /// Markdown: what was decided.
    pub decision: String,
    /// Markdown: what follows from it.
    pub consequences: String,
    pub status: DecisionStatus,
    pub service_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub superseded_by_id: Option<Uuid>,
    pub decided_on: Option<NaiveDate>,
    /// Set by the server.
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct DecisionCreate {
    pub title: String,
    pub context: Option<String>,
    pub decision: Option<String>,
    pub consequences: Option<String>,
    pub status: Option<DecisionStatus>,
    pub service_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub superseded_by_id: Option<Uuid>,
    pub decided_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct DecisionPatch {
    pub title: Option<String>,
    pub context: Option<String>,
    pub decision: Option<String>,
    pub consequences: Option<String>,
    pub status: Option<DecisionStatus>,
    pub service_id: Option<Option<Uuid>>,
    pub project_id: Option<Option<Uuid>>,
    pub superseded_by_id: Option<Option<Uuid>>,
    pub decided_on: Option<Option<NaiveDate>>,
}

fn check_decision_text(texts: [&Option<String>; 3]) -> Result<(), ValidationError> {
    for text in texts {
        optional(text, |t| {
            rules::long_text(t, "decision text", rules::DESCRIPTION)
        })?;
    }
    Ok(())
}

impl Validate for DecisionCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.title, "title", rules::TITLE)?;
        check_decision_text([&self.context, &self.decision, &self.consequences])
    }
}

impl Validate for DecisionPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.title, |t| rules::text(t, "title", rules::TITLE))?;
        check_decision_text([&self.context, &self.decision, &self.consequences])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_has_an_icon_name_and_a_colour() {
        let patch = |icon: &str, color: &str| ServicePatch {
            icon: Some(icon.into()),
            color: Some(color.into()),
            ..Default::default()
        };
        assert!(patch("key", "#4cb782").validate().is_ok());
        assert!(patch("terminal-2", "#4cb782").validate().is_ok());
        for bad in ["", "Key", "git_branch", "-key", "key-", "a--b", "<svg>"] {
            assert!(patch(bad, "#4cb782").validate().is_err(), "{bad}");
        }
        assert!(patch("key", "green").validate().is_err());
    }
}
