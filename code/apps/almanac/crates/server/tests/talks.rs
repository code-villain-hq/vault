//! Topics and talks, with a stand-in for Claude: a topic sets itself up,
//! an answer streams and is kept, and without a key the talk says where to
//! add one.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use almanac_server::claude::Claude;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;

/// What the stand-in was sent, and what it answers.
#[derive(Clone, Default)]
struct Fake {
    sent: Arc<Mutex<Vec<Value>>>,
}

async fn messages(State(fake): State<Fake>, Json(body): Json<Value>) -> Response {
    fake.sent.lock().unwrap().push(body.clone());
    if body["stream"] == true {
        // The letter ß split across two events' chunks stays whole.
        let events = [
            json!({ "type": "message_start", "message": {} }),
            json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Gieß die Monstera " } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "alle 7 Tage." } }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
            json!({ "type": "message_stop" }),
        ];
        let text: String = events
            .iter()
            .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
            .collect();
        return ([("content-type", "text/event-stream")], text).into_response();
    }
    let system = body["system"].as_str().unwrap_or_default();
    let answer = if system.starts_with("You keep the page") {
        json!({
            "blocks": [{ "kind": "paragraph", "title": "Pflege", "fields": [] }],
            "items": [
                { "block": "Pflege", "look": "", "text": "Alle 7 Tage gießen.", "source": "From the talk" },
            ],
            "rows": [
                { "block": "b2", "name": "Monstera", "cells": [], "source": "You said" },
            ],
            "changes": [],
            "remove": [],
            "ticks": [],
            "questions": [
                { "theme": "Monstera", "text": "Ist der Topf größer als 20 cm?", "point": "Topf über 20 cm." },
            ],
            "reminders": [
                { "theme": "Monstera", "title": "Monstera gießen", "item": "", "due_on": "2026-11-01", "at": "", "every_days": 7, "replaces": [] },
            ],
            "elsewhere": "",
            "talk_title": "Monstera gießen",
        })
    } else {
        json!({
            "sticks_to": "Die Zimmerpflanzen zu Hause: gießen, düngen, umtopfen.",
            "icon": "sprout",
            "group": "Home",
            "template": "collection",
            "cards": "Pflanzen",
            "starters": ["Wie oft gieße ich die Monstera?", "Wann topfe ich um?"],
        })
    };
    Json(json!({
        "type": "message",
        "role": "assistant",
        "content": [
            { "type": "thinking", "thinking": "", "signature": "" },
            { "type": "text", "text": answer.to_string() },
        ],
        "stop_reason": "end_turn",
    }))
    .into_response()
}

async fn stand_in() -> (String, Fake) {
    let fake = Fake::default();
    let router = axum::Router::new()
        .route("/v1/messages", post(messages))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    (format!("http://{address}/v1/messages"), fake)
}

