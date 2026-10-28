//! Claude Code on this Mac, with a stand-in for its ACP adapter
//! (`claude-code.ts`, run by bun) that calls Almanac's tools over MCP the
//! way Claude Code does: the workspace's own topic proposes a topic, which
//! starts with a click, and changes one with undo; a topic's talk keeps a
//! point with its source; a new topic sets itself up; and the web can be
//! turned off. The workspace's own topic stays.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use almanac_server::acp::ClaudeCode;
use almanac_server::router;
use axum::http::StatusCode;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;

/// An app whose Claude is the stand-in, with Almanac's MCP endpoint on a
/// real port for it to call. Returns the file its sessions are logged to.
async fn with_claude_code(pool: PgPool, dir: &Path) -> (App, PathBuf) {
    let bun = std::env::split_paths(&std::env::var("PATH").unwrap())
        .map(|dir| dir.join("bun"))
        .find(|bun| bun.is_file())
        .expect("bun on the PATH");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/claude-code.ts");
    let log = dir.join("sessions.jsonl");
    let adapter = dir.join("claude-agent-acp");
    std::fs::write(
        &adapter,
        format!(
            "#!/bin/sh\nexec '{}' '{}' '{}'\n",
            bun.display(),
            script.display(),
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        &adapter,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let claude_code =
        ClaudeCode::new(dir.join("claude"), format!("http://{address}/api/mcp")).program(adapter);
    let state = common::state(pool)
        .on_this_mac()
        .with_claude_code(claude_code);
    let service = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, service).await });
    (App::with_state(state), log)
}

fn sessions(log: &Path) -> Vec<Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

async fn topic_named(app: &App, session: &str, workspace: &str, name: &str) -> Value {
    app.synced(session, workspace, "Topic")
        .await
        .into_iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("no topic {name}"))
}

