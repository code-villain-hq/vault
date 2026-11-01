//! Claude Code on this Mac, with a stand-in for its ACP adapter
//! (`claude-code.ts`, run by bun) that calls Almanac's tools over MCP the
//! way Claude Code does: the workspace's own topic proposes a topic, which
//! starts with a click, and changes one with undo; a topic's talk keeps a
//! point with its source; a new topic sets itself up; and the web can be
//! turned off. The workspace's own topic stays, a start that hangs is
//! tried again, and photos go with the questions of their talk. A moved
//! wash day replaces its point and reminder instead of adding to them. A
//! talk lays out its page anew, and undo puts it back; a question from a
//! section is about it.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use almanac_server::acp::{Adapter, Kind};
use almanac_server::router;
use axum::http::StatusCode;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;

/// An app whose Claude is the stand-in, with Almanac's MCP endpoint on a
/// real port for it to call. Returns the file its sessions are logged to,
/// and the server, which stops when the test lets go of it.
async fn with_claude_code(pool: PgPool, dir: &Path) -> (App, PathBuf, Serving) {
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
    // The stand-in starts in well under a second; one that hangs is tried
    // again soon.
    let claude_code = Adapter::new(
        Kind::ClaudeCode,
        dir.join("claude"),
        format!("http://{address}/api/mcp"),
    )
    .program(&adapter)
    .start_timeout(Duration::from_secs(5));
    let chatgpt = Adapter::new(
        Kind::Chatgpt,
        dir.join("chatgpt"),
        format!("http://{address}/api/mcp"),
    )
    .program(&adapter)
    .start_timeout(Duration::from_secs(5));
    let state = common::state(pool)
        .on_this_mac()
        .with_claude_code(claude_code)
        .with_chatgpt(chatgpt);
    let service = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, service).await });
    (App::with_state(state), log, Serving(server.abort_handle()))
}

/// The test's server, stopped when the test ends. Its state holds the
/// test's pool, whose connections sqlx lends from one pool every test
/// shares and takes back only once nothing holds it: a server left running
/// keeps them past the test, and the tests still running wait.
struct Serving(tokio::task::AbortHandle);

