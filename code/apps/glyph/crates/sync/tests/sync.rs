//! The sync engine against a real Postgres. Each test gets a fresh database.

use glyph_domain::{Actor, ModelKind, Operation, SyncAction, Transaction};
use glyph_sync::{
    Origin, SyncError, apply_in, apply_transaction, bootstrap, entries_after, record_change,
    set_languages,
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A workspace with one admin, English as the source and Swedish.
async fn seed(pool: &PgPool) -> Actor {
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
    apply(
        pool,
        actor,
        vec![
            create(
                ModelKind::Language,
                json!({ "code": "en", "is_source": true }),
            ),
            create(ModelKind::Language, json!({ "code": "sv" })),
        ],
    )
    .await
    .unwrap();
    actor
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

/// Someone else in the actor's workspace.
async fn add_person(pool: &PgPool, actor: Actor, role: &str) -> Actor {
    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $1::text, 'Other')")
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
    add_member(pool, actor.workspace_id, user_id, role).await;
    Actor {
        user_id,
        workspace_id: actor.workspace_id,
    }
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

fn create(model: ModelKind, data: Value) -> Operation {
    create_with(model, Uuid::now_v7(), data)
}

fn create_with(model: ModelKind, id: Uuid, data: Value) -> Operation {
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

async fn records(pool: &PgPool, actor: Actor, model: ModelKind) -> Vec<Value> {
    bootstrap(pool, actor).await.unwrap().records[&model].clone()
}

async fn ids(pool: &PgPool, actor: Actor, model: ModelKind) -> Vec<Uuid> {
    records(pool, actor, model)
        .await
        .iter()
        .map(|r| r["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

async fn language_id(pool: &PgPool, actor: Actor, code: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM languages WHERE workspace_id = $1 AND code = $2")
        .bind(actor.workspace_id)
        .bind(code)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn new_translation(pool: &PgPool, actor: Actor, lang: &str, key: &str, value: Value) -> Uuid {
    let id = Uuid::now_v7();
    let mut data = object(value);
    data.insert("lang".into(), json!(lang));
    data.insert("key".into(), json!(key));
    data.entry("namespace").or_insert(json!("common"));
    data.entry("value").or_insert(json!("Text"));
    apply(
        pool,
        actor,
        vec![Operation::Create {
            model: ModelKind::Translation,
            id,
            data,
        }],
    )
    .await
    .unwrap();
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_translation_counts_as_written_by_hand(pool: PgPool) {
    let actor = seed(&pool).await;
    new_translation(&pool, actor, "sv", "hello", json!({ "value": "Hej" })).await;
    let all = records(&pool, actor, ModelKind::Translation).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["value"], "Hej");
    assert_eq!(all[0]["translated_by"], "human");
    assert_eq!(all[0]["confidence"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_a_models_text_makes_it_a_persons(pool: PgPool) {
    let actor = seed(&pool).await;
    let id = new_translation(
        &pool,
        actor,
        "sv",
        "hello",
        json!({ "value": "Hejsan", "translated_by": "viking", "confidence": 0.6 }),
    )
    .await;
    apply(
        &pool,
        actor,
        vec![update(
            ModelKind::Translation,
            id,
            json!({ "context": "Greeting" }),
        )],
    )
    .await
    .unwrap();
    let t = &records(&pool, actor, ModelKind::Translation).await[0];
    assert_eq!(
        t["translated_by"], "viking",
        "a context note leaves the author"
    );
    assert_eq!(t["confidence"], 0.6);

    apply(
        &pool,
        actor,
        vec![update(
            ModelKind::Translation,
            id,
            json!({ "value": "Hej" }),
        )],
    )
    .await
    .unwrap();
    let t = &records(&pool, actor, ModelKind::Translation).await[0];
    assert_eq!(t["value"], "Hej");
    assert_eq!(t["translated_by"], "human");
    assert_eq!(t["confidence"], Value::Null);
    assert_eq!(t["context"], "Greeting");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_key_exists_once_per_language_and_namespace(pool: PgPool) {
    let actor = seed(&pool).await;
    new_translation(&pool, actor, "sv", "hello", json!({})).await;
    let again = apply(
        &pool,
        actor,
        vec![create(
            ModelKind::Translation,
            json!({ "lang": "sv", "namespace": "common", "key": "hello", "value": "x" }),
        )],
    )
    .await;
    assert!(
        matches!(&again, Err(SyncError::Conflict(m)) if m == "That key already exists in this namespace."),
        "{again:?}"
    );
    // Another namespace or language is another key.
    new_translation(&pool, actor, "sv", "hello", json!({ "namespace": "auth" })).await;
    new_translation(&pool, actor, "en", "hello", json!({})).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn texts_need_a_language_the_workspace_has(pool: PgPool) {
    let actor = seed(&pool).await;
    let result = apply(
        &pool,
        actor,
        vec![create(
            ModelKind::Translation,
            json!({ "lang": "fi", "namespace": "common", "key": "hello", "value": "Hei" }),
        )],
    )
    .await;
    assert!(
        matches!(&result, Err(SyncError::Invalid(m)) if m == "Add that language to the workspace first."),
        "{result:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_one_language_is_the_source(pool: PgPool) {
    let actor = seed(&pool).await;
    let (en, sv) = (
        language_id(&pool, actor, "en").await,
        language_id(&pool, actor, "sv").await,
    );
    let two = apply(
        &pool,
        actor,
        vec![update(
            ModelKind::Language,
            sv,
            json!({ "is_source": true }),
        )],
    )
    .await;
    assert!(matches!(two, Err(SyncError::Conflict(_))), "{two:?}");
    apply(
        &pool,
        actor,
        vec![
            update(ModelKind::Language, en, json!({ "is_source": false })),
            update(ModelKind::Language, sv, json!({ "is_source": true })),
        ],
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_language_takes_its_texts_glossary_and_pairs_through_the_log(pool: PgPool) {
    let actor = seed(&pool).await;
    apply(
        &pool,
        actor,
        vec![create(ModelKind::Language, json!({ "code": "fi" }))],
    )
    .await
    .unwrap();
    let kept = new_translation(&pool, actor, "en", "hello", json!({})).await;
    let gone = new_translation(&pool, actor, "sv", "hello", json!({})).await;
    let (entry, pair, other_pair) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    apply(
        &pool,
        actor,
        vec![
            create_with(
                ModelKind::GlossaryEntry,
                entry,
                json!({ "source_lang": "en", "target_lang": "sv", "source_term": "email", "target_term": "e-post" }),
            ),
            create_with(ModelKind::LanguagePair, pair, json!({ "source_lang": "en", "target_lang": "sv" })),
            create_with(ModelKind::LanguagePair, other_pair, json!({ "source_lang": "en", "target_lang": "fi" })),
        ],
    )
    .await
    .unwrap();
    let before = glyph_sync::current_sync_id(&pool, actor.workspace_id)
        .await
        .unwrap();

    let sv = language_id(&pool, actor, "sv").await;
    apply(&pool, actor, vec![delete(ModelKind::Language, sv)])
        .await
        .unwrap();

    assert_eq!(ids(&pool, actor, ModelKind::Translation).await, vec![kept]);
    assert!(ids(&pool, actor, ModelKind::GlossaryEntry).await.is_empty());
    assert_eq!(
        ids(&pool, actor, ModelKind::LanguagePair).await,
        vec![other_pair]
    );

    let (entries, _) = entries_after(&pool, actor, before, 100).await.unwrap();
    let deleted: Vec<(ModelKind, Uuid)> = entries
        .iter()
        .map(|e| {
            assert_eq!(e.action, SyncAction::Delete);
            (e.model, e.model_id)
        })
        .collect();
    assert_eq!(
        deleted,
        vec![
            (ModelKind::Translation, gone),
            (ModelKind::GlossaryEntry, entry),
            (ModelKind::LanguagePair, pair),
            (ModelKind::Language, sv),
        ]
    );
    // All in one transaction, so clients apply them together.
    assert!(
        entries
            .iter()
            .all(|e| e.transaction_id == entries[0].transaction_id)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn glossary_terms_are_unique_whatever_their_case(pool: PgPool) {
    let actor = seed(&pool).await;
    let entry = |term: &str| {
        create(
            ModelKind::GlossaryEntry,
            json!({ "source_lang": "en", "target_lang": "sv", "source_term": term, "target_term": "e-post", "target_variants": ["e-posten"] }),
        )
    };
    apply(&pool, actor, vec![entry("email")]).await.unwrap();
    let again = apply(&pool, actor, vec![entry(" Email")]).await;
    assert!(
        matches!(&again, Err(SyncError::Conflict(m)) if m == "There's already a glossary entry for that term."),
        "{again:?}"
    );
    let stored = &records(&pool, actor, ModelKind::GlossaryEntry).await[0];
    assert_eq!(stored["target_variants"], json!(["e-posten"]));
    assert_eq!(stored["note"], "");
}

#[sqlx::test(migrations = "../../migrations")]
async fn members_translate_but_only_admins_manage_languages(pool: PgPool) {
    let actor = seed(&pool).await;
    let member = add_person(&pool, actor, "member").await;
    new_translation(&pool, member, "sv", "hello", json!({})).await;
    apply(
        &pool,
        member,
        vec![create(
            ModelKind::GlossaryEntry,
            json!({ "source_lang": "en", "target_lang": "sv", "source_term": "email", "target_term": "e-post" }),
        )],
    )
    .await
    .unwrap();

    let add = apply(
        &pool,
        member,
        vec![create(ModelKind::Language, json!({ "code": "fi" }))],
    )
    .await;
    assert!(
        matches!(&add, Err(SyncError::Forbidden(m)) if m == "Only admins can add or remove languages."),
        "{add:?}"
    );
    let sv = language_id(&pool, actor, "sv").await;
    let remove = apply(&pool, member, vec![delete(ModelKind::Language, sv)]).await;
    assert!(matches!(remove, Err(SyncError::Forbidden(_))), "{remove:?}");
    let pair = apply(
        &pool,
        member,
        vec![create(
            ModelKind::LanguagePair,
            json!({ "source_lang": "en", "target_lang": "sv" }),
        )],
    )
    .await;
    assert!(matches!(pair, Err(SyncError::Forbidden(_))), "{pair:?}");

    // The server's own writes follow no member rules.
    let mut db = pool.begin().await.unwrap();
    apply_in(
        &mut db,
        member,
        &tx(vec![create(ModelKind::Language, json!({ "code": "fi" }))]),
        Origin::Server,
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_retried_transaction_is_applied_once(pool: PgPool) {
    let actor = seed(&pool).await;
    let transaction = tx(vec![create(
        ModelKind::Translation,
        json!({ "lang": "sv", "namespace": "common", "key": "hello", "value": "Hej" }),
    )]);
    let first = apply_transaction(&pool, actor, &transaction).await.unwrap();
    let second = apply_transaction(&pool, actor, &transaction).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(records(&pool, actor, ModelKind::Translation).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_operation_rolls_back_the_whole_transaction(pool: PgPool) {
    let actor = seed(&pool).await;
    let before = glyph_sync::current_sync_id(&pool, actor.workspace_id)
        .await
        .unwrap();
    let result = apply(
        &pool,
        actor,
        vec![
            create(
                ModelKind::Translation,
                json!({ "lang": "sv", "namespace": "common", "key": "a", "value": "A" }),
            ),
            create(
                ModelKind::Translation,
                json!({ "lang": "sv", "namespace": "common", "key": "", "value": "B" }),
            ),
        ],
    )
    .await;
    assert!(matches!(result, Err(SyncError::Invalid(_))), "{result:?}");
    assert!(
        records(&pool, actor, ModelKind::Translation)
            .await
            .is_empty()
    );
    assert_eq!(
        glyph_sync::current_sync_id(&pool, actor.workspace_id)
            .await
            .unwrap(),
        before
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_input_is_rejected(pool: PgPool) {
    let actor = seed(&pool).await;
    for data in [
        json!({ "lang": "sv", "namespace": "common", "key": "a" }),
        json!({ "lang": "sv", "namespace": "a/b", "key": "a", "value": "" }),
        json!({ "lang": "sv", "namespace": "common", "key": "a", "value": "", "confidence": 2 }),
        json!({ "lang": "sv", "namespace": "common", "key": "a", "value": "", "workspace_id": Uuid::now_v7() }),
    ] {
        let result = apply(
            &pool,
            actor,
            vec![create(ModelKind::Translation, data.clone())],
        )
        .await;
        assert!(
            matches!(result, Err(SyncError::Invalid(_))),
            "{data} gave {result:?}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn updating_a_missing_record_fails_but_deleting_one_does_nothing(pool: PgPool) {
    let actor = seed(&pool).await;
    let missing = Uuid::now_v7();
    let result = apply(
        &pool,
        actor,
        vec![update(
            ModelKind::Translation,
            missing,
            json!({ "value": "x" }),
        )],
    )
    .await;
    assert!(matches!(result, Err(SyncError::NotFound(_))), "{result:?}");
    let result = apply(&pool, actor, vec![delete(ModelKind::Translation, missing)]).await;
    assert_eq!(result.unwrap(), None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn clients_cannot_change_users(pool: PgPool) {
    let actor = seed(&pool).await;
    let result = apply(
        &pool,
        actor,
        vec![update(
            ModelKind::User,
            actor.user_id,
            json!({ "name": "x" }),
        )],
    )
    .await;
    assert!(matches!(result, Err(SyncError::Invalid(_))), "{result:?}");
    assert_eq!(records(&pool, actor, ModelKind::User).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn server_recorded_changes_reach_the_log(pool: PgPool) {
    let actor = seed(&pool).await;
    let before = glyph_sync::current_sync_id(&pool, actor.workspace_id)
        .await
        .unwrap();
    let mut db = pool.begin().await.unwrap();
    record_change(
        &mut db,
        actor,
        ModelKind::User,
        actor.user_id,
        SyncAction::Update,
        Some(json!({ "id": actor.user_id })),
    )
    .await
    .unwrap();
    db.commit().await.unwrap();
    let (entries, upto) = entries_after(&pool, actor, before, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].model, ModelKind::User);
    assert_eq!(upto, entries[0].sync_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspaces_cannot_see_or_touch_each_other(pool: PgPool) {
    let ours = seed(&pool).await;
    let theirs = seed(&pool).await;
    let id = new_translation(&pool, theirs, "sv", "hello", json!({})).await;
    assert!(
        records(&pool, ours, ModelKind::Translation)
            .await
            .is_empty()
    );
    let result = apply(
        &pool,
        ours,
        vec![update(ModelKind::Translation, id, json!({ "value": "x" }))],
    )
    .await;
    assert!(matches!(result, Err(SyncError::NotFound(_))), "{result:?}");
    assert_eq!(
        apply(&pool, ours, vec![delete(ModelKind::Translation, id)])
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        records(&pool, theirs, ModelKind::Translation).await.len(),
        1
    );
    let (entries, _) = entries_after(&pool, ours, 0, 100).await.unwrap();
    assert!(entries.iter().all(|e| e.model != ModelKind::Translation));
}

#[sqlx::test(migrations = "../../migrations")]
async fn bootstrap_and_log_agree(pool: PgPool) {
    let actor = seed(&pool).await;
    new_translation(&pool, actor, "sv", "hello", json!({})).await;
    let snapshot = bootstrap(&pool, actor).await.unwrap();
    let (entries, upto) = entries_after(&pool, actor, 0, 100).await.unwrap();
    assert_eq!(snapshot.sync_id, upto);
    // Two languages and a translation.
    assert_eq!(entries.len(), 3);
    let (later, _) = entries_after(&pool, actor, snapshot.sync_id, 100)
        .await
        .unwrap();
    assert!(later.is_empty());
}

/// German too, a guest added to Swedish, and one added to German.
async fn with_guests(pool: &PgPool) -> (Actor, Actor, Actor) {
    let admin = seed(pool).await;
    apply(
        pool,
        admin,
        vec![create(ModelKind::Language, json!({ "code": "de" }))],
    )
    .await
    .unwrap();
    let gus = add_person(pool, admin, "guest").await;
    put_in_languages(pool, admin, gus, &["sv"]).await;
    let dee = add_person(pool, admin, "guest").await;
    put_in_languages(pool, admin, dee, &["de"]).await;
    (admin, gus, dee)
}

async fn put_in_languages(pool: &PgPool, admin: Actor, guest: Actor, codes: &[&str]) {
    let mut ids = Vec::new();
    for code in codes {
        ids.push(language_id(pool, admin, code).await);
    }
    let mut db = pool.begin().await.unwrap();
    set_languages(&mut db, admin, guest.user_id, &ids)
        .await
        .unwrap();
    db.commit().await.unwrap();
}

async fn codes(pool: &PgPool, viewer: Actor, model: ModelKind, field: &str) -> Vec<String> {
    let mut codes: Vec<String> = records(pool, viewer, model)
        .await
        .iter()
        .map(|r| r[field].as_str().unwrap().to_string())
        .collect();
    codes.sort();
    codes
}

fn glossary(source: &str, target: &str, term: &str) -> Operation {
    create(
        ModelKind::GlossaryEntry,
        json!({
            "source_lang": source,
            "target_lang": target,
            "source_term": term,
            "target_term": term,
        }),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_guest_syncs_the_source_and_their_languages(pool: PgPool) {
    let (admin, gus, dee) = with_guests(&pool).await;
    let bob = add_person(&pool, admin, "member").await;
    for lang in ["en", "sv", "de"] {
        new_translation(&pool, admin, lang, "hello", json!({})).await;
    }
    apply(
        &pool,
        admin,
        vec![glossary("en", "sv", "email"), glossary("en", "de", "email")],
    )
    .await
    .unwrap();

    assert_eq!(
        codes(&pool, gus, ModelKind::Language, "code").await,
        ["en", "sv"]
    );
    assert_eq!(
        codes(&pool, gus, ModelKind::Translation, "lang").await,
        ["en", "sv"]
    );
    assert_eq!(
        codes(&pool, gus, ModelKind::GlossaryEntry, "target_lang").await,
        ["sv"]
    );
    // The admin and the members, but not a guest in another language.
    let mut people = ids(&pool, gus, ModelKind::User).await;
    people.sort();
    let mut expected = vec![admin.user_id, gus.user_id, bob.user_id];
    expected.sort();
    assert_eq!(people, expected);
    assert!(!people.contains(&dee.user_id));
    // Members still sync everything.
    assert_eq!(
        codes(&pool, bob, ModelKind::Translation, "lang").await,
        ["de", "en", "sv"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn guests_translate_only_their_languages(pool: PgPool) {
    let (admin, gus, _) = with_guests(&pool).await;
    let english = new_translation(&pool, admin, "en", "hello", json!({})).await;
    let german = new_translation(&pool, admin, "de", "hello", json!({})).await;

    new_translation(&pool, gus, "sv", "hello", json!({ "value": "Hej" })).await;
    apply(&pool, gus, vec![glossary("en", "sv", "email")])
        .await
        .unwrap();

    let source = apply(
        &pool,
        gus,
        vec![update(
            ModelKind::Translation,
            english,
            json!({ "value": "Hi" }),
        )],
    )
    .await;
    assert!(matches!(source, Err(SyncError::Forbidden(_))), "{source:?}");
    let new_key = apply(
        &pool,
        gus,
        vec![create(
            ModelKind::Translation,
            json!({ "lang": "en", "namespace": "common", "key": "bye", "value": "Bye" }),
        )],
    )
    .await;
    assert!(
        matches!(new_key, Err(SyncError::Forbidden(_))),
        "{new_key:?}"
    );
    // German isn't there for them at all.
    let other = apply(&pool, gus, vec![delete(ModelKind::Translation, german)]).await;
    assert!(matches!(other, Err(SyncError::NotFound(_))), "{other:?}");
    let language = apply(
        &pool,
        gus,
        vec![create(ModelKind::Language, json!({ "code": "fi" }))],
    )
    .await;
    assert!(
        matches!(language, Err(SyncError::Forbidden(_))),
        "{language:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_guests_log_hides_other_languages_but_confirms_their_writes(pool: PgPool) {
    let (admin, gus, _) = with_guests(&pool).await;
    let (_, start) = entries_after(&pool, gus, 0, 1000).await.unwrap();
    let german = new_translation(&pool, admin, "de", "hello", json!({})).await;
    let swedish = Uuid::now_v7();
    let confirmed = apply(
        &pool,
        gus,
        vec![create_with(
            ModelKind::Translation,
            swedish,
            json!({ "lang": "sv", "namespace": "common", "key": "hello", "value": "Hej" }),
        )],
    )
    .await
    .unwrap()
    .unwrap();

    let (entries, upto) = entries_after(&pool, gus, start, 1000).await.unwrap();
    let seen: Vec<Uuid> = entries.iter().map(|e| e.model_id).collect();
    assert_eq!(seen, vec![swedish]);
    assert!(!seen.contains(&german));
    assert_eq!(upto, confirmed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_guests_copy_starts_over_when_their_languages_or_the_source_change(pool: PgPool) {
    let (admin, gus, _) = with_guests(&pool).await;
    let bob = add_person(&pool, admin, "member").await;
    let epoch = |viewer| {
        let pool = pool.clone();
        async move { bootstrap(&pool, viewer).await.unwrap().epoch }
    };
    let (shared, first) = (epoch(admin).await, epoch(gus).await);
    assert_eq!(epoch(bob).await, shared);
    assert_ne!(first, shared);

    put_in_languages(&pool, admin, gus, &["sv", "de"]).await;
    let second = epoch(gus).await;
    assert_ne!(second, first);
    assert_eq!(
        codes(&pool, gus, ModelKind::Language, "code").await,
        ["de", "en", "sv"]
    );

    put_in_languages(&pool, admin, gus, &["sv"]).await;
    let (en, de) = (
        language_id(&pool, admin, "en").await,
        language_id(&pool, admin, "de").await,
    );
    apply(
        &pool,
        admin,
        vec![
            update(ModelKind::Language, en, json!({ "is_source": false })),
            update(ModelKind::Language, de, json!({ "is_source": true })),
        ],
    )
    .await
    .unwrap();
    assert_ne!(epoch(gus).await, second);
    assert_eq!(
        codes(&pool, gus, ModelKind::Language, "code").await,
        ["de", "sv"]
    );
    // Nothing changes for everyone else.
    assert_eq!(epoch(admin).await, shared);
    assert_eq!(epoch(bob).await, shared);
}
