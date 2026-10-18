//! An issue's history, worked out from its versions in the change log.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::issue::{Issue, Priority};

/// One thing that happened to an issue.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct IssueEvent {
    pub at: DateTime<Utc>,
    /// Who did it: a person or an agent.
    pub actor_id: Uuid,
    #[serde(flatten)]
    pub change: IssueChange,
}

/// What changed. Ids refer to records the client already has.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "field", rename_all = "snake_case")]
#[ts(export)]
pub enum IssueChange {
    Created,
    Title {
        from: String,
        to: String,
    },
    Description,
    State {
        from: Uuid,
        to: Uuid,
    },
    Priority {
        from: Priority,
        to: Priority,
    },
    Assignee {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    Reviewer {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    Labels {
        added: Vec<Uuid>,
        removed: Vec<Uuid>,
    },
    Project {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    Parent {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    Service {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    DueDate {
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    },
    Team {
        from: Uuid,
        to: Uuid,
    },
}

/// Edits to the text by the same person this close together count as one.
const TYPING: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

/// One version of an issue from the change log.
pub struct IssueVersion {
    pub at: DateTime<Utc>,
    pub actor_id: Uuid,
    /// Whether this version created the issue.
    pub created: bool,
    /// `None` if it couldn't be read, such as one from an older schema.
    pub issue: Option<Issue>,
}

/// The history of an issue from its versions, oldest first. Ordering and
/// other bookkeeping are left out, and quick successive edits to the title
/// or description are merged.
pub fn issue_history(versions: impl IntoIterator<Item = IssueVersion>) -> Vec<IssueEvent> {
    let mut events: Vec<IssueEvent> = Vec::new();
    let mut previous: Option<Issue> = None;
    for version in versions {
        let IssueVersion {
            at,
            actor_id,
            created,
            issue,
        } = version;
        let mut changes = Vec::new();
        if created {
            changes.push(IssueChange::Created);
        }
        // Without both sides there's nothing to compare: the next readable
        // version is compared with this one.
        if let (Some(before), Some(after)) = (&previous, &issue) {
            changes.extend(diff(before, after));
        }
        if issue.is_some() {
            previous = issue;
        }
        for change in changes {
            let event = IssueEvent {
                at,
                actor_id,
                change,
            };
            if !merge(&mut events, &event) {
                events.push(event);
            }
        }
    }
    events
}

fn diff(a: &Issue, b: &Issue) -> Vec<IssueChange> {
    let mut out = Vec::new();
    if a.team_id != b.team_id {
        out.push(IssueChange::Team {
            from: a.team_id,
            to: b.team_id,
        });
    }
    if a.title != b.title {
        out.push(IssueChange::Title {
            from: a.title.clone(),
            to: b.title.clone(),
        });
    }
    if a.description != b.description {
        out.push(IssueChange::Description);
    }
    if a.state_id != b.state_id {
        out.push(IssueChange::State {
            from: a.state_id,
            to: b.state_id,
        });
    }
    if a.priority != b.priority {
        out.push(IssueChange::Priority {
            from: a.priority,
            to: b.priority,
        });
    }
    if a.assignee_id != b.assignee_id {
        out.push(IssueChange::Assignee {
            from: a.assignee_id,
            to: b.assignee_id,
        });
    }
    if a.reviewer_id != b.reviewer_id {
        out.push(IssueChange::Reviewer {
            from: a.reviewer_id,
            to: b.reviewer_id,
        });
    }
    let added: Vec<Uuid> = (b.label_ids.iter())
        .filter(|l| !a.label_ids.contains(l))
        .copied()
        .collect();
    let removed: Vec<Uuid> = (a.label_ids.iter())
        .filter(|l| !b.label_ids.contains(l))
        .copied()
        .collect();
    if !added.is_empty() || !removed.is_empty() {
        out.push(IssueChange::Labels { added, removed });
    }
    if a.project_id != b.project_id {
        out.push(IssueChange::Project {
            from: a.project_id,
            to: b.project_id,
        });
    }
    if a.parent_id != b.parent_id {
        out.push(IssueChange::Parent {
            from: a.parent_id,
            to: b.parent_id,
        });
    }
    if a.service_id != b.service_id {
        out.push(IssueChange::Service {
            from: a.service_id,
            to: b.service_id,
        });
    }
    if a.due_date != b.due_date {
        out.push(IssueChange::DueDate {
            from: a.due_date,
            to: b.due_date,
        });
    }
    out
}

/// Folds a text edit into the same person's edit just before, if any.
fn merge(events: &mut Vec<IssueEvent>, event: &IssueEvent) -> bool {
    let same_field =
        |e: &IssueEvent| std::mem::discriminant(&e.change) == std::mem::discriminant(&event.change);
    let Some(index) = events.iter().rposition(same_field) else {
        return false;
    };
    if events[index].actor_id != event.actor_id || event.at - events[index].at >= TYPING {
        return false;
    }
    match (&mut events[index].change, &event.change) {
        (IssueChange::Description, IssueChange::Description) => {
            events[index].at = event.at;
            true
        }
        (IssueChange::Title { from, to }, IssueChange::Title { to: newer, .. }) => {
            if from == newer {
                // Back where it started: nothing happened.
                events.remove(index);
            } else {
                *to = newer.clone();
                events[index].at = event.at;
            }
            true
        }
        _ => false,
    }
}

/// One version of a spec's text from the change log: the spec's history.
/// Only versions that changed the text count; retitling and the like don't.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SpecVersion {
    pub at: DateTime<Utc>,
    /// Who saved it: a person or an agent, or whoever finished the issue
    /// whose spec change it was.
    pub actor_id: Uuid,
    /// Whether this version created the spec.
    pub created: bool,
    pub body: String,
    /// The spec change that made it, when it took effect with its issue.
    pub spec_change_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue() -> Issue {
        Issue {
            id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            team_id: Uuid::nil(),
            number: 1,
            title: "Rate limits".into(),
            description: String::new(),
            state_id: Uuid::from_u128(1),
            priority: 0,
            assignee_id: None,
            reviewer_id: None,
            parent_id: None,
            project_id: None,
            service_id: None,
            label_ids: vec![],
            due_date: None,
            sort_order: 0.0,
            completed_at: None,
            created_by: Uuid::nil(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn minutes(m: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(m)
    }

    /// Versions at the given minutes; the first one created the issue.
    fn versions(list: Vec<(i64, Uuid, Option<Issue>)>) -> Vec<IssueVersion> {
        list.into_iter()
            .enumerate()
            .map(|(i, (m, actor_id, issue))| IssueVersion {
                at: minutes(m),
                actor_id,
                created: i == 0,
                issue,
            })
            .collect()
    }

    const ADA: Uuid = Uuid::from_u128(10);
    const BOB: Uuid = Uuid::from_u128(11);

    #[test]
    fn each_field_change_is_an_event() {
        let v1 = issue();
        let mut v2 = v1.clone();
        v2.state_id = Uuid::from_u128(2);
        v2.priority = 2;
        v2.label_ids = vec![Uuid::from_u128(5)];
        v2.sort_order = 3.0; // bookkeeping, not history
        v2.updated_at = Utc::now();
        let mut v3 = v2.clone();
        v3.label_ids = vec![Uuid::from_u128(6)];

        let fields: Vec<IssueChange> = issue_history(versions(vec![
            (0, ADA, Some(v1)),
            (1, BOB, Some(v2)),
            (2, BOB, None),
            (3, ADA, Some(v3)),
        ]))
        .into_iter()
        .map(|e| e.change)
        .collect();
        assert_eq!(
            fields,
            [
                IssueChange::Created,
                IssueChange::State {
                    from: Uuid::from_u128(1),
                    to: Uuid::from_u128(2)
                },
                IssueChange::Priority { from: 0, to: 2 },
                IssueChange::Labels {
                    added: vec![Uuid::from_u128(5)],
                    removed: vec![]
                },
                IssueChange::Labels {
                    added: vec![Uuid::from_u128(6)],
                    removed: vec![Uuid::from_u128(5)]
                },
            ]
        );
    }

    #[test]
    fn quick_text_edits_by_one_person_merge() {
        let v1 = issue();
        let mut v2 = v1.clone();
        v2.title = "Rate limit".into();
        v2.description = "a".into();
        let mut v3 = v2.clone();
        v3.title = "Rate limiting".into();
        v3.description = "ab".into();
        let mut v4 = v3.clone();
        v4.title = "Rate limits".into(); // back to the start, by someone else
        let mut v5 = v4.clone();
        v5.description = "abc".into(); // much later

        let events = issue_history(versions(vec![
            (0, ADA, Some(v1)),
            (1, ADA, Some(v2)),
            (2, ADA, Some(v3)),
            (3, BOB, Some(v4)),
            (60, ADA, Some(v5)),
        ]));
        let changes: Vec<(Uuid, IssueChange)> =
            events.into_iter().map(|e| (e.actor_id, e.change)).collect();
        assert_eq!(
            changes,
            [
                (ADA, IssueChange::Created),
                (
                    ADA,
                    IssueChange::Title {
                        from: "Rate limits".into(),
                        to: "Rate limiting".into()
                    }
                ),
                (ADA, IssueChange::Description),
                (
                    BOB,
                    IssueChange::Title {
                        from: "Rate limiting".into(),
                        to: "Rate limits".into()
                    }
                ),
                (ADA, IssueChange::Description),
            ]
        );
    }

    #[test]
    fn a_title_changed_and_changed_back_leaves_nothing() {
        let v1 = issue();
        let mut v2 = v1.clone();
        v2.title = "Oops".into();
        let v3 = v1.clone();
        let events = issue_history(versions(vec![
            (0, ADA, Some(v1)),
            (1, ADA, Some(v2)),
            (2, ADA, Some(v3)),
        ]));
        assert_eq!(events.len(), 1, "{events:?}");
    }

    #[test]
    fn unreadable_versions_still_have_a_creator() {
        let v1 = issue();
        let mut v2 = v1.clone();
        v2.priority = 1;
        let mut v3 = v2.clone();
        v3.priority = 3;
        let events = issue_history(versions(vec![
            (0, ADA, None),
            (1, BOB, Some(v2)),
            (2, BOB, Some(v3)),
        ]));
        let changes: Vec<(Uuid, IssueChange)> =
            events.into_iter().map(|e| (e.actor_id, e.change)).collect();
        assert_eq!(
            changes,
            [
                (ADA, IssueChange::Created),
                (BOB, IssueChange::Priority { from: 1, to: 3 })
            ]
        );
    }
}
