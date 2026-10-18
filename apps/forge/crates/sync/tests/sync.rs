//! The sync engine against a real Postgres. Each test gets a fresh database.

use std::collections::HashMap;

use forge_domain::{
    Actor, Issue, ModelKind, Operation, SyncAction, SyncEntry, Transaction, default_workflow,
};
use forge_sync::{
    Origin, SyncError, apply_in, apply_transaction, bootstrap, entries_after, record_change,
    spec_versions,
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    actor: Actor,
    team: Uuid,
    states: HashMap<&'static str, Uuid>,
}

impl Fixture {
    fn state(&self, name: &str) -> Uuid {
        self.states[name]
    }
}

/// A workspace with one member and one team with the default workflow.
async fn seed(pool: &PgPool) -> Fixture {
    let actor = Actor {
        user_id: Uuid::now_v7(),
        workspace_id: Uuid::now_v7(),
    };
    sqlx::query("INSERT INTO workspaces (id, name, url_key) VALUES ($1, 'Test', $1::text)")
        .bind(actor.workspace_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $1::text, 'Tester')")
        .bind(actor.user_id)
        .execute(pool)
        .await
        .unwrap();
    add_member(pool, actor.workspace_id, actor.user_id, "admin").await;
    let (team, states) = create_team(pool, actor, "TST").await;
    Fixture {
        actor,
        team,
        states,
    }
}

async fn add_member(pool: &PgPool, workspace_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

/// Creates a team with the default workflow the way the server does.
async fn create_team(
    pool: &PgPool,
    actor: Actor,
    key: &str,
) -> (Uuid, HashMap<&'static str, Uuid>) {
    let team = Uuid::now_v7();
    let mut operations = vec![Operation::Create {
        model: ModelKind::Team,
        id: team,
        data: object(json!({ "name": format!("Team {key}"), "key": key })),
    }];
    let mut states = HashMap::new();
    for (position, (name, kind, color)) in default_workflow().into_iter().enumerate() {
        let id = Uuid::now_v7();
        states.insert(name, id);
        operations.push(Operation::Create {
            model: ModelKind::WorkflowState,
            id,
            data: object(json!({ "team_id": team, "name": name, "type": kind, "color": color, "position": position })),
        });
    }
    let mut db = pool.begin().await.unwrap();
    apply_in(&mut db, actor, &tx(operations), Origin::Server)
        .await
        .unwrap();
    db.commit().await.unwrap();
    (team, states)
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn tx(operations: Vec<Operation>) -> Transaction {
    Transaction {
        id: Uuid::now_v7(),
        operations,
    }
}

fn create(model: ModelKind, id: Uuid, data: Value) -> Operation {
    Operation::Create {
        model,
        id,
        data: object(data),
    }
}

fn update(model: ModelKind, id: Uuid, data: Value) -> Operation {
    Operation::Update {
        model,
        id,
        data: object(data),
    }
}

fn delete(model: ModelKind, id: Uuid) -> Operation {
    Operation::Delete { model, id }
}

async fn apply(
    pool: &PgPool,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<Option<i64>, SyncError> {
    apply_transaction(pool, actor, &tx(operations)).await
}

/// Creates an issue in the fixture's team and returns its id.
async fn new_issue(pool: &PgPool, f: &Fixture, data: Value) -> Uuid {
    let id = Uuid::now_v7();
    let mut data = object(data);
    data.insert("team_id".into(), json!(f.team));
    data.entry("title").or_insert(json!("An issue"));
    apply(
        pool,
        f.actor,
        vec![Operation::Create {
            model: ModelKind::Issue,
            id,
            data,
        }],
    )
    .await
    .unwrap();
    id
}

async fn records(pool: &PgPool, actor: Actor, model: ModelKind) -> Vec<Value> {
    bootstrap(pool, actor)
        .await
        .unwrap()
        .records
        .remove(&model)
        .unwrap_or_default()
}

async fn issues(pool: &PgPool, actor: Actor) -> Vec<Issue> {
    records(pool, actor, ModelKind::Issue)
        .await
        .into_iter()
        .map(|r| serde_json::from_value(r).expect("issue rows match the Issue type"))
        .collect()
}

async fn issue(pool: &PgPool, actor: Actor, id: Uuid) -> Issue {
    issues(pool, actor)
        .await
        .into_iter()
        .find(|i| i.id == id)
        .expect("issue exists")
}

async fn log(pool: &PgPool, actor: Actor) -> Vec<SyncEntry> {
    entries_after(pool, actor, 0, 1000).await.unwrap().0
}

fn invalid<T: std::fmt::Debug>(result: Result<T, SyncError>) {
    assert!(
        matches!(result, Err(SyncError::Invalid(_))),
        "expected Invalid, got {result:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_issue_gets_a_number_a_backlog_state_and_an_author(pool: PgPool) {
    let f = seed(&pool).await;
    let before = log(&pool, f.actor).await.len();

    let id = new_issue(&pool, &f, json!({ "title": "First" })).await;

    let created = issue(&pool, f.actor, id).await;
    assert_eq!(created.number, 1);
    assert_eq!(created.state_id, f.state("Backlog"));
    assert_eq!(created.created_by, f.actor.user_id);
    assert!(created.label_ids.is_empty());

    let entries = log(&pool, f.actor).await;
    assert_eq!(entries.len(), before + 1);
    let last = entries.last().unwrap();
    assert_eq!(
        (last.action, last.model, last.model_id),
        (SyncAction::Insert, ModelKind::Issue, id)
    );
    assert_eq!(
        bootstrap(&pool, f.actor).await.unwrap().sync_id,
        last.sync_id
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn issue_numbers_count_up_per_team(pool: PgPool) {
    let f = seed(&pool).await;
    let (other_team, _) = create_team(&pool, f.actor, "OTH").await;
    for _ in 0..2 {
        new_issue(&pool, &f, json!({})).await;
    }
    let other = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Issue,
            other,
            json!({ "team_id": other_team, "title": "x" }),
        )],
    )
    .await
    .unwrap();

    let all = issues(&pool, f.actor).await;
    let mut numbers: Vec<(Uuid, i32)> = all.iter().map(|i| (i.team_id, i.number)).collect();
    numbers.sort();
    assert!(numbers.contains(&(f.team, 1)) && numbers.contains(&(f.team, 2)));
    assert_eq!(issue(&pool, f.actor, other).await.number, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_changes_only_the_fields_sent(pool: PgPool) {
    let f = seed(&pool).await;
    let id = new_issue(&pool, &f, json!({ "title": "Old", "priority": 2 })).await;

    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "state_id": f.state("In Progress") }),
        )],
    )
    .await
    .unwrap();
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "title": "New", "due_date": "2026-11-01" }),
        )],
    )
    .await
    .unwrap();

    let changed = issue(&pool, f.actor, id).await;
    assert_eq!(changed.title, "New");
    assert_eq!(changed.state_id, f.state("In Progress"));
    assert_eq!(changed.priority, 2);
    assert_eq!(changed.due_date.unwrap().to_string(), "2026-11-01");

    apply(
        &pool,
        f.actor,
        vec![update(ModelKind::Issue, id, json!({ "due_date": null }))],
    )
    .await
    .unwrap();
    assert_eq!(issue(&pool, f.actor, id).await.due_date, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn completing_an_issue_records_when(pool: PgPool) {
    let f = seed(&pool).await;
    let id = new_issue(&pool, &f, json!({})).await;
    assert!(issue(&pool, f.actor, id).await.completed_at.is_none());

    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();
    let done = issue(&pool, f.actor, id)
        .await
        .completed_at
        .expect("completed_at is set");

    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "title": "Still done" }),
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        issue(&pool, f.actor, id).await.completed_at,
        Some(done),
        "unchanged while it stays done"
    );

    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "state_id": f.state("Todo") }),
        )],
    )
    .await
    .unwrap();
    assert!(
        issue(&pool, f.actor, id).await.completed_at.is_none(),
        "cleared when reopened"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_retried_transaction_is_applied_once(pool: PgPool) {
    let f = seed(&pool).await;
    let before = log(&pool, f.actor).await.len();
    let transaction = tx(vec![create(
        ModelKind::Issue,
        Uuid::now_v7(),
        json!({ "team_id": f.team, "title": "Once" }),
    )]);

    let first = apply_transaction(&pool, f.actor, &transaction)
        .await
        .unwrap();
    let second = apply_transaction(&pool, f.actor, &transaction)
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(issues(&pool, f.actor).await.len(), 1);
    assert_eq!(log(&pool, f.actor).await.len(), before + 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_operation_rolls_back_the_whole_transaction(pool: PgPool) {
    let f = seed(&pool).await;
    let before = log(&pool, f.actor).await.len();

    let result = apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Issue,
                Uuid::now_v7(),
                json!({ "team_id": f.team, "title": "Good" }),
            ),
            create(
                ModelKind::Issue,
                Uuid::now_v7(),
                json!({ "team_id": f.team, "title": "" }),
            ),
        ],
    )
    .await;

    invalid(result);
    assert!(issues(&pool, f.actor).await.is_empty());
    assert_eq!(log(&pool, f.actor).await.len(), before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_input_is_rejected(pool: PgPool) {
    let f = seed(&pool).await;
    let (other_team, other_states) = create_team(&pool, f.actor, "OTH").await;
    let id = new_issue(&pool, &f, json!({})).await;

    for data in [
        json!({ "number": 99 }),
        json!({ "title": null }),
        json!({ "priority": 7 }),
        json!({ "state_id": other_states["Done"] }),
        json!({ "team_id": other_team }),
        json!({ "assignee_id": Uuid::now_v7() }),
    ] {
        invalid(apply(&pool, f.actor, vec![update(ModelKind::Issue, id, data)]).await);
    }

    let duplicate = apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Issue,
            id,
            json!({ "team_id": f.team, "title": "x" }),
        )],
    );
    assert!(matches!(duplicate.await, Err(SyncError::Conflict(_))));
}

