//! The tool-use loop: send the conversation, run the tools Claude asks
//! for, send their results back, until Claude is done.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::provider::{AiError, ModelProvider};

/// What a tool returns to the model.
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        ToolOutput {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        ToolOutput {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Runs the tools of one agent run.
pub trait ToolHandler: Send {
    fn call<'a>(&'a mut self, name: &'a str, input: &'a Value) -> BoxFuture<'a, ToolOutput>;
    /// True once the agent has said it's finished (for example by calling a
    /// `finish` tool); the loop stops after that turn.
    fn finished(&self) -> bool;
}

/// Token use across a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
}

impl Usage {
    fn add(&mut self, usage: &Value) {
        let get = |field: &str| usage[field].as_i64().unwrap_or(0);
        self.input_tokens += get("input_tokens");
        self.output_tokens += get("output_tokens");
        self.cache_creation_input_tokens += get("cache_creation_input_tokens");
        self.cache_read_input_tokens += get("cache_read_input_tokens");
    }

    /// Cost in US dollars at the model's list price; 0 for models not listed.
    pub fn cost_usd(&self, model: &str) -> f64 {
        // (input, output) per million tokens. Cache writes cost 1.25x input,
        // cache reads 0.1x... except where a model lists its own read price.
        let (input, output, cache_read) = match model {
            "claude-opus-5-5" => (4.0, 20.0, 0.20),
            "claude-sonnet-5-5" | "claude-sonnet-5" => (2.0, 10.0, 0.20),
            "claude-opus-5" | "claude-opus-4-8" | "claude-opus-4-7" => (5.0, 25.0, 0.50),
            "claude-haiku-5-5" => (0.10, 0.50, 0.01),
            _ => return 0.0,
        };
        let million = 1_000_000.0;
        (self.input_tokens as f64 * input
            + self.cache_creation_input_tokens as f64 * input * 1.25
            + self.cache_read_input_tokens as f64 * cache_read
            + self.output_tokens as f64 * output)
            / million
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// Claude finished, or a tool said the work is done.
    Done,
    /// A safety classifier declined, even after any fallback.
    Refused(Option<String>),
    TurnLimit,
    /// A response hit the output limit.
    MaxTokens,
}

pub struct AgentRun {
    /// The whole conversation, as sent and received.
    pub messages: Vec<Value>,
    pub usage: Usage,
    pub stop: Stop,
}

pub struct Agent {
    pub system: String,
    /// Tool definitions, as the Messages API takes them.
    pub tools: Vec<Value>,
    pub max_turns: usize,
    pub max_tokens: u32,
}

impl Agent {
    pub async fn run(
        &self,
        provider: &dyn ModelProvider,
        first_message: String,
        tools: &mut dyn ToolHandler,
    ) -> Result<AgentRun, AiError> {
        let mut messages = vec![json!({ "role": "user", "content": first_message })];
        let mut usage = Usage::default();

        for _ in 0..self.max_turns {
            let request = json!({
                "max_tokens": self.max_tokens,
                "system": self.system,
                "tools": self.tools,
                "messages": messages,
            });
            let response = provider.send(&request).await?;
            usage.add(&response["usage"]);
            // Append the content exactly as returned: thinking and fallback
            // blocks must come back unchanged on the next turn.
            messages.push(json!({ "role": "assistant", "content": response["content"].clone() }));

            match response["stop_reason"].as_str().unwrap_or("") {
                "refusal" => {
                    let why = response["stop_details"]["explanation"]
                        .as_str()
                        .map(str::to_string);
                    return Ok(AgentRun {
                        messages,
                        usage,
                        stop: Stop::Refused(why),
                    });
                }
                "max_tokens" => {
                    return Ok(AgentRun {
                        messages,
                        usage,
                        stop: Stop::MaxTokens,
                    });
                }
                // A server-side loop paused; sending the conversation again resumes it.
                "pause_turn" => continue,
                "tool_use" => {
                    let mut results = Vec::new();
                    for block in response["content"].as_array().into_iter().flatten() {
                        if block["type"] != "tool_use" {
                            continue;
                        }
                        let name = block["name"].as_str().unwrap_or("");
                        let output = tools.call(name, &block["input"]).await;
                        results.push(json!({
                            "type": "tool_result",
                            "tool_use_id": block["id"],
                            "content": output.content,
                            "is_error": output.is_error,
                        }));
                    }
                    // All results go back together, in one message.
                    messages.push(json!({ "role": "user", "content": results }));
                    if tools.finished() {
                        return Ok(AgentRun {
                            messages,
                            usage,
                            stop: Stop::Done,
                        });
                    }
                }
                _ => {
                    return Ok(AgentRun {
                        messages,
                        usage,
                        stop: Stop::Done,
                    });
                }
            }
        }
        Ok(AgentRun {
            messages,
            usage,
            stop: Stop::TurnLimit,
        })
    }
}
