//! Claude Code on this Mac, over the Agent Client Protocol, as Layer's
//! conversations run. For each question Almanac starts `claude-agent-acp`
//! in an empty folder of its own, with Almanac's prompt, none of Claude
//! Code's tools but the web's (when it's on), none of the user's Claude
//! Code settings or CLAUDE.md, and Almanac's own tools over MCP
//! (`crate::mcp`). It answers as the user is signed in to Claude Code:
//! Almanac never signs in for it, and needs no key.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::login_env;
use crate::photos::Image;

/// The ACP adapter Almanac runs, looked for on the login shell's PATH.
pub const ADAPTER: &str = "claude-agent-acp";
/// The command that installs it, as Settings → Claude shows it.
pub const INSTALL: &str = "npm install -g @agentclientprotocol/claude-agent-acp";
/// The name Almanac's tools go by: Claude sees `mcp__almanac__keep_point`.
pub const MCP_NAME: &str = "almanac";
/// Claude Code's own tools Almanac lets it use when the web is on.
pub const WEB_TOOLS: &[&str] = &["WebSearch", "WebFetch"];

const PROTOCOL_VERSION: i64 = 1;
/// Starting the adapter and opening a session, which takes a few seconds.
/// A start that takes longer has hung, and is tried once more.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// One answer, searching included.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(600);
/// How much of the end of its stderr is kept, to say why it stopped.
const STDERR_TAIL: usize = 4096;
/// How long a check that found it ready holds. Settings asks every few
/// seconds while it's open, and each check starts the adapter.
const READY_FOR: Duration = Duration::from_secs(600);

/// Why Claude Code gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The ACP adapter isn't on the login shell's PATH.
    NotInstalled,
    /// Claude Code isn't signed in on this Mac.
    SignedOut,
    /// Anything else, in its words.
    Trouble(String),
}

/// One question for Claude Code.
pub struct Question<'a> {
    /// Almanac's prompt: how to answer, and the page.
    pub system: &'a str,
    /// What they ask, with the talk so far.
    pub prompt: &'a str,
    /// The talk's photos, after the prompt.
    pub images: &'a [Image],
    /// Whether it may search the web and read pages.
    pub web: bool,
    /// Almanac's tools it may use, by their names in [`MCP_NAME`].
    pub tools: &'a [&'a str],
    /// The bearer Almanac's MCP endpoint knows this question by.
    pub token: &'a str,
}

/// What Claude Code answered.
#[derive(Debug, Default)]
pub struct Answer {
    pub text: String,
    /// The pages it read, in order.
    pub read: Vec<String>,
    /// Why it stopped: `end_turn`, `max_tokens`, `refusal`, `cancelled`.
    pub stop: String,
    /// A tool ran after some text: the next text starts a new paragraph.
    paragraph: bool,
}

/// When a check found Claude Code ready, and the adapter's version.
type Ready = Option<(Instant, Option<String>)>;

#[derive(Clone)]
pub struct ClaudeCode {
    /// An empty folder it runs in, so it has no project to look at.
    folder: PathBuf,
    /// Almanac's MCP endpoint (`/api/mcp`) on this Mac's server.
    mcp_url: String,
    /// The adapter to run; looked for on the login PATH when not set.
    program: Option<PathBuf>,
    /// How long a start may take before it's tried again.
    start_timeout: Duration,
    /// The last check that found it ready.
    ready: Arc<Mutex<Ready>>,
}

impl ClaudeCode {
    pub fn new(folder: impl Into<PathBuf>, mcp_url: impl Into<String>) -> Self {
        ClaudeCode {
            folder: folder.into(),
            mcp_url: mcp_url.into(),
            program: None,
            start_timeout: START_TIMEOUT,
            ready: Arc::default(),
        }
    }

    /// Runs `program` instead of the adapter on the PATH, for tests.
    pub fn program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// Gives a start `timeout` instead of half a minute, for tests.
    pub fn start_timeout(mut self, timeout: Duration) -> Self {
        self.start_timeout = timeout;
        self
    }

    /// Whether Claude Code can answer, and the adapter's version: it starts
    /// the adapter and opens a session, which is where a signed-out Claude
    /// Code says so. Nothing is asked, so nothing is spent. A check that
    /// found it ready holds for a while; one that didn't, doesn't.
    pub async fn check(&self) -> Result<Option<String>, Failure> {
        if let Some((at, version)) = self.ready.lock().unwrap().clone()
            && at.elapsed() < READY_FOR
        {
            return Ok(version);
        }
        let checked = self.check_now().await;
        *self.ready.lock().unwrap() = checked
            .as_ref()
            .ok()
            .map(|version| (Instant::now(), version.clone()));
        checked
    }