#[sqlx::test(migrations = "../../migrations")]
async fn updating_a_missing_record_fails_but_deleting_one_does_nothing(pool: PgPool) {
    let f = seed(&pool).await;
    let missing = Uuid::now_v7();

    let result = apply(
        &pool,
        f.actor,
        vec![update(ModelKind::Issue, missing, json!({ "title": "x" }))],
    )
    .await;
    assert!(matches!(result, Err(SyncError::NotFound(_))), "{result:?}");
    assert_eq!(
        apply(&pool, f.actor, vec![delete(ModelKind::Issue, missing)])
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_an_issue_cleans_up_through_the_log(pool: PgPool) {
    let f = seed(&pool).await;
    let parent = new_issue(&pool, &f, json!({ "title": "Parent" })).await;
    let child = new_issue(&pool, &f, json!({ "title": "Child", "parent_id": parent })).await;
    let other = new_issue(&pool, &f, json!({ "title": "Other" })).await;
    let comment = Uuid::now_v7();
    let relation = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Comment,
                comment,
                json!({ "issue_id": parent, "body": "Hi" }),
            ),
            create(
                ModelKind::IssueRelation,
                relation,
                json!({ "issue_id": other, "related_issue_id": parent, "type": "blocks" }),
            ),
        ],
    )
    .await
    .unwrap();
    // A triage run that looked at it, written by the server.
    let run = Uuid::now_v7();
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        f.actor,
        &tx(vec![create(
            ModelKind::AiRun,
            run,
            json!({ "kind": "triage", "issue_id": parent, "trigger": "manual" }),
        )]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
    let before = log(&pool, f.actor).await.len();

    apply(&pool, f.actor, vec![delete(ModelKind::Issue, parent)])
        .await
        .unwrap();

    let changes: Vec<(ModelKind, Uuid, SyncAction)> = log(&pool, f.actor)
        .await
        .into_iter()
        .skip(before)
        .map(|e| (e.model, e.model_id, e.action))
        .collect();
    assert_eq!(
        changes,
        [
            (ModelKind::Comment, comment, SyncAction::Delete),
            (ModelKind::AiRun, run, SyncAction::Update),
            (ModelKind::IssueRelation, relation, SyncAction::Delete),
            (ModelKind::Issue, child, SyncAction::Update),
            (ModelKind::Issue, parent, SyncAction::Delete),
        ]
    );
    assert_eq!(issue(&pool, f.actor, child).await.parent_id, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sub_issues_cannot_loop(pool: PgPool) {
    let f = seed(&pool).await;
    let a = new_issue(&pool, &f, json!({})).await;
    let b = new_issue(&pool, &f, json!({ "parent_id": a })).await;
    let c = new_issue(&pool, &f, json!({ "parent_id": b })).await;

    invalid(
        apply(
            &pool,
            f.actor,
            vec![update(ModelKind::Issue, a, json!({ "parent_id": c }))],
        )
        .await,
    );
    invalid(
        apply(
            &pool,
            f.actor,
            vec![update(ModelKind::Issue, a, json!({ "parent_id": a }))],
        )
        .await,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn labels_must_exist_and_deleting_one_removes_it_from_issues(pool: PgPool) {
    let f = seed(&pool).await;
    let bug = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Label,
            bug,
            json!({ "name": "Bug", "color": "#d1453b" }),
        )],
    )
    .await
    .unwrap();
    let id = new_issue(&pool, &f, json!({ "label_ids": [bug] })).await;

    invalid(
        apply(
            &pool,
            f.actor,
            vec![update(
                ModelKind::Issue,
                id,
                json!({ "label_ids": [Uuid::now_v7()] }),
            )],
        )
        .await,
    );

    apply(&pool, f.actor, vec![delete(ModelKind::Label, bug)])
        .await
        .unwrap();
    assert!(issue(&pool, f.actor, id).await.label_ids.is_empty());
}

/// Asserts the write was refused for who made it, with `message`.
fn forbidden<T: std::fmt::Debug>(result: Result<T, SyncError>, message: &str) {
    match result {
        Err(SyncError::Forbidden(said)) => assert_eq!(said, message),
        other => panic!("expected Forbidden({message:?}), got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_author_can_change_a_comment(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let cy = add_person(&pool, &f, "Cy").await;
    let issue_id = new_issue(&pool, &f, json!({})).await;
    let (comment, other) = (Uuid::now_v7(), Uuid::now_v7());
    for id in [comment, other] {
        apply(
            &pool,
            bob,
            vec![create(
                ModelKind::Comment,
                id,
                json!({ "issue_id": issue_id, "body": "Mine" }),
            )],
        )
        .await
        .unwrap();
    }
    let stored = &records(&pool, f.actor, ModelKind::Comment).await[0];
    assert_eq!(stored["author_id"], json!(bob.user_id));

    let edit = |id| {
        vec![update(
            ModelKind::Comment,
            id,
            json!({ "body": "Not mine" }),
        )]
    };
    forbidden(
        apply(&pool, cy, edit(comment)).await,
        "Only its author can edit a comment.",
    );
    forbidden(
        apply(&pool, cy, vec![delete(ModelKind::Comment, comment)]).await,
        "Only its author or an admin can delete a comment.",
    );
    // Admins may delete one, but not put words in someone's mouth.
    forbidden(
        apply(&pool, f.actor, edit(comment)).await,
        "Only its author can edit a comment.",
    );
    apply(&pool, f.actor, vec![delete(ModelKind::Comment, other)])
        .await
        .unwrap();

    apply(
        &pool,
        bob,
        vec![update(
            ModelKind::Comment,
            comment,
            json!({ "body": "Edited" }),
        )],
    )
    .await
    .unwrap();
    apply(&pool, bob, vec![delete(ModelKind::Comment, comment)])
        .await
        .unwrap();
    assert!(records(&pool, f.actor, ModelKind::Comment).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn members_delete_only_issues_they_created(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let admins = new_issue(&pool, &f, json!({ "title": "The admin's" })).await;
    let (own, other) = (Uuid::now_v7(), Uuid::now_v7());
    for id in [own, other] {
        apply(
            &pool,
            bob,
            vec![create(
                ModelKind::Issue,
                id,
                json!({ "team_id": f.team, "title": "Bob's" }),
            )],
        )
        .await
        .unwrap();
    }

    // Bob may change the admin's issue, but not delete it.
    apply(
        &pool,
        bob,
        vec![update(ModelKind::Issue, admins, json!({ "priority": 2 }))],
    )
    .await
    .unwrap();
    forbidden(
        apply(&pool, bob, vec![delete(ModelKind::Issue, admins)]).await,
        "Only admins and whoever created an issue can delete it.",
    );
    assert_eq!(issue(&pool, f.actor, admins).await.priority, 2);

    // His own he may; admins may delete anyone's.
    apply(&pool, bob, vec![delete(ModelKind::Issue, own)])
        .await
        .unwrap();
    apply(&pool, f.actor, vec![delete(ModelKind::Issue, other)])
        .await
        .unwrap();
    let left: Vec<Uuid> = issues(&pool, f.actor).await.iter().map(|i| i.id).collect();
    assert_eq!(left, [admins]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_delete_knowledge_and_plans(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let records_of = [
        (
            ModelKind::Spec,
            json!({ "title": "How sync works" }),
            "specs",
        ),
        (
            ModelKind::Decision,
            json!({ "title": "Build our own sync" }),
            "decisions",
        ),
        (ModelKind::Project, json!({ "name": "Launch" }), "projects"),
        (ModelKind::Service, json!({ "name": "Sync" }), "services"),
        (ModelKind::Label, json!({ "name": "Bug" }), "labels"),
        (
            ModelKind::Initiative,
            json!({ "name": "Grow" }),
            "initiatives",
        ),
    ];
    for (model, data, plural) in records_of {
        // Members create and edit them, the admin's and their own.
        let (admins, own) = (Uuid::now_v7(), Uuid::now_v7());
        apply(&pool, f.actor, vec![create(model, admins, data.clone())])
            .await
            .unwrap();
        apply(&pool, bob, vec![create(model, own, data.clone())])
            .await
            .unwrap();
        let field = if data.get("title").is_some() {
            "title"
        } else {
            "name"
        };
        apply(
            &pool,
            bob,
            vec![update(model, admins, json!({ field: "Renamed" }))],
        )
        .await
        .unwrap();

        // But don't delete them, not even their own.
        for id in [admins, own] {
            forbidden(
                apply(&pool, bob, vec![delete(model, id)]).await,
                &format!("Only admins can delete {plural}."),
            );
        }
        assert_eq!(records(&pool, f.actor, model).await.len(), 2, "{model:?}");

        apply(
            &pool,
            f.actor,
            vec![delete(model, admins), delete(model, own)],
        )
        .await
        .unwrap();
        assert!(records(&pool, f.actor, model).await.is_empty(), "{model:?}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_change_team_settings(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let state = |id, name: &str| {
        create(
            ModelKind::WorkflowState,
            id,
            json!({ "team_id": f.team, "name": name, "type": "started", "position": 10 }),
        )
    };

    forbidden(
        apply(
            &pool,
            bob,
            vec![update(ModelKind::Team, f.team, json!({ "name": "Mine" }))],
        )
        .await,
        "Only admins can change a team's settings.",
    );
    let statuses = "Only admins can change workflow statuses.";
    forbidden(
        apply(&pool, bob, vec![state(Uuid::now_v7(), "Testing")]).await,
        statuses,
    );
    let todo = f.state("Todo");
    forbidden(
        apply(
            &pool,
            bob,
            vec![update(
                ModelKind::WorkflowState,
                todo,
                json!({ "name": "Next" }),
            )],
        )
        .await,
        statuses,
    );
    let testing = Uuid::now_v7();
    apply(&pool, f.actor, vec![state(testing, "Testing")])
        .await
        .unwrap();
    forbidden(
        apply(&pool, bob, vec![delete(ModelKind::WorkflowState, testing)]).await,
        statuses,
    );
    let states = records(&pool, f.actor, ModelKind::WorkflowState).await;
    assert!(states.iter().any(|s| s["id"] == json!(testing)));
    assert!(states.iter().any(|s| s["name"] == "Todo"));

    // Admins do.
    apply(
        &pool,
        f.actor,
        vec![
            update(ModelKind::Team, f.team, json!({ "name": "Renamed" })),
            update(ModelKind::WorkflowState, todo, json!({ "name": "Next" })),
            delete(ModelKind::WorkflowState, testing),
        ],
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_servers_own_writes_follow_no_member_rules(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let spec = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Spec,
            spec,
            json!({ "title": "How sync works" }),
        )],
    )
    .await
    .unwrap();
    let admins = new_issue(&pool, &f, json!({})).await;

    // Such as a change taking effect when Bob finishes an issue, or the
    // GitHub agent: written as him, but by Forge.
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        bob,
        &tx(vec![
            update(ModelKind::Team, f.team, json!({ "name": "Renamed" })),
            create(
                ModelKind::WorkflowState,
                Uuid::now_v7(),
                json!({ "team_id": f.team, "name": "Testing", "type": "started" }),
            ),
            delete(ModelKind::Spec, spec),
            delete(ModelKind::Issue, admins),
        ]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
    assert!(records(&pool, f.actor, ModelKind::Spec).await.is_empty());
    assert!(issues(&pool, f.actor).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn clients_cannot_create_teams_or_change_users(pool: PgPool) {
    let f = seed(&pool).await;

    invalid(
        apply(
            &pool,
            f.actor,
            vec![create(
                ModelKind::Team,
                Uuid::now_v7(),
                json!({ "name": "X", "key": "X" }),
            )],
        )
        .await,
    );
    invalid(apply(&pool, f.actor, vec![delete(ModelKind::Team, f.team)]).await);
    invalid(
        apply(
            &pool,
            f.actor,
            vec![update(
                ModelKind::User,
                f.actor.user_id,
                json!({ "name": "Me" }),
            )],
        )
        .await,
    );
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Team,
            f.team,
            json!({ "name": "Renamed" }),
        )],
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn server_recorded_changes_reach_the_log(pool: PgPool) {
    let f = seed(&pool).await;
    let users = records(&pool, f.actor, ModelKind::User).await;
    assert_eq!(users.len(), 1);

    let mut db = pool.begin().await.unwrap();
    let sync_id = record_change(
        &mut db,
        f.actor,
        ModelKind::User,
        f.actor.user_id,
        SyncAction::Update,
        Some(users[0].clone()),
    )
    .await
    .unwrap();
    db.commit().await.unwrap();

    let last = log(&pool, f.actor).await.pop().unwrap();
    assert_eq!(
        (last.sync_id, last.model, last.action),
        (sync_id, ModelKind::User, SyncAction::Update)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspaces_cannot_see_or_touch_each_other(pool: PgPool) {
    let alice = seed(&pool).await;
    let bob = seed(&pool).await;
    let id = new_issue(&pool, &alice, json!({ "title": "Alice's" })).await;

    let result = apply(
        &pool,
        bob.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "title": "Bob was here" }),
        )],
    )
    .await;
    assert!(matches!(result, Err(SyncError::NotFound(_))), "{result:?}");
    // Bob can't put an issue in Alice's team either.
    let result = apply(
        &pool,
        bob.actor,
        vec![create(
            ModelKind::Issue,
            Uuid::now_v7(),
            json!({ "team_id": alice.team, "title": "Sneaky" }),
        )],
    )
    .await;
    invalid(result);

    assert!(issues(&pool, bob.actor).await.is_empty());
    assert!(
        log(&pool, bob.actor)
            .await
            .iter()
            .all(|e| e.model != ModelKind::Issue)
    );
    assert_eq!(issue(&pool, alice.actor, id).await.title, "Alice's");
}

/// Another member of the fixture's workspace (not an admin).
async fn add_person(pool: &PgPool, f: &Fixture, name: &str) -> Actor {
    let person = Actor {
        user_id: Uuid::now_v7(),
        workspace_id: f.actor.workspace_id,
    };
    sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $1::text, $2)")
        .bind(person.user_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    add_member(pool, person.workspace_id, person.user_id, "member").await;
    person
}

async fn notifications(pool: &PgPool, viewer: Actor) -> Vec<Value> {
    records(pool, viewer, ModelKind::Notification).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn assigning_someone_notifies_only_them(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let id = new_issue(&pool, &f, json!({})).await;

    let sync_id = apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "assignee_id": bob.user_id }),
        )],
    )
    .await
    .unwrap()
    .unwrap();

    let for_bob = notifications(&pool, bob).await;
    assert_eq!(for_bob.len(), 1);
    assert_eq!(for_bob[0]["type"], "assigned");
    assert_eq!(for_bob[0]["issue_id"], json!(id));
    assert_eq!(for_bob[0]["actor_id"], json!(f.actor.user_id));
    assert!(notifications(&pool, f.actor).await.is_empty());

    // Ada's feed skips Bob's notification but still reaches the
    // transaction's last sync id, which confirms her change.
    let (entries, upto) = entries_after(&pool, f.actor, 0, 1000).await.unwrap();
    assert!(entries.iter().all(|e| e.model != ModelKind::Notification));
    assert_eq!(upto, sync_id);
    let (entries, _) = entries_after(&pool, bob, 0, 1000).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.model == ModelKind::Notification)
            .count(),
        1
    );

    // Assigning yourself notifies no one.
    let mine = new_issue(&pool, &f, json!({ "assignee_id": f.actor.user_id })).await;
    assert!(
        notifications(&pool, f.actor)
            .await
            .iter()
            .all(|n| n["issue_id"] != json!(mine))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn comments_notify_everyone_involved_but_the_author(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let cy = add_person(&pool, &f, "Cy").await;
    let id = new_issue(&pool, &f, json!({ "assignee_id": bob.user_id })).await;

    apply(
        &pool,
        cy,
        vec![create(
            ModelKind::Comment,
            Uuid::now_v7(),
            json!({ "issue_id": id, "body": "Hi" }),
        )],
    )
    .await
    .unwrap();

    let types = |n: Vec<Value>| {
        n.into_iter()
            .map(|n| n["type"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(types(notifications(&pool, f.actor).await), ["comment"]);
    assert_eq!(
        types(notifications(&pool, bob).await),
        ["assigned", "comment"]
    );
    assert!(notifications(&pool, cy).await.is_empty());

    // Cy now takes part, so Ada's reply reaches Cy and Bob.
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Comment,
            Uuid::now_v7(),
            json!({ "issue_id": id, "body": "Yo" }),
        )],
    )
    .await
    .unwrap();
    assert_eq!(types(notifications(&pool, cy).await), ["comment"]);
    assert_eq!(
        types(notifications(&pool, bob).await),
        ["assigned", "comment", "comment"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn mentions_notify_the_people_mentioned_once(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob Stone").await;
    let cy = add_person(&pool, &f, "Cy Young").await;
    let id = new_issue(&pool, &f, json!({ "description": "For @Bob" })).await;
    let types = |n: Vec<Value>| {
        n.into_iter()
            .map(|n| n["type"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(types(notifications(&pool, bob).await), ["mention"]);

    // Bob's mention stays in the description, so editing it doesn't repeat it.
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "description": "For @Bob, with @Cy Young" }),
        )],
    )
    .await
    .unwrap();
    assert_eq!(types(notifications(&pool, bob).await), ["mention"]);
    assert_eq!(types(notifications(&pool, cy).await), ["mention"]);

    // In a comment, a mention replaces the usual comment notification, and
    // the author never notifies themselves.
    let comment = Uuid::now_v7();
    apply(
        &pool,
        cy,
        vec![create(
            ModelKind::Comment,
            comment,
            json!({ "issue_id": id, "body": "@bob stone and @Cy Young: done" }),
        )],
    )
    .await
    .unwrap();
    let for_bob = notifications(&pool, bob).await;
    assert_eq!(types(for_bob.clone()), ["mention", "mention"]);
    assert_eq!(for_bob[1]["comment_id"], json!(comment));
    assert_eq!(types(notifications(&pool, cy).await), ["mention"]);
    assert_eq!(types(notifications(&pool, f.actor).await), ["comment"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn completing_an_issue_notifies_its_creator(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let id = new_issue(&pool, &f, json!({})).await;

    apply(
        &pool,
        bob,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();

    let for_ada = notifications(&pool, f.actor).await;
    assert_eq!(for_ada.len(), 1);
    assert_eq!(for_ada[0]["type"], "completed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_recipient_can_read_or_delete_a_notification(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let id = new_issue(&pool, &f, json!({ "assignee_id": bob.user_id })).await;
    let notification: Uuid = notifications(&pool, bob).await[0]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let read = json!({ "read_at": "2026-10-14T08:00:00Z" });
    assert!(matches!(
        apply(
            &pool,
            f.actor,
            vec![update(ModelKind::Notification, notification, read.clone())]
        )
        .await,
        Err(SyncError::NotFound(_))
    ));
    invalid(
        apply(
            &pool,
            f.actor,
            vec![create(
                ModelKind::Notification,
                Uuid::now_v7(),
                json!({ "user_id": bob.user_id, "type": "assigned" }),
            )],
        )
        .await,
    );

    apply(
        &pool,
        bob,
        vec![update(ModelKind::Notification, notification, read)],
    )
    .await
    .unwrap();
    assert_eq!(
        notifications(&pool, bob).await[0]["read_at"],
        "2026-10-14T08:00:00+00:00"
    );

    // Deleting the issue takes its notifications with it, through the log.
    apply(&pool, f.actor, vec![delete(ModelKind::Issue, id)])
        .await
        .unwrap();
    assert!(notifications(&pool, bob).await.is_empty());
    let (entries, _) = entries_after(&pool, bob, 0, 1000).await.unwrap();
    let last_for_bob = entries
        .iter()
        .rev()
        .find(|e| e.model == ModelKind::Notification)
        .unwrap();
    assert_eq!(
        (last_for_bob.model_id, last_for_bob.action),
        (notification, SyncAction::Delete)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn specs_and_decisions_record_their_author(pool: PgPool) {
    let f = seed(&pool).await;
    let service = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Service,
                service,
                json!({ "name": "Sync", "paths": ["crates/sync/**"] }),
            ),
            create(
                ModelKind::Spec,
                Uuid::now_v7(),
                json!({ "title": "How sync works", "service_ids": [service] }),
            ),
            create(
                ModelKind::Decision,
                Uuid::now_v7(),
                json!({ "title": "Build our own sync", "service_id": service }),
            ),
        ],
    )
    .await
    .unwrap();

    for model in [ModelKind::Spec, ModelKind::Decision] {
        let record = &records(&pool, f.actor, model).await[0];
        assert_eq!(record["created_by"], json!(f.actor.user_id), "{model:?}");
    }
    let spec = &records(&pool, f.actor, ModelKind::Spec).await[0];
    assert_eq!(spec["service_ids"], json!([service]));
    let decision = &records(&pool, f.actor, ModelKind::Decision).await[0];
    assert_eq!(decision["service_id"], json!(service));
    let stored = &records(&pool, f.actor, ModelKind::Service).await[0];
    assert_eq!(stored["paths"], json!(["crates/sync/**"]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_service_detaches_what_pointed_at_it(pool: PgPool) {
    let f = seed(&pool).await;
    let (service, payments) = (Uuid::now_v7(), Uuid::now_v7());
    let (spec, decision, newer) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(ModelKind::Service, service, json!({ "name": "Billing" })),
            create(ModelKind::Service, payments, json!({ "name": "Payments" })),
            create(
                ModelKind::Spec,
                spec,
                json!({ "title": "Invoices", "service_ids": [service, payments] }),
            ),
            create(ModelKind::Decision, newer, json!({ "title": "Use Stripe" })),
            create(
                ModelKind::Decision,
                decision,
                json!({ "title": "Use PayPal", "service_id": service, "superseded_by_id": newer, "status": "superseded" }),
            ),
        ],
    )
    .await
    .unwrap();
    let issue = new_issue(&pool, &f, json!({ "service_id": service })).await;
    // A drift check of it, and what it suggested, written by the server.
    let (run, suggestion) = (Uuid::now_v7(), Uuid::now_v7());
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        f.actor,
        &tx(vec![
            create(
                ModelKind::AiRun,
                run,
                json!({ "service_id": service, "trigger": "manual" }),
            ),
            create(
                ModelKind::Suggestion,
                suggestion,
                json!({ "title": "Close it", "body": "", "changes": [], "operations": [], "service_id": service, "issue_id": issue, "decision_id": decision, "run_id": run }),
            ),
        ]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();

    apply(&pool, f.actor, vec![delete(ModelKind::Service, service)])
        .await
        .unwrap();
    assert_eq!(issue_of(&pool, f.actor, issue).await.service_id, None);
    assert_eq!(
        records(&pool, f.actor, ModelKind::AiRun).await[0]["service_id"],
        Value::Null
    );
    assert_eq!(
        records(&pool, f.actor, ModelKind::Suggestion).await[0]["service_id"],
        Value::Null
    );
    // The spec stays with its other service.
    for record in records(&pool, f.actor, ModelKind::Spec).await {
        assert_eq!(record["service_ids"], json!([payments]));
    }

    apply(&pool, f.actor, vec![delete(ModelKind::Decision, newer)])
        .await
        .unwrap();
    let remaining = &records(&pool, f.actor, ModelKind::Decision).await[0];
    assert_eq!(remaining["superseded_by_id"], Value::Null);

    // The suggestion was about these too; it stays, pointing at nothing.
    apply(
        &pool,
        f.actor,
        vec![
            delete(ModelKind::Decision, decision),
            delete(ModelKind::Issue, issue),
        ],
    )
    .await
    .unwrap();
    let left = &records(&pool, f.actor, ModelKind::Suggestion).await[0];
    assert_eq!(left["issue_id"], Value::Null);
    assert_eq!(left["decision_id"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn services_from_another_workspace_cannot_be_referenced(pool: PgPool) {
    let alice = seed(&pool).await;
    let bob = seed(&pool).await;
    let service = Uuid::now_v7();
    apply(
        &pool,
        alice.actor,
        vec![create(
            ModelKind::Service,
            service,
            json!({ "name": "Secret" }),
        )],
    )
    .await
    .unwrap();

    let result = apply(
        &pool,
        bob.actor,
        vec![create(
            ModelKind::Issue,
            Uuid::now_v7(),
            json!({ "team_id": bob.team, "title": "x", "service_id": service }),
        )],
    )
    .await;
    invalid(result);
}

async fn issue_of(pool: &PgPool, actor: Actor, id: Uuid) -> Issue {
    issue(pool, actor, id).await
}

const RATE_LIMITS: &str = "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429
";

/// Writes an issue's spec change the way the MCP tools do.
async fn write_spec_change(pool: &PgPool, f: &Fixture, issue: Uuid, data: Value) -> Uuid {
    let id = Uuid::now_v7();
    let mut data = object(data);
    data.insert("issue_id".into(), json!(issue));
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        f.actor,
        &tx(vec![Operation::Create {
            model: ModelKind::SpecChange,
            id,
            data,
        }]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
    id
}

fn by_id(records: &[Value], id: Uuid) -> Value {
    records
        .iter()
        .find(|r| r["id"] == json!(id))
        .cloned()
        .unwrap_or(Value::Null)
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_change_takes_effect_when_its_issue_is_done(pool: PgPool) {
    let f = seed(&pool).await;
    let (spec, overview, decision) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Spec,
                spec,
                json!({ "title": "Rate limits", "body": RATE_LIMITS }),
            ),
            create(
                ModelKind::Spec,
                overview,
                json!({ "title": "Overview", "body": "Prose." }),
            ),
            create(
                ModelKind::Decision,
                decision,
                json!({ "title": "D1: 100 a minute", "status": "accepted" }),
            ),
        ],
    )
    .await
    .unwrap();
    let issue = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
    let change = write_spec_change(
        &pool,
        &f,
        issue,
        json!({
            "specs": [
                {
                    "spec_id": spec,
                    "text": "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow 200 requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends a 201st request within a minute\n- **THEN** it gets 429\n",
                    "rewrite": false,
                },
                { "spec_id": overview, "text": "## Purpose\n\nAll of it, again.", "rewrite": true },
                { "new_title": "Quotas", "text": "## Purpose\n\nQuotas.", "rewrite": true },
            ],
            "replaces_decision_ids": [decision],
        }),
    )
    .await;

    // Not while it's only in review.
    let specs = records(&pool, f.actor, ModelKind::Spec).await;
    assert_eq!(by_id(&specs, spec)["body"], RATE_LIMITS);

    let before = log(&pool, f.actor).await.last().unwrap().sync_id;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            issue,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();

    let specs = records(&pool, f.actor, ModelKind::Spec).await;
    let changed = by_id(&specs, spec)["body"].as_str().unwrap().to_string();
    assert!(changed.contains("200 requests a minute"), "{changed}");
    assert!(
        changed.starts_with("## Purpose\n\nRate limits."),
        "{changed}"
    );
    assert_eq!(
        by_id(&specs, overview)["body"],
        "## Purpose\n\nAll of it, again."
    );
    let quotas = specs.iter().find(|s| s["title"] == "Quotas").unwrap();
    assert_eq!(quotas["body"], "## Purpose\n\nQuotas.");
    assert_eq!(quotas["status"], "active");
    let decisions = records(&pool, f.actor, ModelKind::Decision).await;
    assert_eq!(by_id(&decisions, decision)["status"], "superseded");

    let record = by_id(
        &records(&pool, f.actor, ModelKind::SpecChange).await,
        change,
    );
    assert!(record["applied_at"].is_string(), "{record}");
    assert_eq!(
        record["applied_spec_ids"],
        json!([spec, overview, quotas["id"]])
    );
    assert_eq!(record["not_applied"], json!([]));

    // Clients hear about every change, in the same transaction as the issue's.
    let entries: Vec<SyncEntry> = entries_after(&pool, f.actor, before, 100).await.unwrap().0;
    let models: Vec<ModelKind> = entries.iter().map(|e| e.model).collect();
    assert_eq!(
        models,
        [
            ModelKind::Issue,
            ModelKind::Spec,
            ModelKind::Spec,
            ModelKind::Spec,
            ModelKind::Decision,
            ModelKind::SpecChange
        ]
    );
    assert!(
        entries
            .iter()
            .all(|e| e.transaction_id == entries[0].transaction_id)
    );

    // Reopened and done again: it has already taken effect.
    for state in ["In Progress", "Done"] {
        apply(
            &pool,
            f.actor,
            vec![update(
                ModelKind::Issue,
                issue,
                json!({ "state_id": f.state(state) }),
            )],
        )
        .await
        .unwrap();
    }
    let specs = records(&pool, f.actor, ModelKind::Spec).await;
    assert_eq!(specs.iter().filter(|s| s["title"] == "Quotas").count(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_that_no_longer_fits_its_change_stays_as_it_is(pool: PgPool) {
    let f = seed(&pool).await;
    let (spec, other) = (Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Spec,
                spec,
                json!({ "title": "Rate limits", "body": RATE_LIMITS }),
            ),
            create(
                ModelKind::Spec,
                other,
                json!({ "title": "Other", "body": "Old." }),
            ),
        ],
    )
    .await
    .unwrap();
    let issue = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
    let change = write_spec_change(
        &pool,
        &f,
        issue,
        json!({
            "specs": [
                {
                    "spec_id": spec,
                    "text": "## REMOVED Requirements\n\n### Requirement: Tokens are limited\n**Reason**: No limits.\n",
                    "rewrite": false,
                },
                { "spec_id": other, "text": "New.", "rewrite": true },
            ],
        }),
    )
    .await;
    // Someone renames the requirement in the meantime.
    let renamed = RATE_LIMITS.replace("Tokens are limited", "Tokens are capped");
    apply(
        &pool,
        f.actor,
        vec![update(ModelKind::Spec, spec, json!({ "body": renamed }))],
    )
    .await
    .unwrap();

    // Canceling it changes nothing; doing it applies what still fits.
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            issue,
            json!({ "state_id": f.state("Canceled") }),
        )],
    )
    .await
    .unwrap();
    let specs = records(&pool, f.actor, ModelKind::Spec).await;
    assert_eq!(by_id(&specs, other)["body"], "Old.");
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            issue,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();

    assert_eq!(issue_state(&pool, f.actor, issue).await, f.state("Done"));
    let specs = records(&pool, f.actor, ModelKind::Spec).await;
    assert_eq!(by_id(&specs, spec)["body"], json!(renamed));
    assert_eq!(by_id(&specs, other)["body"], "New.");
    let record = by_id(
        &records(&pool, f.actor, ModelKind::SpecChange).await,
        change,
    );
    assert_eq!(record["applied_spec_ids"], json!([other]));
    assert_eq!(
        record["not_applied"],
        json!([{
            "spec_id": spec,
            "title": "Rate limits",
            "reason": "It has no requirement \"Tokens are limited\" any more.",
        }])
    );
}

async fn issue_state(pool: &PgPool, actor: Actor, id: Uuid) -> Uuid {
    issue(pool, actor, id).await.state_id
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_belongs_only_to_services_that_exist(pool: PgPool) {
    let f = seed(&pool).await;
    let refused = apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Spec,
            Uuid::now_v7(),
            json!({ "title": "Invoices", "service_ids": [Uuid::now_v7()] }),
        )],
    )
    .await;
    assert!(
        matches!(&refused, Err(SyncError::Invalid(m)) if m == "service not found"),
        "{refused:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_domain_holds_specs_until_it_goes(pool: PgPool) {
    let f = seed(&pool).await;
    let (domain, spec) = (Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(ModelKind::Domain, domain, json!({ "name": "Accounts" })),
            create(
                ModelKind::Spec,
                spec,
                json!({ "title": "Invite links", "domain_id": domain }),
            ),
        ],
    )
    .await
    .unwrap();

    let twin = apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Domain,
            Uuid::now_v7(),
            json!({ "name": " accounts " }),
        )],
    )
    .await;
    assert!(matches!(twin, Err(SyncError::Invalid(_))), "{twin:?}");

    apply(&pool, f.actor, vec![delete(ModelKind::Domain, domain)])
        .await
        .unwrap();
    let left = &records(&pool, f.actor, ModelKind::Spec).await[0];
    assert_eq!(left["domain_id"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_change_that_took_effect_stays_as_it_was(pool: PgPool) {
    let f = seed(&pool).await;
    let (spec, decision) = (Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Spec,
                spec,
                json!({ "title": "Rate limits", "body": RATE_LIMITS }),
            ),
            create(
                ModelKind::Decision,
                decision,
                json!({ "title": "D1: 100 a minute", "status": "accepted" }),
            ),
        ],
    )
    .await
    .unwrap();
    let done = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
    let applied = write_spec_change(
        &pool,
        &f,
        done,
        json!({
            "specs": [{ "spec_id": spec, "text": "## Purpose\n\nAgain.", "rewrite": true }],
            "replaces_decision_ids": [decision],
        }),
    )
    .await;
    let open = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
    let pending = write_spec_change(
        &pool,
        &f,
        open,
        json!({ "specs": [], "no_change_reason": "Only a refactor." }),
    )
    .await;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            done,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();

    // Deleting what it changed or replaced, and then its issue, rewrites nothing.
    apply(
        &pool,
        f.actor,
        vec![
            Operation::Delete {
                model: ModelKind::Spec,
                id: spec,
            },
            Operation::Delete {
                model: ModelKind::Decision,
                id: decision,
            },
            Operation::Delete {
                model: ModelKind::Issue,
                id: done,
            },
            Operation::Delete {
                model: ModelKind::Issue,
                id: open,
            },
        ],
    )
    .await
    .unwrap();
    let changes = records(&pool, f.actor, ModelKind::SpecChange).await;
    let kept = by_id(&changes, applied);
    assert_eq!(kept["issue_id"], Value::Null, "kept, without its issue");
    assert_eq!(kept["specs"][0]["spec_id"], json!(spec));
    assert_eq!(kept["replaces_decision_ids"], json!([decision]));
    assert_eq!(
        by_id(&changes, pending),
        Value::Null,
        "never took effect: gone with its issue"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_specs_history_keeps_each_text_and_the_spec_change_behind_it(pool: PgPool) {
    let f = seed(&pool).await;
    let spec = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Spec,
            spec,
            json!({ "title": "Rate limits", "body": "## Purpose\n\nLimits." }),
        )],
    )
    .await
    .unwrap();
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Spec,
            spec,
            json!({ "body": RATE_LIMITS }),
        )],
    )
    .await
    .unwrap();
    // Retitling isn't a new version of its text.
    apply(
        &pool,
        f.actor,
        vec![update(ModelKind::Spec, spec, json!({ "title": "Limits" }))],
    )
    .await
    .unwrap();
    let issue = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
    let change = write_spec_change(
        &pool,
        &f,
        issue,
        json!({ "specs": [{ "spec_id": spec, "text": "## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts of 10.\n\n#### Scenario: Burst\n- **WHEN** a token sends 10 at once\n- **THEN** all pass\n", "rewrite": false }] }),
    )
    .await;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            issue,
            json!({ "state_id": f.state("Done") }),
        )],
    )
    .await
    .unwrap();

    let versions = spec_versions(&pool, f.actor, spec).await.unwrap().unwrap();
    let summary: Vec<(bool, bool, Option<Uuid>)> = versions
        .iter()
        .map(|v| (v.created, v.body.contains("Bursts"), v.spec_change_id))
        .collect();
    assert_eq!(
        summary,
        [
            (true, false, None),
            (false, false, None),
            (false, true, Some(change))
        ]
    );
    assert_eq!(versions[1].body, RATE_LIMITS);
    assert!(
        spec_versions(&pool, f.actor, Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn issues_done_together_each_name_their_own_spec_change_in_history(pool: PgPool) {
    let f = seed(&pool).await;
    let (one, two) = (Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Spec,
                one,
                json!({ "title": "One", "body": RATE_LIMITS }),
            ),
            create(
                ModelKind::Spec,
                two,
                json!({ "title": "Two", "body": RATE_LIMITS }),
            ),
        ],
    )
    .await
    .unwrap();
    let added = "## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts.\n\n#### Scenario: Burst\n- **WHEN** w\n- **THEN** t\n";
    let mut changes = Vec::new();
    let mut issues = Vec::new();
    for spec in [one, two] {
        let issue = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
        let change = write_spec_change(
            &pool,
            &f,
            issue,
            json!({ "specs": [{ "spec_id": spec, "text": added, "rewrite": false }] }),
        )
        .await;
        issues.push(issue);
        changes.push(change);
    }
    // Both done in one transaction, as the app's bulk edit does.
    apply(
        &pool,
        f.actor,
        issues
            .iter()
            .map(|issue| {
                update(
                    ModelKind::Issue,
                    *issue,
                    json!({ "state_id": f.state("Done") }),
                )
            })
            .collect(),
    )
    .await
    .unwrap();
    for (spec, change) in [one, two].into_iter().zip(changes) {
        let versions = spec_versions(&pool, f.actor, spec).await.unwrap().unwrap();
        assert_eq!(versions.last().unwrap().spec_change_id, Some(change));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn issues_done_together_on_one_spec_each_name_their_own_version(pool: PgPool) {
    let f = seed(&pool).await;
    let spec = Uuid::now_v7();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Spec,
            spec,
            json!({ "title": "One", "body": RATE_LIMITS }),
        )],
    )
    .await
    .unwrap();
    let mut changes = Vec::new();
    let mut issues = Vec::new();
    for name in ["Bursts", "Quotas"] {
        let issue = new_issue(&pool, &f, json!({ "state_id": f.state("In Review") })).await;
        let text = format!(
            "## ADDED Requirements\n\n### Requirement: {name}\nThe API SHALL have {name}.\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n"
        );
        let change = write_spec_change(
            &pool,
            &f,
            issue,
            json!({ "specs": [{ "spec_id": spec, "text": text, "rewrite": false }] }),
        )
        .await;
        issues.push(issue);
        changes.push(change);
    }
    apply(
        &pool,
        f.actor,
        issues
            .iter()
            .map(|issue| {
                update(
                    ModelKind::Issue,
                    *issue,
                    json!({ "state_id": f.state("Done") }),
                )
            })
            .collect(),
    )
    .await
    .unwrap();
    let versions = spec_versions(&pool, f.actor, spec).await.unwrap().unwrap();
    let linked: Vec<Option<Uuid>> = versions.iter().map(|v| v.spec_change_id).collect();
    assert_eq!(linked, [None, Some(changes[0]), Some(changes[1])]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_spec_is_not_a_change_after_work_started(pool: PgPool) {
    let f = seed(&pool).await;
    let (kept, gone) = (Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        f.actor,
        vec![
            create(
                ModelKind::Spec,
                kept,
                json!({ "title": "Kept", "body": RATE_LIMITS }),
            ),
            create(
                ModelKind::Spec,
                gone,
                json!({ "title": "Gone", "body": RATE_LIMITS }),
            ),
        ],
    )
    .await
    .unwrap();
    let issue = new_issue(&pool, &f, json!({ "state_id": f.state("Todo") })).await;
    let added = "## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts.\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n";
    let change = write_spec_change(
        &pool,
        &f,
        issue,
        json!({ "specs": [
            { "spec_id": kept, "text": added, "rewrite": false },
            { "spec_id": gone, "text": added, "rewrite": false },
        ] }),
    )
    .await;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            issue,
            json!({ "state_id": f.state("In Progress") }),
        )],
    )
    .await
    .unwrap();
    apply(
        &pool,
        f.actor,
        vec![Operation::Delete {
            model: ModelKind::Spec,
            id: gone,
        }],
    )
    .await
    .unwrap();
    let record: forge_domain::SpecChange = serde_json::from_value(by_id(
        &records(&pool, f.actor, ModelKind::SpecChange).await,
        change,
    ))
    .unwrap();
    assert_eq!(record.specs.len(), 1);
    assert!(!record.changed_after_start(), "{record:?}");
    // Its history goes with it.
    assert!(spec_versions(&pool, f.actor, gone).await.unwrap().is_none());
}

