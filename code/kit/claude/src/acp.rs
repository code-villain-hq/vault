//! Claude Code or ChatGPT on this Mac, over the Agent Client Protocol, as
//! Layer's conversations run. For each question the app starts the ACP
//! adapter in an empty folder of its own, with the app's prompt, none of
//! the adapter's tools but the web's (when it's on), none of the user's
//! settings for it, and the app's own tools over MCP (`crate::mcp`).
//!
//! - Claude Code (`claude-agent-acp`) answers as the user is signed in to
//!   it: the app never signs in for it, and needs no key.
//! - ChatGPT answers through Codex (`codex-acp`), from a Codex home of the
//!   app's own, so none of the user's Codex config, plugins or hooks come
//!   along. It's signed in to once, from Settings, in the browser.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::login_env;

/// The app whose tools it answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// The name its tools go by, and its name to the adapter: with `forge`,
    /// Claude sees `mcp__forge__suggest`.
    pub key: &'static str,
    /// Its name in words: `Forge`.
    pub name: &'static str,
    pub version: &'static str,
}

/// A picture that goes with the prompt.
#[derive(Debug, Clone)]
pub struct Image {
    pub media_type: String,
    /// Base64.
    pub data: String,
}

/// Claude Code's own tools the app lets it use when the web is on.
pub const WEB_TOOLS: &[&str] = &["WebSearch", "WebFetch"];
/// Codex's features the app turns off: its shell, its file edits, and
/// whatever else would reach past the app's tools.
const CODEX_OFF: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "apply_patch_freeform",
    "view_image",
    "js_repl",
    "multi_agent",
    "image_generation",
    "apps",
];

/// Which app answers on this Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    ClaudeCode,
    Chatgpt,
}

impl Kind {
    /// What Settings stores and shows: `claude_code` or `chatgpt`.
    pub fn key(self) -> &'static str {
        match self {
            Kind::ClaudeCode => "claude_code",
            Kind::Chatgpt => "chatgpt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::ClaudeCode => "Claude Code",
            Kind::Chatgpt => "ChatGPT",
        }
    }

    /// The ACP adapter the app runs, looked for on the login shell's PATH.
    pub fn adapter(self) -> &'static str {
        match self {
            Kind::ClaudeCode => "claude-agent-acp",
            Kind::Chatgpt => "codex-acp",
        }
    }

    /// The command that installs it, as Settings → Claude shows it.
    pub fn install(self) -> &'static str {
        match self {
            Kind::ClaudeCode => "npm install -g @agentclientprotocol/claude-agent-acp",
            Kind::Chatgpt => "npm install -g @agentclientprotocol/codex-acp",
        }
    }
}

const PROTOCOL_VERSION: i64 = 1;
/// Starting the adapter and opening a session, which takes a few seconds.
/// A start that takes longer has hung, and is tried once more.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// One answer, searching included, unless the question says.
pub(crate) const ANSWER_TIMEOUT: Duration = Duration::from_secs(600);
/// Signing ChatGPT in, in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// How much of the end of its stderr is kept, to say why it stopped.
const STDERR_TAIL: usize = 4096;
/// How long a check that found it ready holds. Settings asks every few
/// seconds while it's open, and each check starts the adapter.
const READY_FOR: Duration = Duration::from_secs(600);

/// Why it gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The ACP adapter isn't on the login shell's PATH.
    NotInstalled,
    /// The app isn't signed in on this Mac.
    SignedOut,
    /// Anything else, in its words.
    Trouble(String),
}

/// One question for it.
pub struct Question<'a> {
    /// The app's prompt: how to answer, and what about.
    pub system: &'a str,
    /// What they ask.
    pub prompt: &'a str,
    /// Pictures, after the prompt, numbered.
    pub images: &'a [Image],
    /// Whether it may search the web and read pages.
    pub web: bool,
    /// The app's tools it may use, by their names in [`App::key`]. With
    /// none, it isn't given the app's MCP endpoint at all.
    pub tools: &'a [&'a str],
    /// The bearer the app's MCP endpoint knows this question by.
    pub token: &'a str,
    /// How long the answer may take; ten minutes when not said.
    pub timeout: Option<Duration>,
    /// How many turns Claude Code may take; as many as it likes when not
    /// said. It stops with `max_turn_requests` at the limit.
    pub max_turns: Option<u32>,
}

