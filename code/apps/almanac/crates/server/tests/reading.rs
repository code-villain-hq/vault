//! Reading a workspace from another app over MCP, with a member's token.

mod common;

use axum::http::StatusCode;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;

/// What a tool says, and whether it failed.
async fn tool(app: &App, token: &str, name: &str, arguments: Value) -> (String, bool) {
    let (status, body) = app
        .read_mcp(
            token,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "name": name, "arguments": arguments } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    (
        result["content"][0]["text"].as_str().unwrap().to_string(),
        result["isError"].as_bool().unwrap(),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_token_reads_the_workspace_until_it_goes(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let reading = format!("/api/w/{home}/reading");
    assert!(app.get(&reading, &ada).await.body["made_at"].is_null());

    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Pflanzen", "template": "collection" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    let block = uuid::Uuid::now_v7();
    let wrote = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "create", "model": "Block", "id": block,
                  "data": { "topic_id": topic, "kind": "paragraph", "title": "Care" } },
                { "action": "create", "model": "Point", "id": uuid::Uuid::now_v7(),
                  "data": { "topic_id": topic, "block_id": block, "text": "Water the Monstera every 7 days." } },
            ]),
        )
        .await;
    assert!(wrote.status.is_success(), "{}", wrote.text);

    let token = app.post(&reading, Some(&ada), json!({})).await.body["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, listed) = app
        .read_mcp(
            &token,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["list_topics", "read_topic", "search"]);

    let (topics, _) = tool(&app, &token, "list_topics", json!({})).await;
    assert!(topics.contains("Pflanzen"), "{topics}");
    let (page, _) = tool(&app, &token, "read_topic", json!({ "topic": "pflanzen" })).await;
    assert!(page.contains("Water the Monstera"), "{page}");
    assert!(!page.contains("How a page is made"), "{page}");
    let (found, _) = tool(&app, &token, "search", json!({ "words": "monstera" })).await;
    assert!(
        found.contains("Pflanzen › Care: Water the Monstera"),
        "{found}"
    );
    let (none, _) = tool(&app, &token, "search", json!({ "words": "100%" })).await;
    assert!(none.starts_with("No page keeps"), "{none}");
    let (missing, failed) = tool(&app, &token, "read_topic", json!({ "topic": "Katzen" })).await;
    assert!(failed && missing.contains("Pflanzen"), "{missing}");
    // Nothing that writes.
    let (_, failed) = tool(&app, &token, "add_block", json!({})).await;
    assert!(failed);
    assert!(!app.get(&reading, &ada).await.body["used_at"].is_null());

    // A new token takes the old one's place, and a revoked one reads nothing.
    let again = app.post(&reading, Some(&ada), json!({})).await.body["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = app
        .read_mcp(
            &token,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let revoked = app.call("DELETE", &reading, Some(&ada), None).await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .read_mcp(
            &again,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