/// An agent in the fixture's workspace.
async fn add_agent(pool: &PgPool, f: &Fixture, name: &str) -> Actor {
    let agent = Actor {
        user_id: Uuid::now_v7(),
        workspace_id: f.actor.workspace_id,
    };
    sqlx::query("INSERT INTO users (id, email, name, kind) VALUES ($1, $1::text, $2, 'agent')")
        .bind(agent.user_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    add_member(pool, agent.workspace_id, agent.user_id, "member").await;
    agent
}

async fn move_to(pool: &PgPool, actor: Actor, id: Uuid, state: Uuid) {
    apply(
        pool,
        actor,
        vec![update(ModelKind::Issue, id, json!({ "state_id": state }))],
    )
    .await
    .unwrap();
}

async fn reviews(pool: &PgPool, viewer: Actor, id: Uuid) -> Vec<Value> {
    notifications(pool, viewer)
        .await
        .into_iter()
        .filter(|n| n["type"] == "review" && n["issue_id"] == json!(id))
        .collect()
}

async fn read_all(pool: &PgPool, viewer: Actor) {
    for n in notifications(pool, viewer).await {
        let id: Uuid = n["id"].as_str().unwrap().parse().unwrap();
        apply(
            pool,
            viewer,
            vec![update(
                ModelKind::Notification,
                id,
                json!({ "read_at": "2026-10-18T08:00:00Z" }),
            )],
        )
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn entering_review_asks_whoever_accepted_the_issue_out_of_triage(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    let id = new_issue(&pool, &f, json!({ "state_id": f.state("Triage") })).await;
    move_to(&pool, bob, id, f.state("Todo")).await;
    move_to(&pool, claude, id, f.state("In Progress")).await;

    move_to(&pool, claude, id, f.state("In Review")).await;

    assert_eq!(
        issue(&pool, f.actor, id).await.reviewer_id,
        Some(bob.user_id)
    );
    let for_bob = reviews(&pool, bob, id).await;
    assert_eq!(for_bob.len(), 1);
    assert_eq!(for_bob[0]["actor_id"], json!(claude.user_id));
    assert!(reviews(&pool, f.actor, id).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_an_accepter_the_creator_reviews_if_a_person_made_it(pool: PgPool) {
    let f = seed(&pool).await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    let id = new_issue(&pool, &f, json!({ "state_id": f.state("Todo") })).await;

    move_to(&pool, claude, id, f.state("In Review")).await;

    assert_eq!(
        issue(&pool, f.actor, id).await.reviewer_id,
        Some(f.actor.user_id)
    );
    assert_eq!(reviews(&pool, f.actor, id).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_nobody_to_ask_the_admins_who_are_people_hear(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    sqlx::query("UPDATE workspace_members SET role = 'admin' WHERE user_id = $1")
        .bind(claude.user_id)
        .execute(&pool)
        .await
        .unwrap();
    let id = Uuid::now_v7();
    apply(
        &pool,
        claude,
        vec![create(
            ModelKind::Issue,
            id,
            json!({ "team_id": f.team, "title": "Agent's own", "state_id": f.state("Todo") }),
        )],
    )
    .await
    .unwrap();

    move_to(&pool, claude, id, f.state("In Review")).await;

    assert_eq!(issue(&pool, f.actor, id).await.reviewer_id, None);
    assert_eq!(reviews(&pool, f.actor, id).await.len(), 1);
    assert!(reviews(&pool, bob, id).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_chosen_reviewer_stays_and_moving_it_yourself_tells_nobody(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let id = new_issue(
        &pool,
        &f,
        json!({ "state_id": f.state("Todo"), "reviewer_id": bob.user_id }),
    )
    .await;

    move_to(&pool, bob, id, f.state("In Review")).await;

    assert_eq!(
        issue(&pool, f.actor, id).await.reviewer_id,
        Some(bob.user_id)
    );
    assert!(reviews(&pool, bob, id).await.is_empty());
    assert!(reviews(&pool, f.actor, id).await.is_empty());
}

/// Forge's GitHub agent, which bounces an issue out of review when its pull
/// request runs into conflicts or failing checks, and returns it.
async fn add_github_agent(pool: &PgPool, f: &Fixture) -> Actor {
    let agent = Actor {
        user_id: Uuid::now_v7(),
        workspace_id: f.actor.workspace_id,
    };
    sqlx::query("INSERT INTO users (id, email, name, kind) VALUES ($1, $2, 'GitHub', 'agent')")
        .bind(agent.user_id)
        .bind(format!(
            "github@{}.{}",
            agent.workspace_id,
            forge_domain::AGENT_EMAIL_DOMAIN
        ))
        .execute(pool)
        .await
        .unwrap();
    add_member(pool, agent.workspace_id, agent.user_id, "member").await;
    agent
}

/// A move Forge makes itself, as the GitHub agent does.
async fn forge_moves(pool: &PgPool, github: Actor, id: Uuid, state: Uuid) {
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        github,
        &tx(vec![update(
            ModelKind::Issue,
            id,
            json!({ "state_id": state }),
        )]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_notifications_do_not_repeat(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let carol = add_person(&pool, &f, "Carol").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    let github = add_github_agent(&pool, &f).await;
    let id = new_issue(
        &pool,
        &f,
        json!({ "state_id": f.state("In Progress"), "reviewer_id": bob.user_id }),
    )
    .await;
    move_to(&pool, claude, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 1);

    // Forge bounces it for failing checks and returns it while Bob hasn't
    // read it: still one.
    forge_moves(&pool, github, id, f.state("In Progress")).await;
    forge_moves(&pool, github, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 1);

    // Edits while it stays in review send nothing.
    read_all(&pool, bob).await;
    apply(
        &pool,
        claude,
        vec![update(ModelKind::Issue, id, json!({ "title": "Better" }))],
    )
    .await
    .unwrap();
    assert_eq!(reviews(&pool, bob, id).await.len(), 1);

    // Once Bob has read it, a bounce and return asks again.
    forge_moves(&pool, github, id, f.state("In Progress")).await;
    forge_moves(&pool, github, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 2);

    // Sent back and resubmitted: a fresh one, though the last is unread.
    move_to(&pool, bob, id, f.state("In Progress")).await;
    move_to(&pool, claude, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 3);

    // Whoever sends it back: here the agent itself, after a bounce.
    forge_moves(&pool, github, id, f.state("In Progress")).await;
    forge_moves(&pool, github, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 3);
    move_to(&pool, claude, id, f.state("In Progress")).await;
    move_to(&pool, claude, id, f.state("In Review")).await;
    assert_eq!(reviews(&pool, bob, id).await.len(), 4);

    // A new reviewer during review hears; the old one doesn't.
    read_all(&pool, bob).await;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "reviewer_id": carol.user_id }),
        )],
    )
    .await
    .unwrap();
    assert_eq!(reviews(&pool, carol, id).await.len(), 1);
    assert_eq!(reviews(&pool, bob, id).await.len(), 4);

    // Outside review, choosing a reviewer tells nobody yet.
    let later = new_issue(&pool, &f, json!({})).await;
    apply(
        &pool,
        f.actor,
        vec![update(
            ModelKind::Issue,
            later,
            json!({ "reviewer_id": bob.user_id }),
        )],
    )
    .await
    .unwrap();
    assert!(reviews(&pool, bob, later).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_reviewer_hears_though_an_older_request_is_unread(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let carol = add_person(&pool, &f, "Carol").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    let id = new_issue(
        &pool,
        &f,
        json!({ "state_id": f.state("In Progress"), "reviewer_id": bob.user_id }),
    )
    .await;
    move_to(&pool, claude, id, f.state("In Review")).await;
    let choose = |reviewer: Actor| {
        apply(
            &pool,
            f.actor,
            vec![update(
                ModelKind::Issue,
                id,
                json!({ "reviewer_id": reviewer.user_id }),
            )],
        )
    };

    // Carol, then Bob again, neither having read theirs.
    choose(carol).await.unwrap();
    choose(bob).await.unwrap();

    assert_eq!(reviews(&pool, bob, id).await.len(), 2);
    assert_eq!(reviews(&pool, carol, id).await.len(), 1);
    choose(carol).await.unwrap();
    assert_eq!(reviews(&pool, carol, id).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn accepting_straight_into_review_makes_you_the_reviewer(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let carol = add_person(&pool, &f, "Carol").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    // Carol accepted it once before it went back to Triage.
    let id = new_issue(&pool, &f, json!({ "state_id": f.state("Triage") })).await;
    move_to(&pool, carol, id, f.state("Todo")).await;
    move_to(&pool, carol, id, f.state("Triage")).await;

    move_to(&pool, bob, id, f.state("In Review")).await;

    assert_eq!(
        issue(&pool, f.actor, id).await.reviewer_id,
        Some(bob.user_id)
    );
    // He moved it himself, so nobody hears.
    assert!(reviews(&pool, bob, id).await.is_empty());
    assert!(reviews(&pool, carol, id).await.is_empty());

    // An agent taking it straight into review isn't a reviewer: the creator is.
    let other = new_issue(&pool, &f, json!({ "state_id": f.state("Triage") })).await;
    move_to(&pool, claude, other, f.state("In Review")).await;
    assert_eq!(
        issue(&pool, f.actor, other).await.reviewer_id,
        Some(f.actor.user_id)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reviewer_is_a_person_and_hears_comments(pool: PgPool) {
    let f = seed(&pool).await;
    let bob = add_person(&pool, &f, "Bob").await;
    let claude = add_agent(&pool, &f, "Claude Code").await;
    let id = new_issue(&pool, &f, json!({})).await;

    invalid(
        apply(
            &pool,
            f.actor,
            vec![update(
                ModelKind::Issue,
                id,
                json!({ "reviewer_id": claude.user_id }),
            )],
        )
        .await,
    );

    // Agents may set it.
    apply(
        &pool,
        claude,
        vec![update(
            ModelKind::Issue,
            id,
            json!({ "reviewer_id": bob.user_id }),
        )],
    )
    .await
    .unwrap();
    apply(
        &pool,
        f.actor,
        vec![create(
            ModelKind::Comment,
            Uuid::now_v7(),
            json!({ "issue_id": id, "body": "Have a look" }),
        )],
    )
    .await
    .unwrap();
    assert!(
        notifications(&pool, bob)
            .await
            .iter()
            .any(|n| n["type"] == "comment" && n["issue_id"] == json!(id))
    );
}
