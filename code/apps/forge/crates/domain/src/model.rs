use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub use code_villain_sync::{Actor, GuestView, ValidationError, Write};

use crate::drift::{AiRunCreate, AiRunPatch, SuggestionCreate, SuggestionPatch};
use crate::issue::{
    CommentCreate, CommentPatch, IssueCreate, IssuePatch, IssueRelationCreate, IssueRelationPatch,
};
use crate::knowledge::{
    DecisionCreate, DecisionPatch, DomainCreate, DomainPatch, ServiceCreate, ServicePatch,
    SpecCreate, SpecPatch,
};
use crate::link::{IssueLinkCreate, IssueLinkPatch};
use crate::notification::NotificationPatch;
use crate::planning::{InitiativeCreate, InitiativePatch, ProjectCreate, ProjectPatch};
use crate::spec_change::{SpecChangeCreate, SpecChangePatch};
use crate::team::{
    LabelCreate, LabelPatch, TeamCreate, TeamPatch, WorkflowStateCreate, WorkflowStatePatch,
};
use crate::verdict::{RequirementVerdictCreate, RequirementVerdictPatch};

/// ` IN (…)` the issues in the viewer's teams, to follow an issue id.
macro_rules! in_their_teams {
    () => {
        " IN (SELECT i.id FROM issues i JOIN team_members tm ON tm.team_id = i.team_id \
         WHERE tm.user_id = $viewer)"
    };
}