    async fn check_now(&self) -> Result<Option<String>, Failure> {
        let mut agent = self.start().await?;
        let checked = tokio::time::timeout(self.start_timeout, async {
            let version = agent.initialize().await?;
            let question = Question {
                system: "",
                prompt: "",
                images: &[],
                web: false,
                tools: &[],
                token: "",
            };
            agent
                .new_session(&self.folder, &self.mcp_url, &question, false)
                .await?;
            Ok(version)
        })
        .await
        .unwrap_or_else(|_| {
            Err(Failure::Trouble(
                "Claude Code took too long to start.".into(),
            ))
        });
        agent.stop().await;
        checked
    }

    /// Asks Claude Code one question, handing each piece of the answer to
    /// `piece` as it's written.
    pub async fn ask(
        &self,
        question: &Question<'_>,
        piece: impl FnMut(&str) + Send,
    ) -> Result<Answer, Failure> {
        let answered = self.ask_now(question, piece).await;
        if matches!(answered, Err(Failure::NotInstalled | Failure::SignedOut)) {
            *self.ready.lock().unwrap() = None;
        }
        answered
    }

    async fn ask_now(
        &self,
        question: &Question<'_>,
        mut piece: impl FnMut(&str) + Send,
    ) -> Result<Answer, Failure> {
        let (mut agent, session) = self.open(question).await?;
        let answered = async {
            let mut answer = Answer::default();
            let params = json!({
                "sessionId": session,
                "prompt": prompt(question),
            });
            let done = tokio::time::timeout(
                ANSWER_TIMEOUT,
                agent.call("session/prompt", params, &mut |update| {
                    heard(update, &mut answer, &mut piece);
                }),
            )
            .await
            .unwrap_or_else(|_| {
                Err(Failure::Trouble(
                    "Claude Code took too long to answer.".into(),
                ))
            })?;
            answer.stop = done["stopReason"]
                .as_str()
                .unwrap_or("end_turn")
                .to_string();
            Ok(answer)
        }
        .await;
        agent.stop().await;
        answered
    }

    /// Starts the adapter and opens a session for `question`. A start that
    /// hangs is stopped and made again, once: it has hung while another
    /// start beside it opened in seconds, so a fresh one is likely to work.
    async fn open(&self, question: &Question<'_>) -> Result<(Agent, String), Failure> {
        for attempt in 1..=2 {
            let mut agent = self.start().await?;
            let opened = tokio::time::timeout(self.start_timeout, async {
                agent.initialize().await?;
                agent
                    .new_session(&self.folder, &self.mcp_url, question, true)
                    .await
            })
            .await;
            match opened {
                Ok(Ok(session)) => return Ok((agent, session)),
                Ok(Err(failure)) => {
                    agent.stop().await;
                    return Err(failure);
                }
                Err(_) => {
                    tracing::warn!(
                        attempt,
                        said = agent.last_said(),
                        "Claude Code didn't open a session in time"
                    );
                    agent.stop().await;
                }
            }
        }
        Err(Failure::Trouble(
            "Claude Code took too long to start.".into(),
        ))
    }

    async fn start(&self) -> Result<Agent, Failure> {
        let program = match &self.program {
            Some(program) => program.clone(),
            None => find_adapter().await.ok_or(Failure::NotInstalled)?,
        };
        let env = tokio::task::spawn_blocking(login_env::user_env)
            .await
            .map_err(|e| Failure::Trouble(e.to_string()))?;
        std::fs::create_dir_all(&self.folder).map_err(|e| Failure::Trouble(e.to_string()))?;
        let mut child = Command::new(&program)
            .current_dir(&self.folder)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own group, so stopping it reaches the Claude Code it starts.
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Failure::NotInstalled,
                _ => Failure::Trouble(format!("Claude Code didn't start: {e}")),
            })?;
        let (Some(stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(Failure::Trouble("Claude Code didn't start.".into()));
        };
        let tail = Arc::new(Mutex::new(String::new()));
        {
            let tail = tail.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_str(&String::from_utf8_lossy(&buf[..n]));
                        if tail.len() > STDERR_TAIL {
                            let mut cut = tail.len() - STDERR_TAIL;
                            while !tail.is_char_boundary(cut) {
                                cut += 1;
                            }
                            tail.drain(..cut);
                        }
                    }
                }
            });
        }
        Ok(Agent {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 0,
            tail,
        })
    }
}

