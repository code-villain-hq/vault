//! A topic the app makes itself, through sync, as it does offline: a
//! plain topic only, set up once the server has it.

mod common;

use axum::http::StatusCode;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;

#[sqlx::test(migrations = "../../migrations")]
async fn a_topic_made_in_the_app_is_set_up_once_the_server_has_it(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;

    // Almanac's own topic comes with the workspace, never from the app.
    let own = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "create", "model": "Topic", "id": uuid::Uuid::now_v7(),
                     "data": { "name": "Mine", "kind": "almanac" } }]),
        )
        .await;
    assert!(!own.status.is_success(), "{}", own.text);

    let topic = uuid::Uuid::now_v7();
    let made = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "create", "model": "Topic", "id": topic,
                     "data": { "name": "Pilze", "sticks_to": "Pilze sammeln", "kind": "topic" } }]),
        )
        .await;
    assert!(made.status.is_success(), "{}", made.text);

    let set_up = app
        .post(
            &format!("/api/w/{home}/topics/{topic}/set_up"),
            Some(&ada),
            json!({}),
        )
        .await;
    assert_eq!(set_up.status, StatusCode::OK, "{}", set_up.text);

    // With no Claude it starts as a collection, around what they wrote.
    let mut blocks: Vec<Value> = Vec::new();
    for _ in 0..100 {
        blocks = app
            .synced(&ada, &home, "Block")
            .await
            .into_iter()
            .filter(|b| b["topic_id"] == json!(topic))
            .collect();
        if !blocks.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let titles: Vec<&str> = blocks.iter().filter_map(|b| b["title"].as_str()).collect();
    assert_eq!(titles, ["To do", "Notes", "You"]);
    let topics = app.synced(&ada, &home, "Topic").await;
    let pilze = topics.iter().find(|t| t["id"] == json!(topic)).unwrap();
    assert_eq!(pilze["sticks_to"], "Pilze sammeln");

    let gone = app
        .post(
            &format!("/api/w/{home}/topics/{}/set_up", uuid::Uuid::now_v7()),
            Some(&ada),
            json!({}),
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND, "{}", gone.text);
    let almanac = topics.iter().find(|t| t["kind"] == "almanac").unwrap();
    let mine = app
        .post(
            &format!(
                "/api/w/{home}/topics/{}/set_up",
                almanac["id"].as_str().unwrap()
            ),
            Some(&ada),
            json!({}),
        )
        .await;
    assert!(!mine.status.is_success(), "{}", mine.text);
}