impl Drop for Serving {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn sessions(log: &Path) -> Vec<Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The blocks of each prompt the stand-in was sent.
fn prompts(log: &Path) -> Vec<Value> {
    sessions(&log.with_extension("jsonl.prompts"))
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
    let (app, log, _serving) = with_claude_code(pool, dir.path()).await;
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

    // After the checks Settings made, Claude Code's and ChatGPT's.
    let session = &sessions(&log)[2];
    let system = session["system"].as_str().unwrap();
    assert!(system.contains("Home"), "knows the workspace: {system}");
    assert_eq!(session["tools"], json!(["WebSearch", "WebFetch"]));
    let allowed = session["allowed"].as_array().unwrap();
    assert!(allowed.contains(&json!("mcp__almanac__propose_topic")));
    assert!(!allowed.contains(&json!("mcp__almanac__add_items")));

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
    assert_eq!(points[0]["text"], "Monstera");
    assert_eq!(points[0]["cells"], json!({ "Light": "North window" }));
    assert_eq!(points[0]["source"], "You said");
    let blocks = app.synced(&ada, &home, "Block").await;
    let rows = blocks.iter().find(|b| b["title"] == "Plants").unwrap();
    assert_eq!(rows["kind"], "collection");
    assert_eq!(rows["fields"][0]["name"], "Light");
    assert_eq!(points[0]["block_id"], rows["id"]);
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
    let (app, log, _serving) = with_claude_code(pool.clone(), dir.path()).await;
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
    assert_eq!(answer["changes"][0]["said"], "Kept a point under Care.");
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
async fn a_talk_changes_the_page_and_undo_puts_it_back(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, log, _serving) = with_claude_code(pool.clone(), dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    // The set-up picks a collection, whose rows are Plants.
    for _ in 0..100 {
        let laid_out: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks WHERE title = 'Plants')")
                .fetch_one(&pool)
                .await
                .unwrap();
        if laid_out {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let titles = |blocks: &[Value]| -> Vec<String> {
        let mut blocks = blocks.to_vec();
        blocks.sort_by_key(|b| b["position"].as_i64());
        blocks
            .iter()
            .map(|b| b["title"].as_str().unwrap().to_string())
            .collect()
    };
    let before = app.synced(&ada, &home, "Block").await;
    assert_eq!(titles(&before), ["To do", "Plants", "You"]);
    let plants = before.iter().find(|b| b["title"] == "Plants").unwrap()["id"].clone();

    // Asked from the Monstera's section, the question is about it, and
    // what it keeps goes in a block it starts.
    let asked = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/ask"),
            Some(&ada),
            json!({ "text": "How often do I water the Monstera?", "about": "Monstera" }),
        )
        .await;
    assert!(asked.text.contains("event: done"), "{}", asked.text);
    let system = sessions(&log).last().unwrap()["system"].clone();
    assert!(
        system
            .as_str()
            .unwrap()
            .contains("They ask from the section Monstera of the page"),
        "{system}"
    );
    let blocks = app.synced(&ada, &home, "Block").await;
    assert_eq!(titles(&blocks), ["To do", "Plants", "You", "Care"]);
    let care = blocks.iter().find(|b| b["title"] == "Care").unwrap()["id"].clone();

    let tidied = ask(&app, &ada, &home, &topic, "Tidy the page.").await;
    let change = &tidied["changes"][0];
    // To do is a checklist now: the sentence becomes one of its rows.
    assert_eq!(
        change["said"],
        "Made a sentence a row in To do. Removed Plants."
    );
    let after = app.synced(&ada, &home, "Block").await;
    assert_eq!(titles(&after), ["To do", "You", "Care"]);
    let to_do = after.iter().find(|b| b["title"] == "To do").unwrap()["id"].clone();
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points.len(), 1, "nothing else goes");
    assert_eq!(points[0]["block_id"], to_do);
    assert_eq!(
        points[0]["text"],
        "Water every 7 days, once the top is dry."
    );
    assert_eq!(points[0]["source"], "Royal Horticultural Society");

    let undone = app
        .post(
            &format!(
                "/api/w/{home}/messages/{}/undo",
                tidied["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(undone.status.is_success(), "{}", undone.body);
    let back = app.synced(&ada, &home, "Block").await;
    assert_eq!(titles(&back), ["To do", "Plants", "You", "Care"]);
    assert!(back.iter().any(|b| b["id"] == plants), "the same block");
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points.len(), 1);
    assert_eq!(points[0]["block_id"], care);
    assert_eq!(points[0]["text"], "Water every 7 days.");
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

#[sqlx::test(migrations = "../../migrations")]
async fn a_topic_sits_under_another_one_level_deep(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let own = topic_named(&app, &ada, &home, "Almanac").await["id"].clone();
    let topics = format!("/api/w/{home}/topics");
    let start = |name: &str, parent: Value| {
        app.post(
            &topics,
            Some(&ada),
            json!({ "name": name, "parent_id": parent }),
        )
    };
    let prices = start("Prices", Value::Null).await.body["id"].clone();
    let shoes = start("Shoes", prices.clone()).await;
    assert_eq!(shoes.status, StatusCode::CREATED, "{}", shoes.body);
    let shoes = shoes.body["id"].clone();

    // Not two levels deep, and not under Almanac's own.
    for parent in [shoes.clone(), own.clone()] {
        let refused = start("Preppy", parent.clone()).await;
        assert!(!refused.status.is_success(), "{parent}: {}", refused.body);
    }
    // Not under itself, and a topic with sub-topics or Almanac's own goes
    // under none.
    for (id, parent) in [(&shoes, &shoes), (&prices, &shoes), (&own, &prices)] {
        let refused = app
            .sync(
                &ada,
                &home,
                json!([{ "action": "update", "model": "Topic", "id": id,
                         "data": { "parent_id": parent } }]),
            )
            .await;
        assert!(!refused.status.is_success(), "{}", refused.body);
    }

    // Deleting the parent leaves its sub-topic, at the top.
    let deleted = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "delete", "model": "Topic", "id": prices }]),
        )
        .await;
    assert!(deleted.status.is_success(), "{}", deleted.body);
    let topics = app.synced(&ada, &home, "Topic").await;
    let shoes = topics.iter().find(|t| t["name"] == "Shoes").unwrap();
    assert!(shoes["parent_id"].is_null(), "{shoes}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_topic_keeps_what_you_wrote_and_learns_how_to_answer(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _, _serving) = with_claude_code(pool.clone(), dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;

    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen", "sticks_to": "Zimmerpflanzen, ich schicke Fotos" }),
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
    // Claude's brief left theirs out, so theirs comes first.
    let brief =
        "Zimmerpflanzen, ich schicke Fotos\nThe houseplants at home: watering, feeding, repotting.";
    let plants = topic_named(&app, &ada, &home, "Pflanzen").await;
    assert_eq!(plants["sticks_to"], brief);

    let answer = ask(&app, &ada, &home, &topic, "From now on, answer in English.").await;
    assert_eq!(answer["changes"][0]["name"], "Pflanzen");
    assert_eq!(answer["changes"][0]["said"], "Answers in English.");
    assert_eq!(answer["changes"][0]["before"]["sticks_to"], brief);
    let plants = topic_named(&app, &ada, &home, "Pflanzen").await;
    assert_eq!(plants["sticks_to"], format!("{brief}\nAnswers in English."));
    let undone = app
        .post(
            &format!(
                "/api/w/{home}/messages/{}/undo",
                answer["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(undone.status.is_success(), "{}", undone.body);
    let plants = topic_named(&app, &ada, &home, "Pflanzen").await;
    assert_eq!(plants["sticks_to"], brief);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_start_that_hangs_is_tried_again(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, log, _serving) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let claude = app.get(&format!("/api/w/{home}/claude"), &ada).await;
    assert_eq!(claude.body["claude_code"]["state"], "ready");
    let opened = sessions(&log).len();

    // The next session the stand-in is asked for never opens.
    let hang = dir.path().join("sessions.jsonl.hang");
    std::fs::write(&hang, "").unwrap();
    let own = topic_named(&app, &ada, &home, "Almanac").await;
    let answer = ask(
        &app,
        &ada,
        &home,
        own["id"].as_str().unwrap(),
        "I want to keep my houseplants alive.",
    )
    .await;
    assert_eq!(answer["text"], "Here's a topic for your plants.");
    assert!(!hang.exists(), "the first start hung");
    assert_eq!(sessions(&log).len(), opened + 1, "the second one opened");
}

#[sqlx::test(migrations = "../../migrations")]
async fn photos_go_with_the_questions_of_their_talk(pool: PgPool) {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    let (app, log, _serving) = with_claude_code(pool.clone(), dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Haare" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    // The set-up has its own prompt, and lays out the page: let it be first.
    for _ in 0..100 {
        let laid_out: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks)")
            .fetch_one(&pool)
            .await
            .unwrap();
        if prompts(&log).len() == 1 && laid_out {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let photo = base64::engine::general_purpose::STANDARD.encode(b"the shampoo");
    let asked = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/ask"),
            Some(&ada),
            json!({
                "text": "Which shampoo is this?",
                "photos": [{ "media_type": "image/jpeg", "data": photo, "width": 300, "height": 400 }],
            }),
        )
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text);
    assert!(asked.text.contains("event: done"), "{}", asked.text);
    let messages = app.synced(&ada, &home, "Message").await;
    let question = messages.iter().find(|m| m["role"] == "you").unwrap();
    let sent = &question["photos"][0];
    assert_eq!(sent["width"], 300);
    let shown = app
        .get(
            &format!("/api/w/{home}/photos/{}", sent["id"].as_str().unwrap()),
            &ada,
        )
        .await;
    assert_eq!(shown.status, StatusCode::OK);
    assert_eq!(shown.text, "the shampoo");

    // Claude reads the photo after the question, numbered, and again with
    // the next, which says it came with none.
    let image = json!({ "type": "image", "mimeType": "image/jpeg", "data": photo });
    let first = prompts(&log).last().cloned().unwrap();
    assert_eq!(first[0]["type"], "text");
    assert!(
        first[0]["text"]
            .as_str()
            .unwrap()
            .contains("(with photo 1)")
    );
    assert_eq!(first[1]["text"], "Photo 1:");
    assert_eq!(first[2], image);
    let talk = question["talk_id"].as_str().unwrap();
    let again = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/ask"),
            Some(&ada),
            json!({ "talk_id": talk, "text": "And how often?" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text);
    let next = prompts(&log).last().cloned().unwrap();
    assert!(
        next[0]["text"]
            .as_str()
            .unwrap()
            .contains("(with no new photo)")
    );
    assert_eq!(next[2], image, "{next}");

    // The photo goes with its talk.
    let deleted = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "delete", "model": "Talk", "id": talk }]),
        )
        .await;
    assert!(deleted.status.is_success(), "{}", deleted.body);
    let gone = app
        .get(
            &format!("/api/w/{home}/photos/{}", sent["id"].as_str().unwrap()),
            &ada,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_item_rings_and_a_tick_ticks_its_reminder(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _, _serving) = with_claude_code(pool.clone(), dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Haare" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    // The set-up lays out the page the points go in.
    for _ in 0..100 {
        let laid_out: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocks)")
            .fetch_one(&pool)
            .await
            .unwrap();
        if laid_out {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ask(&app, &ada, &home, &topic, "I wash on Thursday").await;
    let points = app.synced(&ada, &home, "Point").await;
    let item = points[0]["id"].clone();
    let first = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["title"], "Wash on Thursday", "the item's words");
    assert_eq!(first[0]["point_id"], item);
    let thursday = first[0]["id"].clone();

    let answer = ask(&app, &ada, &home, &topic, "My wash day moved to Friday").await;
    assert_eq!(answer["changes"][0]["said"], "Changed a point.");
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points.len(), 1, "{points:?}");
    assert_eq!(points[0]["text"], "Wash on Friday");
    assert_eq!(points[0]["seen"], false);
    // The Thursday offer wasn't taken yet, so it goes at once.
    let reminders = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(reminders.len(), 1, "{reminders:?}");
    assert_ne!(reminders[0]["id"], thursday);
    assert_eq!(reminders[0]["due_on"], "2026-10-30");
    assert_eq!(reminders[0]["point_id"], item);

    // Taken, the reminder is what a tick of its item ticks.
    let friday = reminders[0]["id"].clone();
    let taken = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Reminder", "id": friday, "data": { "status": "on" } }]),
        )
        .await;
    assert!(taken.status.is_success(), "{}", taken.body);
    ask(&app, &ada, &home, &topic, "I washed today").await;
    let reminders = app.synced(&ada, &home, "Reminder").await;
    assert_eq!(reminders[0]["status"], "on", "it comes round again");
    assert_eq!(reminders[0]["last_done_on"], "2026-10-30");
    assert!(reminders[0]["due_on"].as_str().unwrap() > "2026-10-30");
    let points = app.synced(&ada, &home, "Point").await;
    assert_eq!(points[0]["done"], false, "the reminder keeps the tick");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_talk_moves_a_row_and_its_page_to_a_sub_topic_with_undo(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _, _serving) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topics = format!("/api/w/{home}/topics");
    let start = |name: &str, parent: Value| {
        app.post(
            &topics,
            Some(&ada),
            json!({ "name": name, "template": "study", "parent_id": parent }),
        )
    };
    let backend = start("Backend", Value::Null).await.body["id"].clone();
    let notifications = start("Notifications", backend.clone()).await.body["id"].clone();
    let blocks = app.synced(&ada, &home, "Block").await;
    let reading = blocks
        .iter()
        .find(|b| b["topic_id"] == backend && b["title"] == "Reading")
        .unwrap()["id"]
        .clone();
    let (lesson, sketch) = (uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let wrote = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "create", "model": "Point", "id": lesson,
                  "data": { "topic_id": backend, "block_id": reading, "text": "Lesson 1" } },
                { "action": "create", "model": "Block", "id": sketch,
                  "data": { "topic_id": backend, "kind": "paragraph", "title": "The sketch", "row_id": lesson } },
                { "action": "create", "model": "Point", "id": uuid::Uuid::now_v7(),
                  "data": { "topic_id": backend, "block_id": sketch, "text": "**Caller**: who starts it." } },
            ]),
        )
        .await;
    assert!(wrote.status.is_success(), "{}", wrote.text);

    let backend_id = backend.as_str().unwrap();
    let moved = ask(&app, &ada, &home, backend_id, "move p1 to Notifications").await;
    let changes = moved["changes"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|c| c["topic_id"] == notifications && c["said"] == "Moved Lesson 1 here."),
        "{changes:?}"
    );
    let on = |blocks: &[Value], topic: &Value| -> Vec<String> {
        blocks
            .iter()
            .filter(|b| b["topic_id"] == *topic)
            .map(|b| b["title"].as_str().unwrap().to_string())
            .collect()
    };
    // What moved says so there; here only Reading, which it left empty,
    // is removed in words.
    let here: Vec<&str> = changes
        .iter()
        .filter(|c| c["topic_id"] == backend)
        .filter_map(|c| c["said"].as_str())
        .chain(moved["kept"].as_str())
        .collect();
    assert!(here.iter().any(|s| s.contains("Reading")), "{moved}");
    assert!(
        !here
            .iter()
            .any(|s| s.contains("sketch") || s.contains("paragraph")),
        "{moved}"
    );
    let blocks = app.synced(&ada, &home, "Block").await;
    // The lesson's page is on the sub-topic's own page now, and off the
    // parent's with its row, and Reading, with no row left, with them.
    assert!(on(&blocks, &notifications).contains(&"The sketch".to_string()));
    assert!(!on(&blocks, &backend).contains(&"The sketch".to_string()));
    assert!(!on(&blocks, &backend).contains(&"Reading".to_string()));
    let points = app.synced(&ada, &home, "Point").await;
    let caller = points
        .iter()
        .find(|p| p["text"] == "**Caller**: who starts it.")
        .unwrap();
    assert_eq!(caller["topic_id"], notifications);
    assert!(!points.iter().any(|p| p["text"] == "Lesson 1"));

    let undone = app
        .post(
            &format!(
                "/api/w/{home}/messages/{}/undo",
                moved["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(undone.status.is_success(), "{}", undone.body);
    let blocks = app.synced(&ada, &home, "Block").await;
    assert!(!on(&blocks, &notifications).contains(&"The sketch".to_string()));
    assert!(on(&blocks, &backend).contains(&"Reading".to_string()));
    let back = blocks
        .iter()
        .find(|b| b["id"] == sketch.to_string())
        .unwrap();
    assert_eq!(
        back["row_id"],
        lesson.to_string(),
        "on the lesson's page again"
    );
    let points = app.synced(&ada, &home, "Point").await;
    assert!(points.iter().any(|p| p["id"] == lesson.to_string()));
    let callers: Vec<_> = points
        .iter()
        .filter(|p| p["text"] == "**Caller**: who starts it.")
        .collect();
    assert_eq!(callers.len(), 1, "kept once");
    assert_eq!(callers[0]["topic_id"], backend);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_talk_renames_its_sub_topic_but_no_other_topic(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _, _serving) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topics = format!("/api/w/{home}/topics");
    let start = |name: &str, parent: Value| {
        app.post(
            &topics,
            Some(&ada),
            json!({ "name": name, "template": "study", "parent_id": parent }),
        )
    };
    let backend = start("Backend", Value::Null).await.body["id"].clone();
    let notifications = start("Notifications", backend.clone()).await.body["id"].clone();
    start("Cooking", Value::Null).await;
    let backend_id = backend.as_str().unwrap();
    let named = |topics: &[Value], id: &Value| {
        topics.iter().find(|t| t["id"] == *id).unwrap()["name"].clone()
    };

    let renamed = ask(&app, &ada, &home, backend_id, "call Notifications Alerts").await;
    let topics = app.synced(&ada, &home, "Topic").await;
    assert_eq!(named(&topics, &notifications), "Alerts");
    // Another topic it only reads.
    ask(&app, &ada, &home, backend_id, "call Cooking Baking").await;
    let topics = app.synced(&ada, &home, "Topic").await;
    assert!(topics.iter().any(|t| t["name"] == "Cooking"));

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
    let topics = app.synced(&ada, &home, "Topic").await;
    assert_eq!(named(&topics, &notifications), "Notifications");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_talk_changes_its_sub_topics_pages_but_no_other(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _, _serving) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;
    let topics = format!("/api/w/{home}/topics");
    let start = |name: &str, parent: Value| {
        app.post(
            &topics,
            Some(&ada),
            json!({ "name": name, "template": "study", "parent_id": parent }),
        )
    };
    let backend = start("Backend", Value::Null).await.body["id"].clone();
    let notifications = start("Notifications", backend.clone()).await.body["id"].clone();
    let cooking = start("Cooking", Value::Null).await.body["id"].clone();
    let backend_id = backend.as_str().unwrap();
    let notes_on = |blocks: &[Value], topic: &Value| {
        blocks
            .iter()
            .any(|b| b["topic_id"] == *topic && b["title"] == "Notes")
    };

    let noted = ask(
        &app,
        &ada,
        &home,
        backend_id,
        "in Notifications note Use web push.",
    )
    .await;
    let blocks = app.synced(&ada, &home, "Block").await;
    assert!(notes_on(&blocks, &notifications));
    assert!(!notes_on(&blocks, &backend));
    let points = app.synced(&ada, &home, "Point").await;
    assert!(
        points
            .iter()
            .any(|p| p["topic_id"] == notifications && p["text"] == "Use web push.")
    );
    let changes = noted["changes"].as_array().unwrap();
    assert!(
        changes.iter().any(|c| c["topic_id"] == notifications),
        "{changes:?}"
    );
    // Another topic it only reads.
    ask(
        &app,
        &ada,
        &home,
        backend_id,
        "in Cooking note Salt the water.",
    )
    .await;
    let blocks = app.synced(&ada, &home, "Block").await;
    assert!(!notes_on(&blocks, &cooking));

    let undone = app
        .post(
            &format!(
                "/api/w/{home}/messages/{}/undo",
                noted["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(undone.status.is_success(), "{}", undone.body);
    let blocks = app.synced(&ada, &home, "Block").await;
    assert!(!notes_on(&blocks, &notifications));
    let points = app.synced(&ada, &home, "Point").await;
    assert!(!points.iter().any(|p| p["text"] == "Use web push."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn chatgpt_answers_once_it_is_chosen(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, log, _serving) = with_claude_code(pool, dir.path()).await;
    let (ada, home) = app.signup("Ada", "Home").await;

    let shown = app
        .call("GET", &format!("/api/w/{home}/claude"), Some(&ada), None)
        .await;
    assert_eq!(shown.body["answers"], "claude_code", "{}", shown.body);
    assert_eq!(shown.body["chatgpt"]["state"], "ready", "{}", shown.body);
    assert_eq!(
        shown.body["chatgpt"]["install"],
        "npm install -g @agentclientprotocol/codex-acp"
    );
    let wrong = app
        .call(
            "PUT",
            &format!("/api/w/{home}/claude/answers"),
            Some(&ada),
            Some(json!({ "answers": "gemini" })),
        )
        .await;
    assert!(wrong.status.is_client_error(), "{}", wrong.body);
    let chosen = app
        .call(
            "PUT",
            &format!("/api/w/{home}/claude/answers"),
            Some(&ada),
            Some(json!({ "answers": "chatgpt" })),
        )
        .await;
    assert!(chosen.status.is_success(), "{}", chosen.body);

    let topic = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen" }),
        )
        .await
        .body["id"]
        .as_str()
        .unwrap()
        .to_string();
    let answer = ask(
        &app,
        &ada,
        &home,
        &topic,
        "How often do I water the Monstera?",
    )
    .await;
    assert_eq!(answer["changes"][0]["said"], "Kept a point under Care.");

    // The answer's session: the topic sets itself up in the background, so
    // its session may start after the question's.
    let asked = sessions(&log);
    let last = asked
        .iter()
        .rfind(|s| {
            s["system"]
                .as_str()
                .is_some_and(|s| s.contains("You are Almanac"))
        })
        .expect("Almanac's prompt in place of Codex's");
    assert_eq!(last["config"]["features"]["shell_tool"], false, "{last}");
    assert_eq!(last["config"]["sandbox_mode"], "read-only");
    assert_eq!(last["config"]["web_search"], "live");
    assert!(
        last["home"].as_str().unwrap().ends_with("chatgpt/codex"),
        "a Codex home of Almanac's own: {last}"
    );
    // The set-up's session may still be answering; its file goes with it.
    let mut left = Vec::new();
    for _ in 0..200 {
        left = std::fs::read_dir(dir.path().join("chatgpt/codex"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        if left.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        left.is_empty(),
        "the prompt's file goes once it's answered: {left:?}"
    );
}