/// What it answered.
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

impl Answer {
    /// An answer the asker's Mac told the server ([`crate::handed`]).
    pub fn said(text: String, read: Vec<String>, stop: String) -> Self {
        Answer {
            text,
            read,
            stop,
            paragraph: false,
        }
    }
}

/// When a check found it ready, and the adapter's version.
type Ready = Option<(Instant, Option<String>)>;

/// Claude Code or ChatGPT on this Mac, as the app starts it.
#[derive(Clone)]
pub struct Adapter {
    app: App,
    kind: Kind,
    /// Its folder: Claude Code runs in it, empty, so it has no project to
    /// look at; ChatGPT runs in `empty` inside it, beside its Codex home.
    folder: PathBuf,
    /// The app's MCP endpoint for the question's tools.
    mcp_url: String,
    /// The adapter to run; looked for on the login PATH when not set.
    program: Option<PathBuf>,
    /// How long a start may take before it's tried again.
    start_timeout: Duration,
    /// The last check that found it ready.
    ready: Arc<Mutex<Ready>>,
}

impl Adapter {
    pub fn new(
        app: App,
        kind: Kind,
        folder: impl Into<PathBuf>,
        mcp_url: impl Into<String>,
    ) -> Self {
        Adapter {
            app,
            kind,
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

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Where it runs: no project, no files.
    fn cwd(&self) -> PathBuf {
        match self.kind {
            Kind::ClaudeCode => self.folder.clone(),
            Kind::Chatgpt => self.folder.join("empty"),
        }
    }

    /// Whether it can answer, and the adapter's version: it starts the
    /// adapter and opens a session, which is where a signed-out app says so.
    /// Nothing is asked, so nothing is spent. A check that
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
        let question = Question {
            system: "",
            prompt: "",
            images: &[],
            web: false,
            tools: &[],
            token: "",
            timeout: None,
            max_turns: None,
        };
        let mut agent = self.start(&question).await?;
        let checked = tokio::time::timeout(self.start_timeout, async {
            let version = agent.initialize().await?;
            agent
                .new_session(&self.cwd(), &self.mcp_url, &question, false)
                .await?;
            Ok(version)
        })
        .await
        .unwrap_or_else(|_| Err(self.trouble("took too long to start.")));
        agent.stop().await;
        checked
    }

    /// Signs ChatGPT in for the app: the browser opens on OpenAI's sign-in,
    /// and this waits until it's done there, or for five minutes.
    pub async fn sign_in(&self) -> Result<(), Failure> {
        let question = Question {
            system: "",
            prompt: "",
            images: &[],
            web: false,
            tools: &[],
            token: "",
            timeout: None,
            max_turns: None,
        };
        let mut agent = self.start(&question).await?;
        let signed_in = tokio::time::timeout(SIGN_IN_TIMEOUT, async {
            agent.initialize().await?;
            agent
                .call(
                    "authenticate",
                    json!({ "methodId": "chat-gpt" }),
                    &mut |_| {},
                )
                .await
        })
        .await
        .unwrap_or(Err(Failure::SignedOut));
        agent.stop().await;
        *self.ready.lock().unwrap() = None;
        signed_in.map(|_| ())
    }

    fn trouble(&self, what: &str) -> Failure {
        Failure::Trouble(format!("{} {what}", self.kind.name()))
    }

    /// Asks it one question, handing each piece of the answer to
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
                question.timeout.unwrap_or(ANSWER_TIMEOUT),
                agent.call("session/prompt", params, &mut |update| {
                    heard(update, &mut answer, &mut piece);
                }),
            )
            .await
            .unwrap_or_else(|_| Err(self.trouble("took too long to answer.")))?;
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
            let mut agent = self.start(question).await?;
            let opened = tokio::time::timeout(self.start_timeout, async {
                agent.initialize().await?;
                // A question with no tools gets no endpoint to reach either.
                let with_tools = !question.tools.is_empty();
                agent
                    .new_session(&self.cwd(), &self.mcp_url, question, with_tools)
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
                        kind = self.kind.name(),
                        "the adapter didn't open a session in time"
                    );
                    agent.stop().await;
                }
            }
        }
        Err(self.trouble("took too long to start."))
    }

    async fn start(&self, question: &Question<'_>) -> Result<Agent, Failure> {
        let program = match &self.program {
            Some(program) => program.clone(),
            None => find_adapter(self.kind.adapter())
                .await
                .ok_or(Failure::NotInstalled)?,
        };
        let mut env = tokio::task::spawn_blocking(login_env::user_env)
            .await
            .map_err(|e| Failure::Trouble(e.to_string()))?
            .clone();
        let cwd = self.cwd();
        std::fs::create_dir_all(&cwd).map_err(|e| Failure::Trouble(e.to_string()))?;
        let mut instructions = None;
        if self.kind == Kind::Chatgpt {
            let home = self.folder.join("codex");
            std::fs::create_dir_all(&home).map_err(|e| Failure::Trouble(e.to_string()))?;
            // The app's prompt in place of Codex's own, which is a coding
            // agent's; one file per question, gone when it's answered.
            let mut config = codex_config(question.web);
            if !question.system.is_empty() {
                let file = home.join(format!("{}.md", uuid::Uuid::now_v7()));
                std::fs::write(&file, question.system)
                    .map_err(|e| Failure::Trouble(e.to_string()))?;
                config["model_instructions_file"] = json!(file);
                instructions = Some(file);
            }
            env.insert("CODEX_HOME".into(), home.display().to_string());
            env.insert("CODEX_CONFIG".into(), config.to_string());
        }
        let name = self.kind.name();
        let mut child = Command::new(&program)
            .current_dir(&cwd)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own group, so stopping it reaches the app it starts.
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Failure::NotInstalled,
                _ => Failure::Trouble(format!("{name} didn't start: {e}")),
            })?;
        let (Some(stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(self.trouble("didn't start."));
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
            app: self.app,
            kind: self.kind,
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 0,
            tail,
            instructions,
        })
    }
}

