//! Claude, through Anthropic's Messages API. Rust has no Anthropic SDK, so
//! this speaks the HTTP API itself: a streamed answer for a talk, and JSON
//! for what the page keeps and how a topic starts.
//!
//! The key is set in Settings and kept in `claude_key`, which never syncs;
//! `claude.api_key` in the settings (or `ANTHROPIC_API_KEY`) stands in
//! when none is set there. Web search stays off: Almanac answers from what
//! Claude knows and what the topic has kept. Declined requests fall back to
//! the model Anthropic recommends (`fallbacks: "default"`).

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::photos::Image;

pub const MODEL: &str = "claude-opus-5-5";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";

/// Why Claude gave no answer, in words for the talk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem(pub String);

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One turn of a talk, as Claude reads it.
pub struct Turn {
    /// `user` or `assistant`.
    pub role: &'static str,
    pub text: String,
    /// The photos a question came with.
    pub images: Vec<Image>,
}

#[derive(Clone)]
pub struct Claude {
    http: reqwest::Client,
    url: String,
    from_settings: Option<String>,
}

impl Claude {
    /// `from_settings` is the key the server's settings name, if any.
    pub fn new(from_settings: Option<String>) -> Self {
        // A stalled answer ends: ten minutes with nothing from the API, as
        // its own SDKs wait.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(600))
            .build()
            .unwrap_or_default();
        Claude {
            http,
            url: MESSAGES_URL.into(),
            from_settings: from_settings.filter(|k| !k.trim().is_empty()),
        }
    }

    /// Sends requests to `url` instead of Anthropic's, for tests.
    pub fn at(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// The key to call Claude with: the one set in Settings, else the
    /// settings file's.
    pub async fn key(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let stored: Option<String> = sqlx::query_scalar(crate::keys::STORED)
            .fetch_optional(pool)
            .await?;
        Ok(stored.or_else(|| self.from_settings.clone()))
    }

    /// Whether a key comes from the settings file.
    pub fn has_settings_key(&self) -> bool {
        self.from_settings.is_some()
    }

    fn request(&self, key: &str, body: &Value) -> reqwest::RequestBuilder {
        self.http
            .post(&self.url)
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .header("content-type", "application/json")
            .json(body)
    }

    /// Streams an answer to the talk's last turn, handing each piece of
    /// text to `on_text` as it comes. Returns the whole answer.
    pub async fn answer(
        &self,
        key: &str,
        system: &str,
        turns: &[Turn],
        mut on_text: impl FnMut(&str),
    ) -> Result<String, Problem> {
        let body = json!({
            "model": MODEL,
            "max_tokens": 16000,
            "stream": true,
            "system": system,
            "messages": messages(turns),
            "thinking": { "type": "adaptive" },
            "output_config": { "effort": "medium" },
            "fallbacks": "default",
        });
        let mut response = self.request(key, &body).send().await.map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(refused(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }
        // Bytes until an event ends, so a letter split across chunks stays whole.
        let mut buffer: Vec<u8> = Vec::new();
        let mut text = String::new();
        let mut stop_reason: Option<String> = None;
        while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.windows(2).position(|w| w == b"\n\n") {
                let event: Vec<u8> = buffer.drain(..end + 2).collect();
                let event = String::from_utf8_lossy(&event);
                let Some(data) = event
                    .lines()
                    .find_map(|line| line.strip_prefix("data:"))
                    .and_then(|d| serde_json::from_str::<Value>(d.trim()).ok())
                else {
                    continue;
                };
                match data["type"].as_str() {
                    Some("content_block_delta") if data["delta"]["type"] == "text_delta" => {
                        if let Some(piece) = data["delta"]["text"].as_str() {
                            text.push_str(piece);
                            on_text(piece);
                        }
                    }
                    Some("message_delta") => {
                        if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                            stop_reason = Some(reason.into());
                        }
                    }
                    Some("error") => return Err(api_error(&data)),
                    _ => {}
                }
            }
        }
        finished(stop_reason.as_deref(), &text)?;
        Ok(text)
    }