/// The adapter on the login shell's PATH.
async fn find_adapter() -> Option<PathBuf> {
    let env = tokio::task::spawn_blocking(login_env::user_env)
        .await
        .ok()?;
    let path = env.get("PATH")?;
    std::env::split_paths(path)
        .map(|dir| dir.join(ADAPTER))
        .find(|candidate| candidate.is_file())
}

/// What a session update adds to the answer.
fn heard(update: &Value, answer: &mut Answer, piece: &mut impl FnMut(&str)) {
    match update["sessionUpdate"].as_str() {
        Some("agent_message_chunk") => {
            if let Some(text) = update["content"]["text"].as_str() {
                if answer.paragraph && !text.trim().is_empty() {
                    answer.paragraph = false;
                    answer.text.push_str("\n\n");
                    piece("\n\n");
                }
                answer.text.push_str(text);
                piece(text);
            }
        }
        Some("tool_call") => {
            answer.paragraph = !answer.text.trim().is_empty();
            if let Some(url) = update["rawInput"]["url"].as_str()
                && url.starts_with("http")
                && !answer.read.iter().any(|u| u == url)
            {
                answer.read.push(url.to_string());
            }
        }
        _ => {}
    }
}

/// A running adapter, spoken to in JSON-RPC, one message per line.
struct Agent {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: i64,
    tail: Arc<Mutex<String>>,
}

/// The prompt's content blocks: the text, then each photo with its number.
fn prompt(question: &Question<'_>) -> Vec<Value> {
    let mut blocks = vec![json!({ "type": "text", "text": question.prompt })];
    // Numbered, as the prompt names them.
    for (n, image) in question.images.iter().enumerate() {
        blocks.push(json!({ "type": "text", "text": format!("Photo {}:", n + 1) }));
        blocks.push(json!({ "type": "image", "mimeType": image.media_type, "data": image.data }));
    }
    blocks
}

