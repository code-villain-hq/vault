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
            "points": [
                { "theme": "Monstera", "text": "Alle 7 Tage gießen.", "source": "From the talk" },
                { "theme": "Monstera", "text": "Steht am Nordfenster.", "source": "You said" },
            ],
            "questions": [
                { "theme": "Monstera", "text": "Ist der Topf größer als 20 cm?", "point": "Topf über 20 cm." },
            ],
            "reminders": [
                { "theme": "Monstera", "title": "Monstera gießen", "due_on": "2026-11-01", "at": "", "every_days": 7 },
            ],
            "elsewhere": "",
            "talk_title": "Monstera gießen",
        })
    } else {
        json!({
            "title": "Zimmerpflanzen und wie man sie pflegt",
            "sticks_to": "Die Zimmerpflanzen zu Hause: gießen, düngen, umtopfen.",
            "icon": "sprout",
            "group": "Home",
            "shape": "cards",
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
    assert_eq!(topics[0]["name"], "Pflanzen");
    assert_eq!(topics[0]["group_name"], "Home");
    assert_eq!(topics[0]["shape"], "cards");
    assert_eq!(
        topics[0]["sticks_to"],
        "Die Zimmerpflanzen zu Hause: gießen, düngen, umtopfen."
    );
    assert_eq!(topics[0]["starters"].as_array().map(Vec::len), Some(2));

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
    assert!(
        points
            .iter()
            .all(|p| p["theme"] == "Monstera" && p["seen"] == false)
    );
    let questions = app.synced(&ada, &home, "Question").await;
    assert_eq!(questions[0]["point"], "Topf über 20 cm.");
    let reminders = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(reminders[0]["status"], "offered");
    assert_eq!(reminders[0]["every_days"], 7);
    let messages = app.synced(&ada, &home, "Message").await;
    let answer = messages.iter().find(|m| m["role"] == "almanac").unwrap();
    assert_eq!(answer["text"], "Gieß die Monstera alle 7 Tage.");
    assert_eq!(answer["kept"], "Kept 2 points under Monstera.");
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
    assert_eq!(key.body, json!({ "set": false, "from_settings": false }));
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
