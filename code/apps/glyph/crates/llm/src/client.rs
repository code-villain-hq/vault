//! Talking to one llama-server (or any server with the same API).

use std::time::Duration;

use serde_json::{Value, json};

use crate::check::{GlossaryTerm, Violation, glossary_violations, placeholders_kept, terms_in};
use crate::models::Model;
use crate::{parse, prompt};

/// Local models are slow; a long text on a small machine takes minutes.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The most tokens a reply may have.
const MAX_TOKENS: u32 = 256;
/// Extra tries when the text breaks a rule. Beyond two the model hardly
/// ever recovers, and each try takes seconds.
pub const RETRIES: usize = 2;
/// Confidence is capped here when the text still breaks a rule, so it shows
/// up for review.
pub const BROKEN_CONFIDENCE: f64 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("couldn't reach the model at {endpoint}: {source}")]
    Unreachable {
        endpoint: String,
        source: reqwest::Error,
    },
    #[error("the model answered {status}: {body}")]
    Status { status: u16, body: String },
    #[error("the model's reply wasn't understood: {0}")]
    Reply(String),
}

/// What to translate.
pub struct Request<'a> {
    /// Language names, such as "English".
    pub source_lang: &'a str,
    pub target_lang: &'a str,
    pub text: &'a str,
    /// The pair's whole glossary; only terms in the text are used.
    pub glossary: &'a [GlossaryTerm],
    /// Where the text appears in the product.
    pub context: &'a str,
    pub temperature: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Translated {
    pub text: String,
    pub confidence: f64,
    /// Terms still left out after the retries.
    pub violations: Vec<Violation>,
    /// Whether placeholders were still broken after the retries.
    pub placeholders_broken: bool,
}

#[derive(Clone)]
pub struct Client {
    endpoint: String,
    model: String,
    chat: bool,
    http: reqwest::Client,
}

/// `localhost` as 127.0.0.1: on macOS it often resolves to ::1 first, where
/// llama-server doesn't listen.
pub fn normalize_endpoint(endpoint: &str) -> String {
    let s = endpoint.trim().trim_end_matches('/');
    let Some((scheme, rest)) = s.split_once("://") else {
        return s.to_string();
    };
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(']') || name.ends_with(']') => (name, Some(port)),
        _ => (host, None),
    };
    if !name.eq_ignore_ascii_case("localhost") {
        return s.to_string();
    }
    let port = port.unwrap_or(if scheme == "https" { "443" } else { "80" });
    format!("{scheme}://127.0.0.1:{port}{path}")
}