impl Agent {
    /// The adapter's version.
    async fn initialize(&mut self) -> Result<Option<String>, Failure> {
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {
                "fs": { "readTextFile": false, "writeTextFile": false },
                "terminal": false,
            },
            "clientInfo": { "name": "almanac", "title": "Almanac", "version": env!("CARGO_PKG_VERSION") },
        });
        let result = self.call("initialize", params, &mut |_| {}).await?;
        Ok(result["agentInfo"]["version"].as_str().map(str::to_string))
    }

    /// Opens a session for `question` and returns its id. `with_tools`
    /// leaves Almanac's MCP endpoint out when only checking.
    async fn new_session(
        &mut self,
        folder: &Path,
        mcp_url: &str,
        question: &Question<'_>,
        with_tools: bool,
    ) -> Result<String, Failure> {
        let mut tools: Vec<&str> = Vec::new();
        if question.web {
            tools.extend(WEB_TOOLS);
        }
        let mut allowed: Vec<String> = tools.iter().map(|t| t.to_string()).collect();
        allowed.extend(
            question
                .tools
                .iter()
                .map(|t| format!("mcp__{MCP_NAME}__{t}")),
        );
        let servers = if with_tools {
            json!([{
                "type": "http",
                "name": MCP_NAME,
                "url": mcp_url,
                "headers": [{ "name": "Authorization", "value": format!("Bearer {}", question.token) }],
            }])
        } else {
            json!([])
        };
        let params = json!({
            "cwd": folder,
            "mcpServers": servers,
            "_meta": {
                "systemPrompt": question.system,
                "claudeCode": {
                    "options": {
                        "tools": tools,
                        "allowedTools": allowed,
                        "settingSources": [],
                    },
                },
            },
        });
        let result = self.call("session/new", params, &mut |_| {}).await?;
        result["sessionId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Failure::Trouble("Claude Code opened no session.".into()))
    }

    async fn send(&mut self, message: Value) -> Result<(), Failure> {
        let mut line = message.to_string();
        line.push('\n');
        match self.stdin.write_all(line.as_bytes()).await {
            Ok(()) => self.stdin.flush().await.map_err(|_| self.ended()),
            Err(_) => Err(self.ended()),
        }
    }

    /// Calls `method` and waits for its result, handing the session updates
    /// that come meanwhile to `update` and answering what the adapter asks.
    async fn call(
        &mut self,
        method: &str,
        params: Value,
        update: &mut (dyn FnMut(&Value) + Send),
    ) -> Result<Value, Failure> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        loop {
            let line = match self.lines.next_line().await {
                Ok(Some(line)) => line,
                _ => return Err(self.ended()),
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match message["method"].as_str() {
                Some("session/update") => update(&message["params"]["update"]),
                Some(asked) if !message["id"].is_null() => {
                    let reply = reply_to(asked, &message["params"]);
                    let mut answer = json!({ "jsonrpc": "2.0", "id": message["id"] });
                    if let (Some(answer), Some(reply)) = (answer.as_object_mut(), reply.as_object())
                    {
                        answer.extend(reply.clone());
                    }
                    self.send(answer).await?;
                }
                // Other notifications, such as its account's status: not ours.
                Some(_) => {}
                None if message["id"] == json!(id) => {
                    if !message["error"].is_null() {
                        return Err(failure_of(&message["error"]));
                    }
                    return Ok(message["result"].clone());
                }
                None => {}
            }
        }
    }

    /// The last line the adapter printed to stderr, if any.
    fn last_said(&self) -> String {
        let tail = self.tail.lock().map(|t| t.clone()).unwrap_or_default();
        tail.lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// Why the adapter stopped talking, from the end of what it printed.
    fn ended(&self) -> Failure {
        let last = self.last_said();
        if last.is_empty() {
            Failure::Trouble("Claude Code stopped before it answered.".into())
        } else {
            Failure::Trouble(format!(
                "Claude Code stopped: {}",
                last.chars().take(300).collect::<String>()
            ))
        }
    }

    /// Stops the adapter and what it started: its whole process group,
    /// before it's waited for, so the group can't have gone to anyone else.
    async fn stop(mut self) {
        if let Some(pid) = self.child.id() {
            let _ = Command::new("/bin/kill")
                .args(["-s", "TERM", "--", &format!("-{pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
        if tokio::time::timeout(Duration::from_secs(3), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }
}

/// Almanac's answer to something the adapter asks. Its tools are only
/// Almanac's and the web's, so a permission it asks for is one of those;
/// the files and the terminal it has no business with.
fn reply_to(method: &str, params: &Value) -> Value {
    match method {
        "session/request_permission" => {
            let allow = params["options"].as_array().and_then(|options| {
                options
                    .iter()
                    .find(|o| o["kind"] == "allow_once")
                    .or_else(|| {
                        options
                            .iter()
                            .find(|o| o["kind"].as_str().is_some_and(|k| k.starts_with("allow")))
                    })
            });
            match allow {
                Some(option) => json!({
                    "result": { "outcome": { "outcome": "selected", "optionId": option["optionId"] } }
                }),
                None => json!({ "result": { "outcome": { "outcome": "cancelled" } } }),
            }
        }
        _ => json!({ "error": { "code": -32601, "message": "Almanac doesn't offer that." } }),
    }
}

/// What a JSON-RPC error from the adapter means for the talk.
fn failure_of(error: &Value) -> Failure {
    let message = error["message"].as_str().unwrap_or("").trim();
    let lower = message.to_lowercase();
    if error["code"] == json!(-32000)
        || lower.contains("authentication")
        || lower.contains("not logged in")
    {
        return Failure::SignedOut;
    }
    if message.is_empty() {
        return Failure::Trouble("Claude Code couldn't answer.".into());
    }
    Failure::Trouble(message.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permission_is_given_once() {
        let params = json!({ "options": [
            { "optionId": "always", "kind": "allow_always" },
            { "optionId": "once", "kind": "allow_once" },
            { "optionId": "no", "kind": "reject_once" },
        ]});
        assert_eq!(
            reply_to("session/request_permission", &params)["result"]["outcome"]["optionId"],
            "once"
        );
        assert!(!reply_to("fs/read_text_file", &json!({}))["error"].is_null());
    }

    #[test]
    fn signed_out_is_told_apart_from_other_trouble() {
        assert_eq!(
            failure_of(&json!({ "code": -32000, "message": "Authentication required" })),
            Failure::SignedOut
        );
        assert_eq!(
            failure_of(&json!({ "code": -32603, "message": "Internal error" })),
            Failure::Trouble("Internal error".into())
        );
    }

    #[test]
    fn the_pages_it_read_are_kept_once() {
        let mut answer = Answer::default();
        let mut pieces = Vec::new();
        let mut piece = |p: &str| pieces.push(p.to_string());
        let fetch =
            json!({ "sessionUpdate": "tool_call", "rawInput": { "url": "https://example.org/a" } });
        heard(&fetch, &mut answer, &mut piece);
        heard(&fetch, &mut answer, &mut piece);
        heard(
            &json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Hi" } }),
            &mut answer,
            &mut piece,
        );
        heard(&fetch, &mut answer, &mut piece);
        heard(
            &json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "There." } }),
            &mut answer,
            &mut piece,
        );
        assert_eq!(answer.read, vec!["https://example.org/a".to_string()]);
        assert_eq!(answer.text, "Hi\n\nThere.");
        assert_eq!(pieces, vec!["Hi", "\n\n", "There."]);
    }
}
