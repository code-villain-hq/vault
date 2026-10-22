use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("the model API refused the request: {0}")]
    Request(String),
    #[error("couldn't reach the model API: {0}")]
    Network(String),
    #[error("{0}")]
    Other(String),
}

/// Sends one Messages API request (everything but the model settings) and
/// returns the response message as JSON.
pub trait ModelProvider: Send + Sync {
    fn send<'a>(&'a self, request: &'a Value) -> BoxFuture<'a, Result<Value, AiError>>;
    /// The model's id, for records and pricing.
    fn model(&self) -> &str;
}

/// Claude, over the Messages API. Rust has no official Anthropic SDK, so
/// this speaks HTTP directly.
pub struct ClaudeProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    effort: String,
    base_url: String,
}

const API_VERSION: &str = "2023-06-01";
/// Lets the API retry a declined request on the model Anthropic recommends
/// for that kind of refusal, instead of returning the refusal.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

impl ClaudeProvider {
    pub fn new(api_key: String, model: String, effort: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .expect("an HTTP client with default settings builds");
        ClaudeProvider {
            client,
            api_key,
            model,
            effort,
            base_url: "https://api.anthropic.com".into(),
        }
    }

    /// From the server's settings, looked up by their names in the
    /// environment: `FORGE_AI_API_KEY` (or `ANTHROPIC_API_KEY`),
    /// `FORGE_AI_MODEL`, `FORGE_AI_EFFORT` and `FORGE_AI_BASE_URL` (or
    /// `ANTHROPIC_BASE_URL`). `None` without an API key, which turns the AI
    /// off.
    pub fn from_settings(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let var = |name: &str| var(name).filter(|v| !v.trim().is_empty());
        let api_key = var("FORGE_AI_API_KEY").or_else(|| var("ANTHROPIC_API_KEY"))?;
        let model = var("FORGE_AI_MODEL").unwrap_or_else(|| "claude-opus-5-5".into());
        let effort = var("FORGE_AI_EFFORT").unwrap_or_else(|| "high".into());
        let mut provider = ClaudeProvider::new(api_key, model, effort);
        if let Some(base_url) = var("FORGE_AI_BASE_URL").or_else(|| var("ANTHROPIC_BASE_URL")) {
            provider.base_url = base_url.trim_end_matches('/').to_string();
        }
        Some(provider)
    }

    fn body(&self, request: &Value) -> Value {
        let mut body = request.clone();
        if let Value::Object(map) = &mut body {
            map.insert("model".into(), json!(self.model));
            // Claude Opus 5.5 defaults to medium effort; drift checks are
            // careful reading of code, so ask for more.
            map.insert("output_config".into(), json!({ "effort": self.effort }));
            // The system prompt and tools repeat on every turn of the loop.
            map.insert("cache_control".into(), json!({ "type": "ephemeral" }));
            map.insert("fallbacks".into(), json!("default"));
        }
        body
    }
}

impl ModelProvider for ClaudeProvider {
    fn model(&self) -> &str {
        &self.model
    }

    fn send<'a>(&'a self, request: &'a Value) -> BoxFuture<'a, Result<Value, AiError>> {
        Box::pin(async move {
            let body = self.body(request);
            let mut delay = Duration::from_secs(2);
            for attempt in 0..4 {
                let response = self
                    .client
                    .post(format!("{}/v1/messages", self.base_url))
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", API_VERSION)
                    .header("anthropic-beta", FALLBACK_BETA)
                    .json(&body)
                    .send()
                    .await;
                let response = match response {
                    Ok(response) => response,
                    Err(e) if attempt < 3 => {
                        tracing::warn!(error = %e, "model API unreachable; retrying");
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                    Err(e) => return Err(AiError::Network(e.to_string())),
                };
                let status = response.status();
                // Rate limits, overload and server errors are worth another try.
                if (status.as_u16() == 429 || status.is_server_error()) && attempt < 3 {
                    let wait = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map_or(delay, Duration::from_secs);
                    tracing::warn!(%status, "model API busy; retrying in {wait:?}");
                    tokio::time::sleep(wait).await;
                    delay *= 2;
                    continue;
                }
                let text = response
                    .text()
                    .await
                    .map_err(|e| AiError::Network(e.to_string()))?;
                if !status.is_success() {
                    let message = serde_json::from_str::<Value>(&text)
                        .ok()
                        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                        .unwrap_or(text);
                    return Err(AiError::Request(format!("{status}: {message}")));
                }
                return serde_json::from_str(&text).map_err(|e| AiError::Other(e.to_string()));
            }
            Err(AiError::Other("gave up after retries".into()))
        })
    }
}

/// Plays back prepared responses, for tests. Records every request.
pub struct ScriptedProvider {
    responses: Mutex<VecDeque<Value>>,
    pub requests: Mutex<Vec<Value>>,
}

impl ScriptedProvider {
    pub fn new(responses: Vec<Value>) -> Self {
        ScriptedProvider {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// A response that calls one tool.
    pub fn tool_call(id: &str, name: &str, input: Value) -> Value {
        json!({
            "role": "assistant",
            "stop_reason": "tool_use",
            "content": [{ "type": "tool_use", "id": id, "name": name, "input": input }],
            "usage": { "input_tokens": 1000, "output_tokens": 100 },
        })
    }

    /// A response that ends the turn with text.
    pub fn text(text: &str) -> Value {
        json!({
            "role": "assistant",
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": text }],
            "usage": { "input_tokens": 1000, "output_tokens": 50 },
        })
    }
}

impl ModelProvider for ScriptedProvider {
    fn model(&self) -> &str {
        "scripted"
    }

    fn send<'a>(&'a self, request: &'a Value) -> BoxFuture<'a, Result<Value, AiError>> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("not poisoned")
                .push(request.clone());
            self.responses
                .lock()
                .expect("not poisoned")
                .pop_front()
                .ok_or_else(|| AiError::Other("the script ran out of responses".into()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_requests_carry_the_model_settings() {
        let provider = ClaudeProvider::new("key".into(), "claude-opus-5-5".into(), "high".into());
        let body = provider.body(&json!({ "max_tokens": 16000, "messages": [] }));
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["output_config"], json!({ "effort": "high" }));
        assert_eq!(body["cache_control"], json!({ "type": "ephemeral" }));
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["max_tokens"], 16000);
        assert!(
            body.get("thinking").is_none(),
            "adaptive thinking is the default; disabling it is a 400"
        );
        assert!(
            body.get("tool_choice").is_none(),
            "forced tool use is a 400 on this model"
        );
    }
}