/// Asks a topic and returns the answer's message.
async fn ask(app: &App, session: &str, workspace: &str, topic: &str, text: &str) -> Value {
    let asked = app
        .post(
            &format!("/api/w/{workspace}/topics/{topic}/ask"),
            Some(session),
            json!({ "text": text }),
        )
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text);
    assert!(!asked.text.contains("event: problem"), "{}", asked.text);
    assert!(asked.text.contains("event: done"), "{}", asked.text);
    app.synced(session, workspace, "Message")
        .await
        .into_iter()
        .filter(|m| m["role"] == "almanac")
        .max_by_key(|m| m["created_at"].as_str().map(str::to_string))
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn almanac_proposes_a_topic_and_changes_one_with_undo(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, log) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;

    let claude = app.get(&format!("/api/w/{home}/claude"), &ada).await;
    assert_eq!(claude.body["via"], "claude_code");
    assert_eq!(claude.body["claude_code"]["state"], "ready");
    assert_eq!(claude.body["claude_code"]["version"], "0.0.0-test");
    assert_eq!(claude.body["web"], true);

    let own = topic_named(&app, &ada, &home, "Almanac").await;
    assert_eq!(own["kind"], "almanac");
    let own = own["id"].as_str().unwrap();
    let answer = ask(
        &app,
        &ada,
        &home,
        own,
        "I want to keep my houseplants alive.",
    )
    .await;
    assert_eq!(answer["text"], "Here's a topic for your plants.");
    let proposal = &answer["proposal"];
    assert_eq!(proposal["name"], "Plants");
    assert_eq!(proposal["icon"], "sprout");
    assert!(proposal["topic_id"].is_null(), "not started until a click");
    assert!(
        app.synced(&ada, &home, "Topic").await.len() == 1,
        "only Almanac so far"
    );

    let session = &sessions(&log)[1];
    let system = session["system"].as_str().unwrap();
    assert!(system.contains("Home"), "knows the workspace: {system}");
    assert_eq!(session["tools"], json!(["WebSearch", "WebFetch"]));
    let allowed = session["allowed"].as_array().unwrap();
    assert!(allowed.contains(&json!("mcp__almanac__propose_topic")));
    assert!(!allowed.contains(&json!("mcp__almanac__keep_point")));

    // Start: the proposal as it is, with its first points and reminders.
    let start = json!({ "name": "Plants", "from_message": answer["id"] });
    let started = app
        .post(&format!("/api/w/{home}/topics"), Some(&ada), start.clone())
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    let plants = topic_named(&app, &ada, &home, "Plants").await;
    assert_eq!(plants["kind"], "topic");
    assert_eq!(plants["group_name"], "Home");
    assert_eq!(plants["starters"].as_array().map(Vec::len), Some(2));
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points[0]["text"], "Stands by the north window.");
    assert_eq!(points[0]["source"], "You said");
    let reminders = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(reminders[0]["status"], "offered");
    assert_eq!(reminders[0]["topic_id"], plants["id"]);
    let messages = app.synced(&ada, &home, "Message").await;
    let answered = messages.iter().find(|m| m["id"] == answer["id"]).unwrap();
    assert_eq!(answered["proposal"]["topic_id"], plants["id"]);
    let again = app
        .post(&format!("/api/w/{home}/topics"), Some(&ada), start)
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);

    // A change says what it did, and undo puts it back.
    let renamed = ask(&app, &ada, &home, own, "Rename Plants to Garden").await;
    assert_eq!(renamed["changes"][0]["said"], "Renamed it Garden.");
    assert_eq!(renamed["changes"][0]["before"]["name"], "Plants");
    topic_named(&app, &ada, &home, "Garden").await;
    let undone = app
        .post(
            &format!(
                "/api/w/{home}/messages/{}/undo",
                renamed["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(undone.status.is_success(), "{}", undone.body);
    topic_named(&app, &ada, &home, "Plants").await;
    let messages = app.synced(&ada, &home, "Message").await;
    let renamed = messages.iter().find(|m| m["id"] == renamed["id"]).unwrap();
    assert_eq!(renamed["changes"][0]["undone"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_topic_sets_itself_up_and_keeps_a_point_without_the_web(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, log) = with_claude_code(pool.clone(), dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;

    let off = app
        .call(
            "PUT",
            &format!("/api/w/{home}/claude/web"),
            Some(&ada),
            Some(json!({ "web": false })),
        )
        .await;
    assert!(off.status.is_success(), "{}", off.body);

    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::CREATED, "{}", started.body);
    let topic = started.body["id"].as_str().unwrap().to_string();
    let mut set_up = false;
    for _ in 0..100 {
        set_up = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM topics WHERE icon = 'sprout')")
            .fetch_one(&pool)
            .await
            .unwrap();
        if set_up {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(set_up, "set up in the background");
    let plants = topic_named(&app, &ada, &home, "Pflanzen").await;
    assert_eq!(
        plants["sticks_to"],
        "The houseplants at home: watering, feeding, repotting."
    );

    let answer = ask(
        &app,
        &ada,
        &home,
        &topic,
        "How often do I water the Monstera?",
    )
    .await;
    assert_eq!(
        answer["text"],
        "Water it every 7 days.\n\nCheck that the top of the soil is dry first."
    );
    assert_eq!(answer["kept"], "Kept a point under Monstera.");
    assert_eq!(
        answer["sources"][0]["url"],
        "https://www.rhs.org.uk/plants/monstera"
    );
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points[0]["source"], "Royal Horticultural Society");
    assert_eq!(points[0]["talk_id"], answer["talk_id"]);

    let asked = sessions(&log);
    let last = asked.last().unwrap();
    assert_eq!(last["tools"], json!([]), "no web: {last}");
    assert!(
        last["system"].as_str().unwrap().contains("houseplants"),
        "reads the page"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_workspaces_own_topic_can_be_renamed_but_stays(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let own = topic_named(&app, &ada, &home, "Almanac").await;
    let id = own["id"].clone();

    let deleted = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "delete", "model": "Topic", "id": id }]),
        )
        .await;
    assert!(!deleted.status.is_success(), "{}", deleted.body);
    assert!(
        deleted.text.contains("You can rename it"),
        "{}",
        deleted.text
    );

    let renamed = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Topic", "id": id, "data": { "name": "Index" } }]),
        )
        .await;
    assert!(renamed.status.is_success(), "{}", renamed.body);
    let topics = app.synced(&ada, &home, "Topic").await;
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0]["name"], "Index");
}