/// Every kind of record that clients sync.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
/// See "Adding a synced model" in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    Team,
    WorkflowState,
    Label,
    Issue,
    IssueRelation,
    Comment,
    Project,
    Initiative,
    User,
    Notification,
    Service,
    Domain,
    Spec,
    Decision,
    AiRun,
    Suggestion,
    IssueLink,
    RequirementVerdict,
    SpecChange,
    TeamMember,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[
        ModelKind::Team,
        ModelKind::WorkflowState,
        ModelKind::Label,
        ModelKind::Issue,
        ModelKind::IssueRelation,
        ModelKind::Comment,
        ModelKind::Project,
        ModelKind::Initiative,
        ModelKind::User,
        ModelKind::Notification,
        ModelKind::Service,
        ModelKind::Domain,
        ModelKind::Spec,
        ModelKind::Decision,
        ModelKind::AiRun,
        ModelKind::Suggestion,
        ModelKind::IssueLink,
        ModelKind::RequirementVerdict,
        ModelKind::SpecChange,
        ModelKind::TeamMember,
    ];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::Team => "teams",
            ModelKind::WorkflowState => "workflow_states",
            ModelKind::Label => "labels",
            ModelKind::Issue => "issues",
            ModelKind::IssueRelation => "issue_relations",
            ModelKind::Comment => "comments",
            ModelKind::Project => "projects",
            ModelKind::Initiative => "initiatives",
            ModelKind::User => "workspace_users",
            ModelKind::Notification => "notifications",
            ModelKind::Service => "services",
            ModelKind::Domain => "domains",
            ModelKind::Spec => "specs",
            ModelKind::Decision => "decisions",
            ModelKind::AiRun => "ai_runs",
            ModelKind::Suggestion => "suggestions",
            ModelKind::IssueLink => "issue_links",
            ModelKind::RequirementVerdict => "requirement_verdicts",
            ModelKind::SpecChange => "spec_changes",
            ModelKind::TeamMember => "team_members",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::Team => "Team",
            ModelKind::WorkflowState => "WorkflowState",
            ModelKind::Label => "Label",
            ModelKind::Issue => "Issue",
            ModelKind::IssueRelation => "IssueRelation",
            ModelKind::Comment => "Comment",
            ModelKind::Project => "Project",
            ModelKind::Initiative => "Initiative",
            ModelKind::User => "User",
            ModelKind::Notification => "Notification",
            ModelKind::Service => "Service",
            ModelKind::Domain => "Domain",
            ModelKind::Spec => "Spec",
            ModelKind::Decision => "Decision",
            ModelKind::AiRun => "AiRun",
            ModelKind::Suggestion => "Suggestion",
            ModelKind::IssueLink => "IssueLink",
            ModelKind::RequirementVerdict => "RequirementVerdict",
            ModelKind::SpecChange => "SpecChange",
            ModelKind::TeamMember => "TeamMember",
        }
    }

    pub fn from_name(name: &str) -> Option<ModelKind> {
        ModelKind::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync. Teams are
    /// created by the server (with their workflow); users are read-only;
    /// notifications come from the server, and their recipient may mark them
    /// read or delete them.
    pub fn client_may(self, write: Write) -> bool {
        match self {
            // Server-managed: runs are requested, and suggestions accepted or
            // dismissed, through the API; verdicts come from drift checks;
            // agents write spec changes through the MCP tools.
            ModelKind::User
            | ModelKind::AiRun
            | ModelKind::Suggestion
            | ModelKind::IssueLink
            | ModelKind::RequirementVerdict
            | ModelKind::SpecChange
            | ModelKind::TeamMember => false,
            ModelKind::Team => write == Write::Update,
            ModelKind::Notification => write != Write::Create,
            _ => true,
        }
    }

    /// Records only their owner (`user_id`) syncs, rather than the whole workspace.
    pub fn private_to_user(self) -> bool {
        self == ModelKind::Notification
    }

    /// Checks the fields sent to create a record.
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Team => check::<TeamCreate>(data),
            ModelKind::WorkflowState => check::<WorkflowStateCreate>(data),
            ModelKind::Label => check::<LabelCreate>(data),
            ModelKind::Issue => check::<IssueCreate>(data),
            ModelKind::IssueRelation => check::<IssueRelationCreate>(data),
            ModelKind::Comment => check::<CommentCreate>(data),
            ModelKind::Project => check::<ProjectCreate>(data),
            ModelKind::Initiative => check::<InitiativeCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
            ModelKind::Notification => {
                Err(ValidationError("notifications come from the server".into()))
            }
            ModelKind::Service => check::<ServiceCreate>(data),
            ModelKind::Domain => check::<DomainCreate>(data),
            ModelKind::Spec => check::<SpecCreate>(data),
            ModelKind::Decision => check::<DecisionCreate>(data),
            ModelKind::AiRun => check::<AiRunCreate>(data),
            ModelKind::Suggestion => check::<SuggestionCreate>(data),
            ModelKind::IssueLink => check::<IssueLinkCreate>(data),
            ModelKind::RequirementVerdict => check::<RequirementVerdictCreate>(data),
            ModelKind::SpecChange => check::<SpecChangeCreate>(data),
            ModelKind::TeamMember => Err(ValidationError("teams are set in Members".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Team => check::<TeamPatch>(data),
            ModelKind::WorkflowState => check::<WorkflowStatePatch>(data),
            ModelKind::Label => check::<LabelPatch>(data),
            ModelKind::Issue => check::<IssuePatch>(data),
            ModelKind::IssueRelation => check::<IssueRelationPatch>(data),
            ModelKind::Comment => check::<CommentPatch>(data),
            ModelKind::Project => check::<ProjectPatch>(data),
            ModelKind::Initiative => check::<InitiativePatch>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
            ModelKind::Notification => check::<NotificationPatch>(data),
            ModelKind::Service => check::<ServicePatch>(data),
            ModelKind::Domain => check::<DomainPatch>(data),
            ModelKind::Spec => check::<SpecPatch>(data),
            ModelKind::Decision => check::<DecisionPatch>(data),
            ModelKind::AiRun => check::<AiRunPatch>(data),
            ModelKind::Suggestion => check::<SuggestionPatch>(data),
            ModelKind::IssueLink => check::<IssueLinkPatch>(data),
            ModelKind::RequirementVerdict => check::<RequirementVerdictPatch>(data),
            ModelKind::SpecChange => check::<SpecChangePatch>(data),
            ModelKind::TeamMember => Err(ValidationError("teams are set in Members".into())),
        }
    }

    /// What a guest syncs of this kind: what's in their teams. Records of a
    /// team's issue follow the issue; the knowledge base, projects and
    /// initiatives span the workspace, so a guest has none.
    pub fn guest_view(self) -> GuestView {
        match self {
            ModelKind::Team => GuestView::Where(
                "t.id IN (SELECT team_id FROM team_members WHERE user_id = $viewer)",
            ),
            ModelKind::WorkflowState | ModelKind::Issue | ModelKind::TeamMember => {
                GuestView::Where(
                    "t.team_id IN (SELECT team_id FROM team_members WHERE user_id = $viewer)",
                )
            }
            ModelKind::Label => GuestView::Where(
                "t.team_id IS NULL \
                 OR t.team_id IN (SELECT team_id FROM team_members WHERE user_id = $viewer)",
            ),
            ModelKind::Comment | ModelKind::IssueLink | ModelKind::SpecChange => {
                GuestView::Where(concat!("t.issue_id", in_their_teams!()))
            }
            ModelKind::AiRun | ModelKind::Suggestion => GuestView::Where(concat!(
                "t.issue_id IS NOT NULL AND t.issue_id",
                in_their_teams!()
            )),
            ModelKind::Notification => GuestView::Where(concat!(
                "t.issue_id IS NULL OR t.issue_id",
                in_their_teams!()
            )),
            // Both ends, so no relation points at an issue they can't see.
            ModelKind::IssueRelation => GuestView::Where(concat!(
                "t.issue_id",
                in_their_teams!(),
                " AND t.related_issue_id",
                in_their_teams!()
            )),
            // Themselves, the admins (to ask), and whoever shares a team.
            ModelKind::User => GuestView::Where(
                "t.id = $viewer OR t.role = 'admin' OR t.id IN (SELECT o.user_id \
                 FROM team_members o JOIN team_members mine ON mine.team_id = o.team_id \
                 WHERE mine.user_id = $viewer)",
            ),
            ModelKind::Project
            | ModelKind::Initiative
            | ModelKind::Service
            | ModelKind::Domain
            | ModelKind::Spec
            | ModelKind::Decision
            | ModelKind::RequirementVerdict => GuestView::Hidden,
        }
    }
}

/// How the sync engine knows Forge's models.
impl code_villain_sync::Model for ModelKind {
    const ALL: &'static [ModelKind] = ModelKind::ALL;

    fn table(self) -> &'static str {
        ModelKind::table(self)
    }

    fn name(self) -> &'static str {
        ModelKind::name(self)
    }

    fn client_may(self, write: Write) -> bool {
        ModelKind::client_may(self, write)
    }

    fn private_to_user(self) -> bool {
        ModelKind::private_to_user(self)
    }

    fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_create(self, data)
    }

    fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_patch(self, data)
    }

    fn guest_view(self) -> GuestView {
        ModelKind::guest_view(self)
    }
}

/// Business rules a create or patch input must satisfy, beyond its types.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}

/// Parses `data` as `T`, which rejects unknown fields and wrong types, then
/// applies `T`'s rules.
fn check<T: DeserializeOwned + Validate>(data: &Map<String, Value>) -> Result<(), ValidationError> {
    let input: T = serde_json::from_value(Value::Object(data.clone()))
        .map_err(|e| ValidationError(e.to_string()))?;
    input.validate()
}