/// Waits for the server's background work to show in the database.
async fn eventually(pool: &PgPool, query: &'static str) -> bool {
    for _ in 0..100 {
        let done: bool = sqlx::query_scalar(query).fetch_one(pool).await.unwrap();
        if done {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_topic_sets_itself_up_and_a_talk_keeps_what_it_learned(pool: PgPool) {
    let (url, fake) = stand_in().await;
    let state = common::state(pool.clone());
    let state = almanac_server::AppState {
        claude: Claude::new(Some("sk-test".into())).at(url),
        ..state
    };
    let app = App::with_state(state);
    let (ada, home) = app.signup("Ada", "Home").await;

    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    let topic = started.body["id"].as_str().unwrap().to_string();
    assert!(
        eventually(
            &pool,
            "SELECT EXISTS (SELECT 1 FROM topics WHERE icon = 'sprout')"
        )
        .await,
        "set up in the background"
    );
    let topics = app.synced(&ada, &home, "Topic").await;
    let plants = topics.iter().find(|t| t["name"] == "Pflanzen").unwrap();
    assert_eq!(plants["kind"], "topic");
    assert_eq!(plants["group_name"], "Home");
    // Its page starts as the template Claude picked, the collection named,
    // and You for About.
    let blocks = app.synced(&ada, &home, "Block").await;
    let mut blocks: Vec<_> = blocks
        .iter()
        .filter(|b| b["topic_id"] == plants["id"])
        .collect();
    blocks.sort_by_key(|b| b["position"].as_i64());
    let laid_out: Vec<(&str, &str)> = blocks
        .iter()
        .map(|b| (b["kind"].as_str().unwrap(), b["title"].as_str().unwrap()))
        .collect();
    assert_eq!(
        laid_out,
        [
            ("collection", "To do"),
            ("collection", "Pflanzen"),
            ("you", "You")
        ]
    );
    assert_eq!(
        plants["sticks_to"],
        "Die Zimmerpflanzen zu Hause: gießen, düngen, umtopfen."
    );
    assert_eq!(plants["starters"].as_array().map(Vec::len), Some(2));

    let asked = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/ask"),
            Some(&ada),
            json!({ "text": "Wie oft gieße ich die Monstera? Sie steht am Nordfenster." }),
        )
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text);
    assert!(asked.text.contains("event: talk"), "{}", asked.text);
    assert!(asked.text.contains("Gieß die Monstera "), "{}", asked.text);
    assert!(asked.text.contains("event: done"), "{}", asked.text);

    assert!(
        eventually(&pool, "SELECT EXISTS (SELECT 1 FROM reminders)").await,
        "kept after the answer"
    );
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points.len(), 2);
    assert!(points.iter().all(|p| p["seen"] == false));
    let row = points.iter().find(|p| p["text"] == "Monstera").unwrap();
    assert_eq!(row["block_id"], blocks[1]["id"]);
    let questions = app.synced(&ada, &home, "Question").await;
    assert_eq!(questions[0]["point"], "Topf über 20 cm.");
    let reminders = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(reminders[0]["status"], "offered");
    assert_eq!(reminders[0]["every_days"], 7);
    let messages = app.synced(&ada, &home, "Message").await;
    let answer = messages.iter().find(|m| m["role"] == "almanac").unwrap();
    assert_eq!(answer["text"], "Gieß die Monstera alle 7 Tage.");
    assert_eq!(
        answer["changes"][0]["said"],
        "Kept 2 points under Pflege and Pflanzen."
    );
    assert_eq!(reminders[0]["message_id"], answer["id"]);
    let talks = app.synced(&ada, &home, "Talk").await;
    assert_eq!(talks[0]["title"], "Monstera gießen");

    // Claude reads what the topic sticks to and what the page kept, and
    // only this talk.
    let sent = fake.sent.lock().unwrap().clone();
    let streamed: Vec<&Value> = sent.iter().filter(|b| b["stream"] == true).collect();
    assert_eq!(streamed.len(), 1);
    let system = streamed[0]["system"].as_str().unwrap();
    assert!(system.contains("Die Zimmerpflanzen zu Hause"), "{system}");
    assert_eq!(streamed[0]["messages"].as_array().map(Vec::len), Some(1));
    assert!(streamed[0].get("tools").is_none(), "no web search");
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_key_the_talk_says_where_to_add_one(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "CTO pitch", "sticks_to": "Research for my pitch." }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap();
    let asked = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/ask"),
            Some(&ada),
            json!({ "text": "Who should I talk to first?" }),
        )
        .await;
    assert!(asked.text.contains("event: problem"), "{}", asked.text);
    assert!(asked.text.contains("Settings → Claude"), "{}", asked.text);
    let messages = app.synced(&ada, &home, "Message").await;
    assert_eq!(messages.len(), 2, "the question and why there's no answer");

    let key = app.get(&format!("/api/w/{home}/claude"), &ada).await;
    assert_eq!(
        key.body,
        json!({ "via": "key", "web": false, "set": false, "from_settings": false })
    );
    let set = app
        .call(
            "PUT",
            &format!("/api/w/{home}/claude"),
            Some(&ada),
            Some(json!({ "api_key": "sk-ant-test-1234" })),
        )
        .await;
    assert!(set.status.is_success(), "{}", set.body);
    let key = app.get(&format!("/api/w/{home}/claude"), &ada).await;
    assert_eq!(key.body["ends_with"], "1234");
    assert_eq!(key.body["set"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sub_topics_take_what_their_parent_kept_about_them(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Backend", "template": "routine" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    let parent = started.body["id"].as_str().unwrap().to_string();
    let [reading, lesson, page, plan] = [(); 4].map(|_| uuid::Uuid::now_v7());
    let point = |block: uuid::Uuid, text: &str| {
        json!({ "action": "create", "model": "Point", "id": uuid::Uuid::now_v7(),
            "data": { "topic_id": parent, "block_id": block, "text": text } })
    };
    let part = json!([{ "name": "Part", "kind": "choice", "unit": "", "goal": null,
        "options": ["Basics", "Notifications"] }]);
    let wrote = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "create", "model": "Block", "id": reading,
                  "data": { "topic_id": parent, "kind": "collection", "title": "Reading", "fields": part } },
                { "action": "create", "model": "Point", "id": lesson,
                  "data": { "topic_id": parent, "block_id": reading, "text": "Lesson 1",
                            "cells": { "Part": "Notifications" } } },
                { "action": "create", "model": "Point", "id": uuid::Uuid::now_v7(),
                  "data": { "topic_id": parent, "block_id": reading, "text": "Lesson 0",
                            "cells": { "Part": "Basics" } } },
                { "action": "create", "model": "Block", "id": page,
                  "data": { "topic_id": parent, "kind": "paragraph", "title": "The sketch", "row_id": lesson } },
                point(page, "**Caller**: who starts it."),
                point(page, "**Failures**: what if the store is down."),
                { "action": "create", "model": "Block", "id": plan,
                  "data": { "topic_id": parent, "kind": "collection", "title": "Plan" } },
                point(plan, "Week 1"),
            ]),
        )
        .await;
    assert_eq!(wrote.status, StatusCode::OK, "{}", wrote.text);

    // A talk of the parent proposed two in one answer: one taking the
    // lesson's page, one the plan.
    let (talk, message) = (uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let workspace: uuid::Uuid = home.parse().unwrap();
    let parent_id: uuid::Uuid = parent.parse().unwrap();
    sqlx::query(
        "INSERT INTO talks (id, workspace_id, topic_id, title) VALUES ($1, $2, $3, 'Split')",
    )
    .bind(talk)
    .bind(workspace)
    .bind(parent_id)
    .execute(&pool)
    .await
    .unwrap();
    let proposal = |name: &str, takes: uuid::Uuid| {
        json!({
            "name": name, "sticks_to": "Its part.", "icon": "book", "group_name": "",
            "template": "study", "fields": [], "starters": [], "points": [], "reminders": [],
            "topic_id": null, "parent_id": parent, "takes": [takes],
        })
    };
    sqlx::query(
        "INSERT INTO messages (id, workspace_id, talk_id, topic_id, role, text, sub_topics) \
         VALUES ($1, $2, $3, $4, 'almanac', 'Here they are.', $5)",
    )
    .bind(message)
    .bind(workspace)
    .bind(talk)
    .bind(parent_id)
    .bind(json!([
        proposal("Notifications", lesson),
        proposal("Planning", plan)
    ]))
    .execute(&pool)
    .await
    .unwrap();
    let start = |name: &'static str, n: u32| {
        let app = &app;
        let (ada, home) = (&ada, &home);
        async move {
            let started = app
                .post(
                    &format!("/api/w/{home}/topics"),
                    Some(ada),
                    json!({ "name": name, "from_message": message, "sub_topic": n }),
                )
                .await;
            assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
            started.body["id"].as_str().unwrap().to_string()
        }
    };
    let child = start("Notifications", 0).await;
    let planning = start("Planning", 1).await;
    // Each once.
    let again = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Planning", "from_message": message, "sub_topic": 1 }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);
    let messages = app.synced(&ada, &home, "Message").await;
    let said = messages.iter().find(|m| m["id"] == json!(message)).unwrap();
    assert_eq!(said["sub_topics"][0]["topic_id"], json!(child));
    assert_eq!(said["sub_topics"][1]["topic_id"], json!(planning));

    let blocks = app.synced(&ada, &home, "Block").await;
    let of = |topic: &str| -> Vec<String> {
        blocks
            .iter()
            .filter(|b| b["topic_id"] == topic)
            .map(|b| {
                format!(
                    "{}:{}",
                    b["kind"].as_str().unwrap(),
                    b["title"].as_str().unwrap()
                )
            })
            .collect()
    };
    // The lesson's page is the sub-topic's own, beside You; the template
    // lays out nothing else.
    assert_eq!(of(&child), ["paragraph:The sketch", "you:You"]);
    let sketch = blocks
        .iter()
        .find(|b| b["topic_id"] == child.as_str() && b["kind"] == "paragraph")
        .unwrap();
    assert!(sketch["row_id"].is_null());
    assert_eq!(of(&planning), ["collection:Plan", "you:You"]);
    // The parent keeps the rest: Reading without the lesson, its page or
    // the lane only it was in.
    let left = of(&parent);
    assert!(left.contains(&"collection:Reading".to_string()), "{left:?}");
    assert!(!left.contains(&"collection:Plan".to_string()), "{left:?}");
    assert!(
        !left.contains(&"paragraph:The sketch".to_string()),
        "{left:?}"
    );
    let reading = blocks.iter().find(|b| b["id"] == json!(reading)).unwrap();
    assert_eq!(reading["fields"][0]["options"], json!(["Basics"]));
    let points = app.synced(&ada, &home, "Point").await;
    let texts = |topic: &str| -> Vec<&str> {
        points
            .iter()
            .filter(|p| p["topic_id"] == topic)
            .map(|p| p["text"].as_str().unwrap())
            .collect()
    };
    assert_eq!(
        texts(&child),
        [
            "**Caller**: who starts it.",
            "**Failures**: what if the store is down."
        ]
    );
    assert_eq!(texts(&planning), ["Week 1"]);
    assert_eq!(texts(&parent), ["Lesson 0"]);
}
