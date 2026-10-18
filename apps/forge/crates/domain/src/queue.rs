//! The agents' work queue: the order agents take up the issues assigned to
//! them, by priority, then age.

use std::cmp::Ordering;
use std::collections::HashMap;

use uuid::Uuid;

use crate::issue::{Issue, Priority};
use crate::team::{StateType, WorkflowState};
use crate::user::{User, UserKind};

/// A team's review status. Teams have no status type for review, so it's
/// the last of the started statuses when there are several (In Progress,
/// then In Review), as the review bar has it.
pub fn review_state(states: &[WorkflowState], team_id: Uuid) -> Option<&WorkflowState> {
    let mut started: Vec<&WorkflowState> = states
        .iter()
        .filter(|s| s.team_id == team_id && s.kind == StateType::Started)
        .collect();
    if started.len() < 2 {
        return None;
    }
    started.sort_by(|a, b| a.position.total_cmp(&b.position));
    started.last().copied()
}

/// Urgent first, no priority last.
fn priority_rank(priority: Priority) -> Priority {
    if priority == 0 { 5 } else { priority }
}

/// Queue order: by priority (urgent, high, medium, low, none), then oldest.
pub fn queue_order(a: &Issue, b: &Issue) -> Ordering {
    priority_rank(a.priority)
        .cmp(&priority_rank(b.priority))
        .then_with(|| a.created_at.cmp(&b.created_at))
        .then_with(|| a.id.cmp(&b.id))
}

/// Whether an issue waits in its assignee's queue: assigned to an agent, in
/// an unstarted or started status, and not in review (that waits for people).
pub fn in_queue(issue: &Issue, states: &[WorkflowState], users: &[User]) -> bool {
    let Some(assignee) = issue.assignee_id else {
        return false;
    };
    let agent = users
        .iter()
        .any(|u| u.id == assignee && u.kind == UserKind::Agent);
    let Some(state) = states.iter().find(|s| s.id == issue.state_id) else {
        return false;
    };
    agent
        && matches!(state.kind, StateType::Unstarted | StateType::Started)
        && review_state(states, issue.team_id).is_none_or(|review| review.id != state.id)
}

