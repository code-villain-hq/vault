//! One run of Forge AI, by whoever runs it: the server's key, through the
//! Messages API ([`by_key`]), or Claude Code on a Mac ([`by_claude_code`],
//! see `here.rs`). Both get the same prompt, first message and tools
//! ([`Prepared`]), and the run ends the same way.

use std::time::Duration;

use code_villain_claude::Answer;
use forge_ai::{Agent, ModelProvider, Stop, ToolHandler};
use serde_json::{Value, json};

/// What a run comes to.
pub struct RunResult {
    pub summary: String,
    pub error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub suggestions: i32,
    pub messages: Vec<Value>,
}

/// A run's tools, and what they gathered.
pub trait RunTools: ToolHandler {
    /// What the model said it did, with `finish` (or `suggest`).
    fn summary(&self) -> Option<String>;
    fn suggestions(&self) -> i32;
}

/// A run, ready for whoever runs it.
pub struct Prepared {
    pub system: &'static str,
    /// The first message: what to look at.
    pub context: String,
    /// The tools, as the Messages API takes them.
    pub definitions: Vec<Value>,
    pub max_turns: usize,
    pub max_tokens: u32,
    /// How long Claude Code on a Mac may take.
    pub timeout: Duration,
    pub tools: Box<dyn RunTools>,
}

/// Runs it with the server's key.
pub async fn by_key(provider: &dyn ModelProvider, prepared: Prepared) -> Result<RunResult, String> {
    let Prepared {
        system,
        context,
        definitions,
        max_turns,
        max_tokens,
        mut tools,
        ..
    } = prepared;
    let agent_loop = Agent {
        system: system.into(),
        tools: definitions,
        max_turns,
        max_tokens,
    };
    let outcome = agent_loop
        .run(provider, context, tools.as_mut())
        .await
        .map_err(|e| e.to_string())?;
    let error = match &outcome.stop {
        Stop::Done => None,
        Stop::Refused(why) => Some(declined(why.as_deref())),
        Stop::TurnLimit => Some(turn_limit(max_turns)),
        Stop::MaxTokens => Some(OUTPUT_LIMIT.into()),
    };
    Ok(RunResult {
        summary: tools
            .summary()
            .unwrap_or_else(|| last_text(&outcome.messages)),
        error,
        input_tokens: outcome.usage.input_tokens
            + outcome.usage.cache_creation_input_tokens
            + outcome.usage.cache_read_input_tokens,
        output_tokens: outcome.usage.output_tokens,
        cost_usd: outcome.usage.cost_usd(provider.model()),
        suggestions: tools.suggestions(),
        messages: outcome.messages,
    })
}

/// What Claude Code's answer comes to: the tools' summary, or its last
/// words. It runs as the person is signed in to it, so the run has no
/// tokens or cost of the server's; its conversation is the first message
/// and the answer.
pub fn by_claude_code(
    context: String,
    max_turns: usize,
    answer: Answer,
    tools: &dyn RunTools,
) -> RunResult {
    let error = match answer.stop.as_str() {
        "refusal" => Some(declined(None)),
        "max_turn_requests" => Some(turn_limit(max_turns)),
        "max_tokens" => Some(OUTPUT_LIMIT.into()),
        "cancelled" => Some(STOPPED.into()),
        _ => None,
    };
    let text = answer.text.trim().to_string();
    RunResult {
        summary: tools.summary().unwrap_or_else(|| text.clone()),
        error,
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
        suggestions: tools.suggestions(),
        messages: vec![
            json!({ "role": "user", "content": context }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": text }] }),
        ],
    }
}

const OUTPUT_LIMIT: &str = "A response hit the output limit.";
pub const STOPPED: &str = "Claude Code on your Mac stopped before it finished.";

fn declined(why: Option<&str>) -> String {
    format!(
        "The model declined to continue{}",
        why.map(|w| format!(": {w}")).unwrap_or_default()
    )
}

fn turn_limit(turns: usize) -> String {
    format!("Stopped after {turns} turns without finishing.")
}

/// The last thing the model said, for runs that ended without `finish`.
pub(super) fn last_text(messages: &[Value]) -> String {
    messages
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .find_map(|block| {
            (block["type"] == "text").then(|| block["text"].as_str().unwrap_or("").to_string())
        })
        .unwrap_or_default()
}
