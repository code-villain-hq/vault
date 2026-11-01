//! A server hands a question to the asker's Mac: the desktop app asks with
//! `here`, gets the question and its token, calls Almanac's tools at the
//! server's `/api/mcp`, and says what it made of it at `/api/answered`.
//! Signed out there, the server's key answers, or the talk says how to
//! sign in. A new topic sets itself up the same way. Claude Code on the
//! Mac, a stand-in for its ACP adapter (`claude-code.ts`), keeps points on
//! the server through its tools.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use almanac_server::acp::{Adapter, Kind};
use almanac_server::claude::Claude;
use almanac_server::{AppState, handed, router};
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The app on a real port, as the Mac reaches it, stopped when the test
/// lets go of it.
struct Served {
    app: App,
    url: String,
    server: tokio::task::AbortHandle,
}

impl Drop for Served {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn serve(state: AppState) -> Served {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, service).await });
    Served {
        app: App::with_state(state),
        url,
        server: server.abort_handle(),
    }
}

impl Served {
    /// Asks the topic from the desktop app, which answers on its Mac.
    async fn ask_here(&self, session: &str, workspace: &str, topic: &str, text: &str) -> Events {
        let response = reqwest::Client::new()
            .post(format!("{}/api/w/{workspace}/topics/{topic}/ask", self.url))
            .header("cookie", format!("almanac_session={session}"))
            .json(&json!({ "text": text, "here": true }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        Events {
            response,
            read: Vec::new(),
        }
    }

    /// What the Mac sends with the question's token.
    async fn mac(&self, path: &str, token: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}{path}", self.url))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
}

/// A question's events, as they come.
struct Events {
    response: reqwest::Response,
    read: Vec<u8>,
}

impl Events {
    /// The next event's name and data, or nothing once the answer is over.
    async fn next(&mut self) -> Option<(String, String)> {
        loop {
            if let Some(end) = self.read.windows(2).position(|w| w == b"\n\n") {
                let block: Vec<u8> = self.read.drain(..end + 2).collect();
                let block = String::from_utf8(block).unwrap();
                let (mut event, mut data) = (String::new(), String::new());
                for line in block.lines() {
                    if let Some(name) = line.strip_prefix("event: ") {
                        event = name.to_string();
                    } else if let Some(more) = line.strip_prefix("data: ") {
                        data.push_str(more);
                    }
                }
                return Some((event, data));
            }
            let chunk = tokio::time::timeout(Duration::from_secs(10), self.response.chunk())
                .await
                .expect("the next event in time")
                .unwrap()?;
            self.read.extend_from_slice(&chunk);
        }
    }

    /// The data of the next `event`, past any others.
    async fn until(&mut self, event: &str) -> Value {
        while let Some((name, data)) = self.next().await {
            if name == event {
                return serde_json::from_str(&data).unwrap();
            }
        }
        panic!("no {event} event");
    }

    /// The events left, as `name: data` lines.
    async fn rest(mut self) -> String {
        let mut rest = String::new();
        while let Some((name, data)) = self.next().await {
            rest.push_str(&format!("{name}: {data}\n"));
        }
        rest
    }
}

fn mcp(id: u32, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

async fn topic(app: &App, session: &str, workspace: &str) -> String {
    let started = app
        .post(
            &format!("/api/w/{workspace}/topics"),
            Some(session),
            json!({ "name": "Pflanzen" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    assert!(started.body.get("set_up").is_none(), "{}", started.body);
    started.body["id"].as_str().unwrap().to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_server_hands_the_question_to_the_asking_mac(pool: PgPool) {
    let served = serve(common::state(pool)).await;
    let app = &served.app;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topic = topic(app, &ada, &home).await;

    let mut events = served
        .ask_here(&ada, &home, &topic, "Wie oft gieße ich die Monstera?")
        .await;
    let question = events.until("question").await;
    let token = question["token"].as_str().unwrap();
    assert!(
        question["prompt"].as_str().unwrap().contains("Monstera"),
        "{question}"
    );
    assert!(
        question["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("name_talk"))
    );
    assert_eq!(question["web"], true);

    // Claude Code there works through the server's tools.
    let listed: Value = served
        .mac("/api/mcp", token, mcp(1, "tools/list", json!({})))
        .await
        .json()
        .await
        .unwrap();
    assert!(!listed["result"]["tools"].as_array().unwrap().is_empty());
    let named: Value = served
        .mac(
            "/api/mcp",
            token,
            mcp(
                2,
                "tools/call",
                json!({ "name": "name_talk", "arguments": { "title": "Monstera gießen" } }),
            ),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(named["result"]["isError"], false, "{named}");

    let answered = served
        .mac("/api/answered", token, json!({ "text": "Alle 7 Tage." }))
        .await;
    assert_eq!(answered.status(), 204);
    let rest = events.rest().await;
    assert!(rest.contains("done: "), "{rest}");
    assert!(!rest.contains("problem: "), "{rest}");

    let messages = app.synced(&ada, &home, "Message").await;
    let answer = messages.iter().find(|m| m["role"] == "almanac").unwrap();
    assert_eq!(answer["text"], "Alle 7 Tage.");
    let talks = app.synced(&ada, &home, "Talk").await;
    assert_eq!(talks[0]["title"], "Monstera gießen");

    // The question is over, and its token with it.
    let again = served
        .mac("/api/answered", token, json!({ "text": "Noch mal." }))
        .await;
    assert_eq!(again.status(), 401);
    let listed = served
        .mac("/api/mcp", token, mcp(3, "tools/list", json!({})))
        .await;
    assert_eq!(listed.status(), 401);
}

#[sqlx::test(migrations = "../../migrations")]
async fn signed_out_there_and_no_key_says_how_to_sign_in(pool: PgPool) {
    let served = serve(common::state(pool)).await;
    let app = &served.app;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topic = topic(app, &ada, &home).await;

    let mut events = served.ask_here(&ada, &home, &topic, "Hallo?").await;
    let question = events.until("question").await;
    let token = question["token"].as_str().unwrap();
    let answered = served
        .mac(
            "/api/answered",
            token,
            json!({ "not_answered": "signed_out" }),
        )
        .await;
    assert_eq!(answered.status(), 204);
    let rest = events.rest().await;
    assert!(
        rest.contains("problem: ") && rest.contains("Claude Code isn't signed in on this Mac."),
        "{rest}"
    );
}

/// The Messages API, answering every talk the same.
async fn messages(Json(body): Json<Value>) -> Response {
    if body["stream"] != true {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "type": "error", "error": { "type": "invalid_request_error", "message": "Only talks here." } })),
        )
            .into_response();
    }
    let events = [
        json!({ "type": "message_start", "message": {} }),
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Vom Server." } }),
        json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
        json!({ "type": "message_stop" }),
    ];
    let text: String = events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect();
    ([("content-type", "text/event-stream")], text).into_response()
}

#[sqlx::test(migrations = "../../migrations")]
async fn signed_out_there_the_servers_key_answers(pool: PgPool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}/v1/messages", listener.local_addr().unwrap());
    let stand_in = axum::Router::new().route("/v1/messages", post(messages));
    let stand_in = tokio::spawn(async move { axum::serve(listener, stand_in).await });
    let state = AppState {
        claude: Claude::new(Some("sk-test".into())).at(api),
        ..common::state(pool)
    };
    let served = serve(state).await;
    let app = &served.app;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topic = topic(app, &ada, &home).await;

    let mut events = served.ask_here(&ada, &home, &topic, "Hallo?").await;
    let question = events.until("question").await;
    let token = question["token"].as_str().unwrap();
    let answered = served
        .mac(
            "/api/answered",
            token,
            json!({ "not_answered": "not_installed" }),
        )
        .await;
    assert_eq!(answered.status(), 204);
    let rest = events.rest().await;
    assert!(rest.contains("Vom Server."), "{rest}");
    assert!(rest.contains("done: "), "{rest}");
    assert!(!rest.contains("problem: "), "{rest}");
    stand_in.abort();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_topic_sets_itself_up_on_the_asking_mac(pool: PgPool) {
    let served = serve(common::state(pool.clone())).await;
    let app = &served.app;
    let (ada, home) = app.signup("Ada", "Home").await;

    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen", "here": true }),
        )
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    let set_up = &started.body["set_up"];
    assert_eq!(set_up["tools"], json!(["set_up_topic"]));
    assert!(
        set_up["prompt"].as_str().unwrap().contains("Pflanzen"),
        "{set_up}"
    );
    let token = set_up["token"].as_str().unwrap();

    let called: Value = served
        .mac(
            "/api/mcp",
            token,
            mcp(
                1,
                "tools/call",
                json!({ "name": "set_up_topic", "arguments": {
                    "sticks_to": "Die Zimmerpflanzen zu Hause.",
                    "icon": "sprout",
                    "group": "Home",
                    "template": "collection",
                    "cards": "Pflanzen",
                    "starters": ["Wie oft gieße ich die Monstera?"],
                } }),
            ),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(called["result"]["isError"], false, "{called}");
    let answered = served
        .mac("/api/answered", token, json!({ "text": "Set up." }))
        .await;
    assert_eq!(answered.status(), 204);

    let mut set = false;
    for _ in 0..100 {
        let icon: Option<String> =
            sqlx::query_scalar("SELECT icon FROM topics WHERE name = 'Pflanzen'")
                .fetch_one(&pool)
                .await
                .unwrap();
        if icon.as_deref() == Some("sprout") {
            set = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(set, "set up once the Mac said so");
    let topics = app.synced(&ada, &home, "Topic").await;
    let plants = topics.iter().find(|t| t["name"] == "Pflanzen").unwrap();
    assert_eq!(plants["group_name"], "Home");
}

#[sqlx::test(migrations = "../../migrations")]
async fn claude_code_on_the_mac_keeps_points_on_the_server(pool: PgPool) {
    let served = serve(common::state(pool)).await;
    let app = &served.app;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topic = topic(app, &ada, &home).await;
    // The Mac's Claude Code, with the server's tools.
    let dir = tempfile::tempdir().unwrap();
    let (program, _) = common::claude_code_stand_in(dir.path());
    let mac = Adapter::new(
        Kind::ClaudeCode,
        dir.path().join("claude"),
        format!("{}/api/mcp", served.url),
    )
    .program(program);

    let mut events = served
        .ask_here(&ada, &home, &topic, "Wie oft gieße ich die Monstera?")
        .await;
    let question: almanac_domain::HandedQuestion =
        serde_json::from_value(events.until("question").await).unwrap();
    let token = question.token.clone();
    let mut pieces = String::new();
    let answered = handed::answer(&mac, question, |piece| pieces.push_str(piece)).await;
    assert!(answered.not_answered.is_none(), "{answered:?}");
    assert!(pieces.starts_with("Water it every 7 days."), "{pieces}");
    let posted = served
        .mac(
            "/api/answered",
            &token,
            serde_json::to_value(&answered).unwrap(),
        )
        .await;
    assert_eq!(posted.status(), 204);
    let rest = events.rest().await;
    assert!(rest.contains("done: "), "{rest}");

    let points = app.synced(&ada, &home, "Point").await;
    let kept = points
        .iter()
        .find(|p| p["text"] == "Water every 7 days.")
        .expect("kept on the server");
    assert_eq!(kept["source"], "Royal Horticultural Society");
    let messages = app.synced(&ada, &home, "Message").await;
    let answer = messages.iter().find(|m| m["role"] == "almanac").unwrap();
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .starts_with("Water it every 7 days."),
        "{answer}"
    );
}