/// Each queued issue's place in its assignee's queue, counting from 1.
pub fn queue_positions(
    issues: &[Issue],
    states: &[WorkflowState],
    users: &[User],
) -> HashMap<Uuid, usize> {
    let mut queued: Vec<&Issue> = issues
        .iter()
        .filter(|i| in_queue(i, states, users))
        .collect();
    queued.sort_by(|a, b| queue_order(a, b));
    let mut taken: HashMap<Uuid, usize> = HashMap::new();
    queued
        .into_iter()
        .filter_map(|issue| {
            let count = taken.entry(issue.assignee_id?).or_default();
            *count += 1;
            Some((issue.id, *count))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeDelta, Utc};

    use super::*;
    use crate::account::Role;

    const TEAM: Uuid = Uuid::from_u128(1);
    const TODO: Uuid = Uuid::from_u128(2);
    const IN_PROGRESS: Uuid = Uuid::from_u128(3);
    const IN_REVIEW: Uuid = Uuid::from_u128(4);
    const BACKLOG: Uuid = Uuid::from_u128(5);
    const AGENT: Uuid = Uuid::from_u128(10);
    const OTHER_AGENT: Uuid = Uuid::from_u128(11);
    const PERSON: Uuid = Uuid::from_u128(12);

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::minutes(minutes)
    }

    fn state(id: Uuid, kind: StateType, position: f64) -> WorkflowState {
        WorkflowState {
            id,
            workspace_id: Uuid::nil(),
            team_id: TEAM,
            name: format!("{kind:?}"),
            kind,
            color: "#000000".into(),
            position,
            created_at: at(0),
            updated_at: at(0),
        }
    }

    fn states() -> Vec<WorkflowState> {
        vec![
            state(BACKLOG, StateType::Backlog, 1.0),
            state(TODO, StateType::Unstarted, 2.0),
            state(IN_REVIEW, StateType::Started, 4.0),
            state(IN_PROGRESS, StateType::Started, 3.0),
        ]
    }

    fn user(id: Uuid, kind: UserKind) -> User {
        User {
            id,
            workspace_id: Uuid::nil(),
            name: format!("{id}"),
            email: format!("{id}@example.com"),
            role: Role::Member,
            created_at: at(0),
            kind,
            working_on_id: None,
            working_since: None,
            working_seen_at: None,
        }
    }

    fn users() -> Vec<User> {
        vec![
            user(AGENT, UserKind::Agent),
            user(OTHER_AGENT, UserKind::Agent),
            user(PERSON, UserKind::Person),
        ]
    }

    fn issue(n: u128, priority: Priority, created: i64) -> Issue {
        Issue {
            id: Uuid::from_u128(100 + n),
            workspace_id: Uuid::nil(),
            team_id: TEAM,
            number: n as i32,
            title: format!("Issue {n}"),
            description: String::new(),
            state_id: TODO,
            priority,
            assignee_id: Some(AGENT),
            reviewer_id: None,
            parent_id: None,
            project_id: None,
            service_id: None,
            label_ids: vec![],
            due_date: None,
            sort_order: 0.0,
            completed_at: None,
            created_by: Uuid::nil(),
            created_at: at(created),
            updated_at: at(created),
        }
    }

    fn numbers(mut issues: Vec<Issue>) -> Vec<i32> {
        issues.sort_by(queue_order);
        issues.iter().map(|i| i.number).collect()
    }

    #[test]
    fn issues_come_by_priority_then_age() {
        let issues = vec![
            issue(1, 4, 0), // low
            issue(2, 0, 1), // no priority
            issue(3, 1, 5), // urgent, newer
            issue(4, 1, 2), // urgent, older
            issue(7, 3, 3), // medium
        ];
        assert_eq!(numbers(issues), [4, 3, 7, 1, 2]);
    }

    #[test]
    fn the_review_status_is_the_last_started_one_when_there_are_several() {
        let all = states();
        assert_eq!(review_state(&all, TEAM).map(|s| s.id), Some(IN_REVIEW));
        let one_started: Vec<WorkflowState> =
            all.into_iter().filter(|s| s.id != IN_REVIEW).collect();
        assert_eq!(review_state(&one_started, TEAM), None);
    }

    #[test]
    fn the_queue_holds_agents_planned_and_started_work_but_not_review() {
        let (states, users) = (states(), users());
        let with = |state_id: Uuid, assignee: Option<Uuid>| Issue {
            state_id,
            assignee_id: assignee,
            reviewer_id: None,
            ..issue(1, 0, 0)
        };
        assert!(in_queue(&with(TODO, Some(AGENT)), &states, &users));
        assert!(in_queue(&with(IN_PROGRESS, Some(AGENT)), &states, &users));
        assert!(!in_queue(&with(IN_REVIEW, Some(AGENT)), &states, &users));
        assert!(!in_queue(&with(BACKLOG, Some(AGENT)), &states, &users));
        assert!(!in_queue(&with(TODO, Some(PERSON)), &states, &users));
        assert!(!in_queue(&with(TODO, None), &states, &users));
    }

    #[test]
    fn positions_count_per_agent_in_queue_order() {
        let mut other = issue(3, 1, 0);
        other.assignee_id = Some(OTHER_AGENT);
        let mut reviewed = issue(4, 1, 0);
        reviewed.state_id = IN_REVIEW;
        let issues = vec![issue(1, 2, 0), issue(2, 4, 0), other, reviewed];
        let positions = queue_positions(&issues, &states(), &users());
        let position = |n: u128| positions.get(&Uuid::from_u128(100 + n)).copied();
        assert_eq!(position(1), Some(1), "high before low");
        assert_eq!(position(2), Some(2));
        assert_eq!(position(3), Some(1));
        assert_eq!(position(4), None);
    }
}