impl Client {
    pub fn new(model: &Model) -> Client {
        Client {
            endpoint: normalize_endpoint(&model.endpoint),
            model: model.name.clone(),
            chat: model.chat_mode,
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .expect("an HTTP client"),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Whether the server answers `/health` within two seconds.
    pub async fn reachable(&self) -> bool {
        let url = format!("{}/health", self.endpoint);
        matches!(
            self.http.get(url).timeout(Duration::from_secs(2)).send().await,
            Ok(r) if r.status().is_success()
        )
    }

    /// Translates `request.text`, retrying while the text drops a
    /// placeholder or a glossary term. The first clean text wins; otherwise
    /// the last one comes back with what's wrong with it, and a confidence of
    /// at most [`BROKEN_CONFIDENCE`].
    pub async fn translate(&self, request: &Request<'_>) -> Result<Translated, LlmError> {
        let temperature = if request.temperature <= 0.0 {
            0.2
        } else {
            request.temperature.min(2.0)
        };
        let glossary: Vec<GlossaryTerm> = terms_in(request.text, request.glossary)
            .into_iter()
            .cloned()
            .collect();
        let job = prompt::Job {
            source_lang: request.source_lang,
            target_lang: request.target_lang,
            text: request.text,
            glossary: &glossary,
            context: request.context,
        };

        let mut prompt = prompt::translate(&job, self.chat);
        let mut attempt = 0;
        loop {
            let reply = self.call(&prompt, temperature).await?;
            tracing::debug!(attempt, reply, "model replied");
            let (text, confidence) = parse::translation(&reply);
            let kept = placeholders_kept(request.text, &text);
            let violations = glossary_violations(&text, &glossary);
            let clean = kept && violations.is_empty();
            if clean || attempt == RETRIES {
                return Ok(Translated {
                    confidence: if clean {
                        confidence
                    } else {
                        confidence.min(BROKEN_CONFIDENCE)
                    },
                    text,
                    violations,
                    placeholders_broken: !kept,
                });
            }
            prompt = if kept {
                prompt::fix_glossary(&job, &text, &violations, self.chat)
            } else {
                prompt::fix_placeholders(&job, &text, self.chat)
            };
            attempt += 1;
        }
    }

    /// Grammatical forms of a glossary term, without the term itself. An
    /// empty list is a fair answer: some languages hardly inflect.
    pub async fn variants(
        &self,
        source_lang: &str,
        target_lang: &str,
        source_term: &str,
        target_term: &str,
    ) -> Result<Vec<String>, LlmError> {
        let prompt = prompt::variants(source_lang, target_lang, source_term, target_term);
        let reply = self.call(&prompt, 0.1).await?;
        Ok(parse::variants(&reply, target_term))
    }

    async fn call(&self, prompt: &str, temperature: f64) -> Result<String, LlmError> {
        let (path, body) = if self.chat {
            (
                "/v1/chat/completions",
                json!({
                    "model": self.model,
                    "stream": false,
                    "temperature": temperature,
                    "max_tokens": MAX_TOKENS,
                    "messages": [
                        { "role": "system", "content": prompt::SYSTEM },
                        { "role": "user", "content": format!("{prompt}\n\nRespond with ONLY JSON. No explanation.") },
                    ],
                }),
            )
        } else {
            (
                "/completion",
                json!({
                    "prompt": prompt,
                    "stream": false,
                    "temperature": temperature,
                    "n_predict": MAX_TOKENS,
                    "stop": ["###", "Source:", "Translate from", "Text to translate:"],
                }),
            )
        };
        let unreachable = |source| LlmError::Unreachable {
            endpoint: self.endpoint.clone(),
            source,
        };
        let response = self
            .http
            .post(format!("{}{path}", self.endpoint))
            .json(&body)
            .send()
            .await
            .map_err(unreachable)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Status {
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }
        let reply: Value = response
            .json()
            .await
            .map_err(|e| LlmError::Reply(e.to_string()))?;
        parse::generated_text(&reply)
            .map(str::to_string)
            .ok_or_else(|| LlmError::Reply("no text in it".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;

    /// A model that gives these replies in turn, then the last one again.
    async fn fake_model(replies: &[&str], chat: bool) -> (Client, Arc<AtomicUsize>) {
        let replies: Vec<String> = replies.iter().map(|r| r.to_string()).collect();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let reply = move || {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            let text = replies[n.min(replies.len() - 1)].clone();
            async move { Json(json!({ "choices": [{ "text": text, "message": { "content": text } }] })) }
        };
        let app = Router::new()
            .route("/completion", post(reply.clone()))
            .route("/v1/chat/completions", post(reply));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let model = Model {
            name: "test-model".into(),
            endpoint: format!("http://localhost:{}", address.port()),
            chat_mode: chat,
            ..Model::default()
        };
        (Client::new(&model), calls)
    }

    fn email() -> Vec<GlossaryTerm> {
        vec![GlossaryTerm {
            source_term: "email".into(),
            target_term: "e-post".into(),
            target_variants: vec![],
        }]
    }

    fn request<'a>(text: &'a str, glossary: &'a [GlossaryTerm]) -> Request<'a> {
        Request {
            source_lang: "English",
            target_lang: "Swedish",
            text,
            glossary,
            context: "",
            temperature: 0.2,
        }
    }

    #[tokio::test]
    async fn a_glossary_miss_is_retried() {
        let (client, calls) = fake_model(
            &[
                r#"{"translation":"E-postadressen är redan registrerad.","confidence":0.9}"#,
                r#"{"translation":"E-post är redan registrerad.","confidence":0.9}"#,
            ],
            false,
        )
        .await;
        let glossary = email();
        let out = client
            .translate(&request("Email is already registered.", &glossary))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(out.text, "E-post är redan registrerad.");
        assert_eq!(out.confidence, 0.9);
        assert!(out.violations.is_empty());
    }

    #[tokio::test]
    async fn after_the_retries_the_miss_is_reported_and_confidence_capped() {
        let (client, calls) = fake_model(
            &[r#"{"translation":"E-postadressen är redan registrerad.","confidence":0.9}"#],
            true,
        )
        .await;
        let glossary = email();
        let out = client
            .translate(&request("Email is already registered.", &glossary))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1 + RETRIES);
        assert_eq!(out.violations.len(), 1);
        assert_eq!(out.confidence, BROKEN_CONFIDENCE);
    }

    #[tokio::test]
    async fn a_clean_text_is_not_retried() {
        let (client, calls) = fake_model(
            &[r#"{"translation":"E-post är redan registrerad.","confidence":0.9}"#],
            false,
        )
        .await;
        let glossary = email();
        let out = client
            .translate(&request("Email is already registered.", &glossary))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(out.confidence, 0.9);
    }

    #[tokio::test]
    async fn dropped_and_invented_placeholders_are_retried() {
        let (client, calls) = fake_model(
            &[
                r#"{"translation":"Hej name!","confidence":0.9}"#,
                r#"{"translation":"Hej {{name}}!","confidence":0.85}"#,
            ],
            false,
        )
        .await;
        let out = client
            .translate(&request("Hello {{name}}!", &[]))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(out.text, "Hej {{name}}!");
        assert!(!out.placeholders_broken);

        let (client, calls) = fake_model(
            &[
                r#"{"translation":"{{count}} {{name}}s","confidence":0.9}"#,
                r#"{"translation":"Ankunftsortname fehlt.","confidence":0.85}"#,
            ],
            true,
        )
        .await;
        let out = client
            .translate(&request("Arrival location name missing.", &[]))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(out.text, "Ankunftsortname fehlt.");
    }

    #[test]
    fn localhost_becomes_ipv4_loopback() {
        assert_eq!(
            normalize_endpoint("http://localhost:8081/"),
            "http://127.0.0.1:8081"
        );
        assert_eq!(
            normalize_endpoint("http://LOCALHOST"),
            "http://127.0.0.1:80"
        );
        assert_eq!(
            normalize_endpoint("http://10.0.0.2:8080"),
            "http://10.0.0.2:8080"
        );
        assert_eq!(normalize_endpoint("http://[::1]:8080"), "http://[::1]:8080");
    }
}