/// What Codex runs with: its shell, files and extras off, nothing to ask
/// about, and the web as Settings says.
fn codex_config(web: bool) -> Value {
    let features: serde_json::Map<String, Value> = CODEX_OFF
        .iter()
        .map(|feature| (feature.to_string(), json!(false)))
        .collect();
    json!({
        "sandbox_mode": "read-only",
        "approval_policy": "never",
        "web_search": if web { "live" } else { "disabled" },
        "features": features,
    })
}

/// The adapter on the login shell's PATH.
async fn find_adapter(adapter: &str) -> Option<PathBuf> {
    let env = tokio::task::spawn_blocking(login_env::user_env)
        .await
        .ok()?;
    let path = env.get("PATH")?;
    std::env::split_paths(path)
        .map(|dir| dir.join(adapter))
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
    app: App,
    kind: Kind,
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: i64,
    tail: Arc<Mutex<String>>,
    /// ChatGPT's prompt for this question, removed when it stops.
    instructions: Option<PathBuf>,
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
            "clientInfo": { "name": self.app.key, "title": self.app.name, "version": self.app.version },
        });
        let result = self.call("initialize", params, &mut |_| {}).await?;
        Ok(result["agentInfo"]["version"].as_str().map(str::to_string))
    }

    /// Opens a session for `question` and returns its id. `with_tools`
    /// leaves the app's MCP endpoint out when only checking.
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
        let app = self.app.key;
        allowed.extend(question.tools.iter().map(|t| format!("mcp__{app}__{t}")));
        let servers = if with_tools {
            json!([{
                "type": "http",
                "name": app,
                "url": mcp_url,
                "headers": [{ "name": "Authorization", "value": format!("Bearer {}", question.token) }],
            }])
        } else {
            json!([])
        };
        let mut options = json!({
            "tools": tools,
            "allowedTools": allowed,
            "settingSources": [],
        });
        if let Some(turns) = question.max_turns {
            options["maxTurns"] = json!(turns);
        }
        // ChatGPT's prompt and tools came with its start (`codex_config`).
        let params = match self.kind {
            Kind::ClaudeCode => json!({
                "cwd": folder,
                "mcpServers": servers,
                "_meta": {
                    "systemPrompt": question.system,
                    "claudeCode": { "options": options },
                },
            }),
            Kind::Chatgpt => json!({ "cwd": folder, "mcpServers": servers }),
        };
        let result = self.call("session/new", params, &mut |_| {}).await?;
        result["sessionId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Failure::Trouble(format!("{} opened no session.", self.kind.name())))
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
                    let reply = reply_to(self.app, asked, &message["params"]);
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
        let name = self.kind.name();
        if last.is_empty() {
            Failure::Trouble(format!("{name} stopped before it answered."))
        } else {
            Failure::Trouble(format!(
                "{name} stopped: {}",
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
        if let Some(file) = &self.instructions {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// The app's answer to something the adapter asks. Only the app's MCP
/// tools and the web's may run; anything else (shell, files, …) is cancelled.
fn reply_to(app: App, method: &str, params: &Value) -> Value {
    match method {
        "session/request_permission" => {
            if !permission_is_for_the_app(app, params) {
                return json!({ "result": { "outcome": { "outcome": "cancelled" } } });
            }
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
        _ => {
            json!({ "error": { "code": -32601, "message": format!("{} doesn't offer that.", app.name) } })
        }
    }
}

/// Whether the permission is for a tool the app means to allow: one of its
/// MCP tools (`mcp__forge__…`) or a web tool when those are on. Shell,
/// files and anything unnamed are refused.
fn permission_is_for_the_app(app: App, params: &Value) -> bool {
    let tool = &params["toolCall"];
    let kind = tool["kind"].as_str().unwrap_or("");
    if matches!(kind, "execute" | "edit" | "delete" | "move") {
        return false;
    }
    let names = [
        tool["title"].as_str(),
        tool["name"].as_str(),
        tool["rawInput"]["name"].as_str(),
        tool["rawInput"]["toolName"].as_str(),
    ];
    let mcp = format!("mcp__{}__", app.key);
    if names
        .into_iter()
        .flatten()
        .any(|name| WEB_TOOLS.iter().any(|w| name == *w || name.contains(w)) || name.contains(&mcp))
    {
        return true;
    }
    kind == "fetch"
}

/// What a JSON-RPC error from the adapter means for the question.
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
        return Failure::Trouble("It couldn't answer.".into());
    }
    Failure::Trouble(message.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALMANAC: App = App {
        key: "almanac",
        name: "Almanac",
        version: "0.0.0",
    };

    #[test]
    fn a_permission_is_given_once() {
        let params = json!({
            "toolCall": { "title": "mcp__almanac__add_items" },
            "options": [
                { "optionId": "always", "kind": "allow_always" },
                { "optionId": "once", "kind": "allow_once" },
                { "optionId": "no", "kind": "reject_once" },
            ]
        });
        assert_eq!(
            reply_to(ALMANAC, "session/request_permission", &params)["result"]["outcome"]["optionId"],
            "once"
        );
        assert!(!reply_to(ALMANAC, "fs/read_text_file", &json!({}))["error"].is_null());
    }

    #[test]
    fn a_shell_permission_is_refused() {
        let params = json!({
            "toolCall": { "title": "Bash", "kind": "execute" },
            "options": [
                { "optionId": "once", "kind": "allow_once" },
                { "optionId": "no", "kind": "reject_once" },
            ]
        });
        assert_eq!(
            reply_to(ALMANAC, "session/request_permission", &params)["result"]["outcome"]["outcome"],
            "cancelled"
        );
    }

    #[test]
    fn another_apps_tools_are_refused() {
        let params = json!({
            "toolCall": { "title": "mcp__forge__suggest" },
            "options": [{ "optionId": "once", "kind": "allow_once" }]
        });
        assert_eq!(
            reply_to(ALMANAC, "session/request_permission", &params)["result"]["outcome"]["outcome"],
            "cancelled"
        );
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
