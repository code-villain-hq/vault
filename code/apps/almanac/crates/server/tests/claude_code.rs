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
    // The stand-in starts in well under a second; one that hangs is tried
    // again soon.
    let claude_code = ClaudeCode::new(dir.join("claude"), format!("http://{address}/api/mcp"))
        .program(adapter)
        .start_timeout(Duration::from_secs(5));
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
    let rows = blocks.iter().find(|b| b["kind"] == "collection").unwrap();
    assert_eq!(rows["title"], "Plants");
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
    let (app, log) = with_claude_code(pool.clone(), dir.path()).await;
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
    assert_eq!(
        change["said"],
        "Changed a point, moved a point and removed Plants."
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
async fn a_topic_keeps_what_you_wrote_and_learns_how_to_answer(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = with_claude_code(pool.clone(), dir.path()).await;
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
    let (app, log) = with_claude_code(pool, dir.path()).await;
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
    let (app, log) = with_claude_code(pool.clone(), dir.path()).await;
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
    let (app, _) = with_claude_code(pool.clone(), dir.path()).await;
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
