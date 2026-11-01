//! The app's tools for one question, over MCP's Streamable HTTP, as Claude
//! Code calls them: one JSON-RPC message per POST, one reply, no stream.
//! The app's endpoint finds the question by its bearer ([`crate::questions`])
//! and hands each message to [`reply`]; `None` means a notification, which
//! it answers with 202 Accepted.

use std::future::Future;

use serde_json::{Value, json};

use crate::acp::App;

/// The MCP version the replies speak.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// What a tool said. What's wrong comes back as its result, in words, so
/// Claude can fix it and call again.
pub struct Called {
    pub text: String,
    pub failed: bool,
}

impl Called {
    pub fn ok(text: impl Into<String>) -> Self {
        Called {
            text: text.into(),
            failed: false,
        }
    }

    pub fn failed(text: impl Into<String>) -> Self {
        Called {
            text: text.into(),
            failed: true,
        }
    }
}

/// One question's tools.
pub trait Tools {
    /// The tools, as MCP lists them: `name`, `description`, `inputSchema`.
    fn list(&self) -> Vec<Value>;
    /// Runs one.
    fn call(&self, name: &str, arguments: Value) -> impl Future<Output = Called> + Send;
}

/// The reply to one message, or `None` for a notification.
pub async fn reply(app: App, message: &Value, tools: &impl Tools) -> Option<Value> {
    let id = message["id"].clone();
    if id.is_null() {
        // A notification, such as `notifications/initialized`.
        return None;
    }
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": app.key, "title": app.name, "version": app.version },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools.list() }),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or("");
            let arguments = match &message["params"]["arguments"] {
                Value::Null => json!({}),
                arguments => arguments.clone(),
            };
            let called = tools.call(name, arguments).await;
            tracing::info!(
                tool = name,
                failed = called.failed,
                "Claude Code called a tool"
            );
            json!({ "content": [{ "type": "text", "text": called.text }], "isError": called.failed })
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("{} doesn't offer that.", app.name) },
            }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// A tool as the Messages API defines it (`input_schema`), as MCP lists it.
pub fn listed(definition: &Value) -> Value {
    json!({
        "name": definition["name"],
        "description": definition["description"],
        "inputSchema": definition["input_schema"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORGE: App = App {
        key: "forge",
        name: "Forge",
        version: "0.0.0",
    };

    struct Echo;

    impl Tools for Echo {
        fn list(&self) -> Vec<Value> {
            vec![listed(&json!({
                "name": "echo",
                "description": "Says it back.",
                "input_schema": { "type": "object" },
            }))]
        }

        async fn call(&self, name: &str, arguments: Value) -> Called {
            match name {
                "echo" => Called::ok(arguments["text"].as_str().unwrap_or_default()),
                other => Called::failed(format!("There's no {other} here.")),
            }
        }
    }

    #[tokio::test]
    async fn it_lists_and_calls_the_tools() {
        let started = reply(
            FORGE,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            &Echo,
        )
        .await
        .unwrap();
        assert_eq!(started["result"]["serverInfo"]["name"], "forge");
        assert_eq!(started["result"]["protocolVersion"], PROTOCOL_VERSION);

        let listed = reply(
            FORGE,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            &Echo,
        )
        .await
        .unwrap();
        assert_eq!(listed["result"]["tools"][0]["name"], "echo");
        assert_eq!(
            listed["result"]["tools"][0]["inputSchema"]["type"],
            "object"
        );

        let called = reply(
            FORGE,
            &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                     "params": { "name": "echo", "arguments": { "text": "hi" } } }),
            &Echo,
        )
        .await
        .unwrap();
        assert_eq!(called["id"], 3);
        assert_eq!(called["result"]["content"][0]["text"], "hi");
        assert_eq!(called["result"]["isError"], false);

        let unknown = reply(
            FORGE,
            &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                     "params": { "name": "rm" } }),
            &Echo,
        )
        .await
        .unwrap();
        assert_eq!(unknown["result"]["isError"], true);
    }

    #[tokio::test]
    async fn notifications_get_no_reply_and_the_rest_an_error() {
        let notified = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(reply(FORGE, &notified, &Echo).await.is_none());
        let other = json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" });
        let replied = reply(FORGE, &other, &Echo).await.unwrap();
        assert_eq!(replied["error"]["code"], -32601);
    }
}