    /// Asks for JSON in the shape of `schema`, as structured output.
    pub async fn json<T: DeserializeOwned>(
        &self,
        key: &str,
        system: &str,
        prompt: &str,
        schema: Value,
    ) -> Result<T, Problem> {
        let body = json!({
            "model": MODEL,
            "max_tokens": 12000,
            "system": system,
            "messages": [{ "role": "user", "content": prompt }],
            "thinking": { "type": "adaptive" },
            "output_config": {
                "effort": "low",
                "format": { "type": "json_schema", "schema": schema },
            },
            "fallbacks": "default",
        });
        let response = self.request(key, &body).send().await.map_err(unreachable)?;
        let status = response.status();
        let body = response.text().await.map_err(unreachable)?;
        if !status.is_success() {
            return Err(refused(status, body));
        }
        let message: Value = serde_json::from_str(&body)
            .map_err(|_| Problem("Claude's answer wasn't readable.".into()))?;
        let text: String = message["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect();
        finished(message["stop_reason"].as_str(), &text)?;
        serde_json::from_str(&text)
            .map_err(|_| Problem("Claude's answer wasn't in the shape Almanac asked for.".into()))
    }
}

/// The turns as the API wants them: starting with a question, taking
/// turns, with what one side said twice in a row joined.
fn messages(turns: &[Turn]) -> Vec<Value> {
    let mut out: Vec<(&str, String, Vec<&Image>)> = Vec::new();
    for turn in turns
        .iter()
        .filter(|t| !t.text.trim().is_empty() || !t.images.is_empty())
    {
        match out.last_mut() {
            Some((role, text, images)) if *role == turn.role => {
                if !text.is_empty() && !turn.text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&turn.text);
                images.extend(&turn.images);
            }
            None if turn.role != "user" => {}
            _ => out.push((turn.role, turn.text.clone(), turn.images.iter().collect())),
        }
    }
    out.into_iter()
        .map(|(role, text, images)| {
            if images.is_empty() {
                return json!({ "role": role, "content": text });
            }
            // The photos first, then what they asked about them.
            let mut content: Vec<Value> = images
                .into_iter()
                .map(|image| {
                    json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": image.media_type, "data": image.data },
                    })
                })
                .collect();
            if !text.is_empty() {
                content.push(json!({ "type": "text", "text": text }));
            }
            json!({ "role": role, "content": content })
        })
        .collect()
}

fn finished(stop_reason: Option<&str>, text: &str) -> Result<(), Problem> {
    match stop_reason {
        Some("refusal") => Err(Problem(
            "Claude declined to answer that. Try asking it another way.".into(),
        )),
        Some("max_tokens") if text.is_empty() => {
            Err(Problem("Claude ran out of room before it answered.".into()))
        }
        _ if text.is_empty() => Err(Problem("Claude gave no answer.".into())),
        _ => Ok(()),
    }
}

fn unreachable(error: reqwest::Error) -> Problem {
    tracing::warn!(%error, "couldn't reach Claude");
    Problem("Almanac couldn't reach Claude. Check the connection and ask again.".into())
}

fn refused(status: reqwest::StatusCode, body: String) -> Problem {
    let data: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    match status.as_u16() {
        401 | 403 => Problem("Claude didn't take the key. Check it in Settings → Claude.".into()),
        429 => Problem("Claude is busy for this key right now. Ask again in a minute.".into()),
        529 | 503 => Problem("Claude is overloaded right now. Ask again in a minute.".into()),
        _ => {
            tracing::warn!(%status, %body, "Claude refused the request");
            api_error(&data)
        }
    }
}

fn api_error(data: &Value) -> Problem {
    match data["error"]["message"].as_str() {
        Some(message) => Problem(format!("Claude said: {message}")),
        None => Problem("Claude couldn't answer. Ask again in a minute.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_start_with_a_question_and_alternate() {
        let turn = |role, text: &str| Turn {
            role,
            text: text.into(),
            images: Vec::new(),
        };
        let out = messages(&[
            turn("assistant", "Hello"),
            turn("user", "Wie oft gießen?"),
            turn("user", "Die Monstera."),
            turn("assistant", ""),
            turn("assistant", "Alle 7 Tage."),
        ]);
        assert_eq!(
            out,
            vec![
                json!({ "role": "user", "content": "Wie oft gießen?\n\nDie Monstera." }),
                json!({ "role": "assistant", "content": "Alle 7 Tage." }),
            ]
        );
    }

    #[test]
    fn photos_come_before_the_question() {
        let image = Image {
            id: uuid::Uuid::nil(),
            media_type: "image/png".into(),
            data: "AAAA".into(),
        };
        let out = messages(&[Turn {
            role: "user",
            text: "Welches Shampoo ist das?".into(),
            images: vec![image],
        }]);
        assert_eq!(out[0]["content"][0]["type"], "image");
        assert_eq!(out[0]["content"][0]["source"]["media_type"], "image/png");
        assert_eq!(out[0]["content"][1]["text"], "Welches Shampoo ist das?");
    }

    #[test]
    fn a_refusal_or_nothing_is_a_problem() {
        assert!(finished(Some("refusal"), "partial").is_err());
        assert!(finished(Some("end_turn"), "").is_err());
        assert!(finished(Some("end_turn"), "Yes.").is_ok());
    }
}
