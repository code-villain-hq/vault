//! The protocol, as the app's side of it: what the agent sends, and what
//! the app sends back. Runs nothing itself: it is handed the agent's lines
//! (`process.rs` reads them) and puts what to send on a channel. What the
//! user does to a conversation is in `user.rs`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::{json, Value};

use super::conversation::Conversation;
use super::{Image, Launch, Mcp, OnSession, Pane, Pick, Setting};
use crate::pty::Activity;

/// The protocol version the app speaks.
const PROTOCOL_VERSION: u64 = 1;

/// What a request the app sent is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Waiting {
    Initialize,
    List,
    Open(Opening),
    Prompt,
    Setting,
    /// A setting the launch asked for (`Launch::presets`), named by its
    /// category, which the first prompt waits on.
    Preset(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Opening {
    New,
    Load(String),
    Resume(String),
}

/// A permission question the agent is waiting on.
pub(super) struct Question {
    /// The id of the agent's request, which the answer goes back under.
    pub(super) rpc: Value,
    pub(super) entry: usize,
    pub(super) options: Vec<String>,
}

#[derive(Default)]
pub(super) struct Caps {
    load: bool,
    resume: bool,
    list: bool,
    http: bool,
    /// It takes images in a prompt (ACP-16).
    pub(super) image: bool,
    /// How the agent says to sign in, for a session it will not open.
    sign_in: Vec<String>,
}

pub(super) struct State {
    pub(super) convo: Conversation,
    pub(super) cwd: String,
    pub(super) mcp: Option<Mcp>,
    pub(super) pick: Pick,
    pub(super) caps: Caps,
    pub(super) agent: Option<String>,
    pub(super) session: Option<String>,
    pub(super) waiting: HashMap<u64, Waiting>,
    pub(super) questions: Vec<Question>,
    pub(super) busy: bool,
    pub(super) ready: bool,
    pub(super) exited: bool,
    pub(super) queue: VecDeque<Queued>,
    /// What to set once the conversation is open (`Launch::presets`).
    pub(super) presets: VecDeque<(String, String)>,
    /// What its finished turns used, where it says (RUN-4).
    pub(super) tokens: Option<crate::runs::Tokens>,
}

impl State {
    /// What was waiting will not be sent: shown as not sent, so the text is
    /// still there to copy.
    pub(super) fn drop_queue(&mut self) {
        for q in std::mem::take(&mut self.queue) {
            let entry = q.entry.unwrap_or_else(|| self.convo.user(&q.shown(), false));
            self.convo.dequeue(entry, true);
        }
    }
}

/// A prompt waiting to be sent, with its entry. One given before the
/// conversation opened has none yet: a conversation being picked up replays
/// itself first, and the prompt goes after it, not into it.
pub(super) struct Queued {
    pub(super) text: String,
    pub(super) images: Vec<Image>,
    pub(super) entry: Option<usize>,
}

impl Queued {
    /// What the conversation shows of it: its text, and each image named as
    /// a replay names one (`read::block_text`), so the two read alike.
    pub(super) fn shown(&self) -> String {
        let images = vec!["[image]"; self.images.len()].join(" ");
        match (self.text.trim().is_empty(), images.is_empty()) {
            (_, true) => self.text.clone(),
            (true, false) => images,
            (false, false) => format!("{}\n{images}", self.text),
        }
    }
}

/// What handling something decided, done once the lock is let go.
#[derive(Default)]
pub(super) struct Effects {
    pub(super) send: Vec<Value>,
    pub(super) report: Option<Activity>,
    pub(super) topic: Option<String>,
    pub(super) session: Option<String>,
    /// The settings offered when the conversation opened, for `OnSession`.
    pub(super) offered: Vec<Setting>,
    pub(super) failed: Option<String>,
    pub(super) text: String,
    pub(super) changed: bool,
}

/// One ACP agent's side of the protocol, as the app sees it.
pub struct Conn {
    pane_id: String,
    out: std::sync::mpsc::Sender<String>,
    next: AtomicU64,
    pub(super) state: Mutex<State>,
    pane: OnceLock<Box<dyn Pane>>,
    pub(super) stderr: Mutex<String>,
    on_session: Option<OnSession>,
}

impl Conn {
    /// A connection that writes its lines to `out`. Nothing is sent until
    /// `begin`.
    pub(super) fn new(launch: Launch, cwd: &str, pane_id: &str, out: std::sync::mpsc::Sender<String>) -> Conn {
        let convo = Conversation::default();
        let mut queue = VecDeque::new();
        if let Some(prompt) = launch.prompt.filter(|p| !p.trim().is_empty()) {
            queue.push_back(Queued { text: prompt, images: Vec::new(), entry: None });
        }
        Conn {
            pane_id: pane_id.to_string(),
            out,
            next: AtomicU64::new(1),
            state: Mutex::new(State {
                convo,
                cwd: cwd.to_string(),
                mcp: launch.mcp,
                pick: launch.pick,
                caps: Caps::default(),
                agent: None,
                session: None,
                waiting: HashMap::new(),
                questions: Vec::new(),
                busy: false,
                ready: false,
                exited: false,
                queue,
                presets: launch.presets.into(),
                tokens: None,
            }),
            pane: OnceLock::new(),
            stderr: Mutex::new(String::new()),
            on_session: launch.on_session,
        }
    }

    /// Say hello: `initialize`, and from its answer the rest follows.
    pub(super) fn begin(&self, pane: Box<dyn Pane>) {
        let _ = self.pane.set(pane);
        self.with(|st, fx| {
            let params = json!({
                "protocolVersion": PROTOCOL_VERSION,
                // Agents read, write and run things themselves, as they do
                // in a terminal: the app takes on none of it.
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
                "clientInfo": { "name": "villain-layer", "title": "Villain Layer", "version": env!("CARGO_PKG_VERSION") },
            });
            self.request(st, fx, "initialize", params, Waiting::Initialize);
            fx.report = Some(Activity::Idle);
        });
    }

    /// Run `f` under the lock, then carry out what it decided outside it.
    pub(super) fn with<T>(&self, f: impl FnOnce(&mut State, &mut Effects) -> T) -> T {
        let mut fx = Effects::default();
        let out = {
            let mut st = self.state.lock();
            let rev = st.convo.rev();
            let out = f(&mut st, &mut fx);
            fx.text = st.convo.take_text();
            fx.changed |= st.convo.rev() != rev;
            out
        };
        for msg in fx.send {
            let _ = self.out.send(msg.to_string());
        }
        if let Some(pane) = self.pane.get() {
            if !fx.text.is_empty() {
                pane.print(&fx.text);
            }
            // The pane first, then whoever remembers it: one of the two is
            // sure to see the other (`commands::remember_pane`).
            if let Some(id) = &fx.session {
                pane.session(id);
            }
            if let Some(topic) = fx.topic {
                pane.topic(topic);
            }
            if let Some(message) = &fx.failed {
                pane.failed(message);
            }
            if let Some(activity) = fx.report {
                pane.report(activity);
            }
            if fx.changed {
                pane.changed();
            }
        }
        if let (Some(id), Some(remember)) = (&fx.session, &self.on_session) {
            remember(&self.pane_id, id, &fx.offered);
        }
        out
    }

    pub(super) fn request(&self, st: &mut State, fx: &mut Effects, method: &str, params: Value, waiting: Waiting) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        st.waiting.insert(id, waiting);
        fx.send.push(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }

    /// The app's MCP server, as an agent that takes one over HTTP is given it.
    fn servers(&self, st: &mut State) -> Value {
        match &st.mcp {
            Some(mcp) if st.caps.http => json!([{
                "type": "http",
                "name": mcp.name,
                "url": mcp.url,
                "headers": [
                    { "name": "Authorization", "value": format!("Bearer {}", mcp.token) },
                    { "name": "X-Villain-Pane", "value": self.pane_id },
                ],
            }]),
            Some(_) => {
                st.mcp = None;
                st.convo.note("This agent takes no MCP server over HTTP, so it cannot use the app's tools.", false);
                json!([])
            }
            None => json!([]),
        }
    }

    /// Open the conversation the launch asked for.
    fn open(&self, st: &mut State, fx: &mut Effects) {
        let pick = std::mem::replace(&mut st.pick, Pick::New);
        let opening = match pick {
            Pick::Id(id) if st.caps.load => Opening::Load(id),
            Pick::Id(id) if st.caps.resume => Opening::Resume(id),
            Pick::Newest if st.caps.list => {
                let cwd = st.cwd.clone();
                self.request(st, fx, "session/list", json!({ "cwd": cwd }), Waiting::List);
                return;
            }
            Pick::Id(_) | Pick::Newest => {
                st.convo.note("This agent cannot pick a conversation back up, so this is a new one.", false);
                Opening::New
            }
            Pick::New => Opening::New,
        };
        self.open_as(st, fx, opening);
    }

    fn open_as(&self, st: &mut State, fx: &mut Effects, opening: Opening) {
        let servers = self.servers(st);
        let cwd = st.cwd.clone();
        let (method, params) = match &opening {
            Opening::New => ("session/new", json!({ "cwd": cwd, "mcpServers": servers })),
            Opening::Load(id) => ("session/load", json!({ "sessionId": id, "cwd": cwd, "mcpServers": servers })),
            Opening::Resume(id) => ("session/resume", json!({ "sessionId": id, "cwd": cwd, "mcpServers": servers })),
        };
        self.request(st, fx, method, params, Waiting::Open(opening));
    }

    /// Ask for the next of the launch's settings the agent offers and is
    /// not in already, one at a time. True when the first prompt has to wait
    /// for the answer: a turn begun in the old mode would stop at its first
    /// command to ask, and one begun on the old model would run on it.
    fn set_preset(&self, st: &mut State, fx: &mut Effects) -> bool {
        let Some(session) = st.session.clone() else { return false };
        while let Some((category, value)) = st.presets.pop_front() {
            let offered = st
                .convo
                .settings
                .iter()
                .find(|s| s.category.as_deref() == Some(category.as_str()) && s.options.iter().any(|o| o.value == value));
            let Some(setting) = offered.filter(|s| s.current != value) else { continue };
            let params = json!({ "sessionId": session, "configId": setting.id, "value": value });
            self.request(st, fx, "session/set_config_option", params, Waiting::Preset(category));
            return true;
        }
        false
    }

    pub(super) fn send_prompt(&self, st: &mut State, fx: &mut Effects, q: &Queued) {
        let Some(session) = st.session.clone() else { return };
        let mut blocks: Vec<Value> = q
            .images
            .iter()
            .map(|i| json!({ "type": "image", "mimeType": i.mime_type, "data": i.data }))
            .collect();
        if !q.text.trim().is_empty() {
            blocks.push(json!({ "type": "text", "text": q.text }));
        }
        let params = json!({ "sessionId": session, "prompt": blocks });
        self.request(st, fx, "session/prompt", params, Waiting::Prompt);
        st.busy = true;
        fx.report = Some(Activity::Working);
    }

    /// The next waiting prompt, if a turn can start.
    pub(super) fn next_prompt(&self, st: &mut State, fx: &mut Effects) {
        if st.busy || !st.ready {
            return;
        }
        if let Some(q) = st.queue.pop_front() {
            match q.entry {
                Some(entry) => st.convo.dequeue(entry, false),
                None => {
                    st.convo.user(&q.shown(), false);
                }
            }
            self.send_prompt(st, fx, &q);
        }
    }

    // ---------------------------------------------------------- the agent

    /// One line from the agent.
    pub(crate) fn handle_line(&self, line: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(line) else { return };
        let method = msg.get("method").and_then(Value::as_str);
        match (method, msg.get("id")) {
            (Some(method), Some(id)) => self.on_request(method, id.clone(), msg.get("params")),
            (Some(method), None) => self.on_notification(method, msg.get("params")),
            (None, Some(id)) => {
                if let Some(id) = id.as_u64() {
                    self.on_response(id, msg.get("result"), msg.get("error"));
                }
            }
            _ => {}
        }
    }

    fn on_notification(&self, method: &str, params: Option<&Value>) {
        if method != "session/update" {
            return;
        }
        let Some(update) = params.and_then(|p| p.get("update")) else { return };
        self.with(|st, fx| {
            let applied = st.convo.apply(update);
            fx.topic = applied.title;
            fx.changed = true;
        });
    }

    fn on_request(&self, method: &str, id: Value, params: Option<&Value>) {
        if method != "session/request_permission" {
            // Not offered in `initialize`, so not expected; answered all the
            // same, so the agent is not left waiting on it.
            let reply = json!({ "jsonrpc": "2.0", "id": id, "error": {
                "code": -32601, "message": format!("Villain Layer does not offer {method}") } });
            let _ = self.out.send(reply.to_string());
            return;
        }
        let empty = Value::Null;
        let params = params.unwrap_or(&empty);
        let tool = params.get("toolCall").unwrap_or(&empty);
        let options = params.get("options").unwrap_or(&empty);
        self.with(|st, fx| {
            let entry = st.convo.question(tool, options);
            let options = options
                .as_array()
                .map(|all| all.iter().filter_map(|o| o.get("optionId")?.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            st.questions.push(Question { rpc: id, entry, options });
            fx.report = Some(Activity::Asking);
        });
    }

    fn on_response(&self, id: u64, result: Option<&Value>, error: Option<&Value>) {
        self.with(|st, fx| {
            let Some(waiting) = st.waiting.remove(&id) else { return };
            let failed = error.map(|e| {
                e.get("message").and_then(Value::as_str).unwrap_or("the agent gave an error").to_string()
            });
            let empty = Value::Null;
            let result = result.unwrap_or(&empty);
            match (waiting, failed) {
                (Waiting::Initialize, None) => {
                    let caps = result.get("agentCapabilities").unwrap_or(&empty);
                    let session = caps.get("sessionCapabilities").unwrap_or(&empty);
                    st.caps = Caps {
                        load: caps.get("loadSession").and_then(Value::as_bool).unwrap_or(false),
                        resume: session.get("resume").is_some_and(Value::is_object),
                        list: session.get("list").is_some_and(Value::is_object),
                        http: caps.pointer("/mcpCapabilities/http").and_then(Value::as_bool).unwrap_or(false),
                        image: caps.pointer("/promptCapabilities/image").and_then(Value::as_bool).unwrap_or(false),
                        sign_in: sign_in(result.get("authMethods")),
                    };
                    st.agent = result.get("agentInfo").map(|a| {
                        let name = a.get("title").or_else(|| a.get("name")).and_then(Value::as_str).unwrap_or("agent");
                        match a.get("version").and_then(Value::as_str) {
                            Some(v) => format!("{name} {v}"),
                            None => name.to_string(),
                        }
                    });
                    self.open(st, fx);
                }
                (Waiting::Initialize, Some(e)) => {
                    st.convo.note(&format!("The agent would not start: {e}"), true);
                    self.give_up(st, fx);
                }
                (Waiting::List, failed) => {
                    let cwd = st.cwd.clone();
                    let newest = failed.is_none().then(|| newest_in(result, &cwd)).flatten();
                    match newest {
                        Some(id) => {
                            let opening = if st.caps.load { Opening::Load(id) } else if st.caps.resume { Opening::Resume(id) } else { Opening::New };
                            self.open_as(st, fx, opening);
                        }
                        None => self.open_as(st, fx, Opening::New),
                    }
                }
                (Waiting::Open(opening), None) => {
                    let id = match opening {
                        Opening::New => result.get("sessionId").and_then(Value::as_str).map(str::to_string),
                        Opening::Load(id) | Opening::Resume(id) => Some(id),
                    };
                    let Some(id) = id else {
                        st.convo.note("The agent opened a conversation without naming it.", true);
                        self.give_up(st, fx);
                        return;
                    };
                    if let Some(options) = result.get("configOptions") {
                        st.convo.set_settings(options);
                    }
                    st.session = Some(id.clone());
                    fx.session = Some(id);
                    fx.offered = st.convo.settings.clone();
                    st.ready = true;
                    fx.report = Some(Activity::Idle);
                    if !self.set_preset(st, fx) {
                        self.next_prompt(st, fx);
                    }
                }
                (Waiting::Open(Opening::Load(_) | Opening::Resume(_)), Some(e)) => {
                    st.convo.note(&format!("Could not pick the conversation back up ({e}), so this is a new one."), false);
                    self.open_as(st, fx, Opening::New);
                }
                (Waiting::Open(Opening::New), Some(e)) => {
                    st.convo.note(&format!("The agent would not open a conversation: {e}"), true);
                    if !st.caps.sign_in.is_empty() {
                        let how = st.caps.sign_in.join("\n");
                        st.convo.note(&format!("If it needs you to sign in, this is how it says to:\n{how}"), false);
                    }
                    self.give_up(st, fx);
                }
                (Waiting::Prompt, failed) => {
                    st.busy = false;
                    match failed {
                        Some(e) => {
                            st.convo.note(&e, true);
                            fx.failed = Some(e);
                            fx.report = Some(Activity::Done);
                        }
                        None => {
                            let reason = result.get("stopReason").and_then(Value::as_str).unwrap_or("end_turn");
                            st.convo.turn_ended(reason);
                            if let Some(used) = result.get("usage").and_then(turn_tokens) {
                                st.tokens.get_or_insert_default().add(used);
                            }
                            fx.report = Some(if reason == "cancelled" { Activity::Idle } else { Activity::Done });
                        }
                    }
                    // A question left open by a turn that is over can no
                    // longer be answered.
                    for q in std::mem::take(&mut st.questions) {
                        st.convo.answered(q.entry, "cancelled");
                    }
                    self.next_prompt(st, fx);
                }
                (Waiting::Setting, None) => {
                    if let Some(options) = result.get("configOptions") {
                        st.convo.set_settings(options);
                    }
                }
                (Waiting::Setting, Some(e)) => {
                    st.convo.note(&format!("The agent would not change that: {e}"), true);
                }
                (Waiting::Preset(category), failed) => {
                    match failed {
                        None => {
                            if let Some(options) = result.get("configOptions") {
                                st.convo.set_settings(options);
                            }
                        }
                        Some(e) => {
                            st.convo.note(&format!("The agent would not change its {category}: {e}"), false);
                        }
                    }
                    if !self.set_preset(st, fx) {
                        self.next_prompt(st, fx);
                    }
                }
            }
        });
    }

    /// The conversation will not open: what was waiting to be sent never
    /// will be, and the pane is finished with something to read.
    fn give_up(&self, st: &mut State, fx: &mut Effects) {
        st.drop_queue();
        st.ready = false;
        fx.report = Some(Activity::Done);
    }

    /// The process is gone.
    pub(super) fn exited(&self, code: Option<i32>) {
        let stderr = self.stderr.lock().trim().to_string();
        self.with(|st, _| {
            st.exited = true;
            st.busy = false;
            for q in std::mem::take(&mut st.questions) {
                st.convo.answered(q.entry, "cancelled");
            }
            st.drop_queue();
            // Before the conversation opened, the only word on why is what it
            // printed on its way out.
            if st.session.is_none() && !stderr.is_empty() {
                st.convo.note(&format!("The agent exited before it was ready. It said:\n{stderr}"), true);
            }
        });
        if let Some(pane) = self.pane.get() {
            pane.exited(code);
        }
    }
}

/// What one turn used, from the `usage` on the answer to its prompt, which
/// Claude's adapter counts afresh for each prompt.
fn turn_tokens(usage: &Value) -> Option<crate::runs::Tokens> {
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    let used = crate::runs::Tokens {
        input: n("inputTokens"),
        output: n("outputTokens"),
        cached_read: n("cachedReadTokens"),
        cached_write: n("cachedWriteTokens"),
    };
    (used != crate::runs::Tokens::default()).then_some(used)
}

/// How an agent's `authMethods` say to sign in, one line each.
fn sign_in(methods: Option<&Value>) -> Vec<String> {
    let Some(all) = methods.and_then(Value::as_array) else { return Vec::new() };
    all.iter()
        .filter_map(|m| {
            let name = m.get("name").and_then(Value::as_str)?;
            let terminal = m.pointer("/_meta/terminal-auth").and_then(|t| {
                let cmd = t.get("command")?.as_str()?;
                let args: Vec<&str> = t.get("args")?.as_array()?.iter().filter_map(Value::as_str).collect();
                Some(format!("{cmd} {}", args.join(" ")).trim().to_string())
            });
            let said = m.get("description").and_then(Value::as_str);
            Some(match (terminal, said) {
                (Some(cmd), _) => format!("- {name}: run `{cmd}` in a shell"),
                (None, Some(said)) => format!("- {name}: {said}"),
                (None, None) => format!("- {name}"),
            })
        })
        .collect()
}

/// The newest conversation `session/list` names for `cwd`.
fn newest_in(result: &Value, cwd: &str) -> Option<String> {
    result
        .get("sessions")?
        .as_array()?
        .iter()
        .filter(|s| s.get("cwd").and_then(Value::as_str).is_none_or(|c| c == cwd))
        .max_by_key(|s| s.get("updatedAt").and_then(Value::as_str).unwrap_or_default().to_string())
        .and_then(|s| s.get("sessionId")?.as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    use std::sync::Arc;

    use crate::acp::entry::Body;
    use crate::acp::{Mcp, Pick};

    #[derive(Default)]
    struct Seen {
        reports: Vec<Activity>,
        text: String,
        topic: Option<String>,
        session: Option<String>,
        failed: Option<String>,
    }

    struct FakePane(Arc<Mutex<Seen>>);

    impl Pane for FakePane {
        fn report(&self, a: Activity) { self.0.lock().reports.push(a); }
        fn print(&self, t: &str) { self.0.lock().text.push_str(t); }
        fn changed(&self) {}
        fn topic(&self, t: String) { self.0.lock().topic = Some(t); }
        fn session(&self, id: &str) { self.0.lock().session = Some(id.into()); }
        fn failed(&self, m: &str) { self.0.lock().failed = Some(m.into()); }
        fn exited(&self, _: Option<i32>) {}
    }

    fn launch(pick: Pick, prompt: Option<&str>) -> Launch {
        Launch {
            mcp: Some(Mcp { name: "villain-layer".into(), url: "http://127.0.0.1:1/mcp".into(), token: "tok".into() }),
            pick,
            prompt: prompt.map(str::to_string),
            presets: Vec::new(),
            on_session: None,
        }
    }

    /// A connection with no process: what it sends comes out of the receiver.
    fn conn(pick: Pick, prompt: Option<&str>) -> (Conn, Receiver<String>, Arc<Mutex<Seen>>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let c = Conn::new(launch(pick, prompt), "/work", "pane-1", tx);
        let seen = Arc::new(Mutex::new(Seen::default()));
        c.begin(Box::new(FakePane(seen.clone())));
        (c, rx, seen)
    }

    fn sent(rx: &Receiver<String>) -> Vec<Value> {
        rx.try_iter().map(|l| serde_json::from_str(&l).unwrap()).collect()
    }

    fn reply(c: &Conn, id: &Value, result: Value) {
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string());
    }

    const CAPS: &str = r#"{ "protocolVersion": 1, "agentInfo": { "title": "Fake", "version": "1.0" },
        "agentCapabilities": { "loadSession": true, "mcpCapabilities": { "http": true },
        "sessionCapabilities": { "list": {}, "resume": {} } } }"#;

    /// Through `initialize` and `session/new`, as OpenCode, Copilot and the
    /// Claude adapter answered them in the spike.
    fn opened(prompt: Option<&str>) -> (Conn, Receiver<String>, Arc<Mutex<Seen>>) {
        let (c, rx, seen) = conn(Pick::New, prompt);
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let new = sent(&rx).remove(0);
        assert_eq!(new["method"], "session/new");
        reply(&c, &new["id"], json!({ "sessionId": "s-1" }));
        (c, rx, seen)
    }

    #[test]
    fn the_apps_tools_go_to_the_agent_over_its_stdin_with_the_token_as_a_header() {
        let (c, rx, _) = conn(Pick::New, None);
        let init = sent(&rx).remove(0);
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["clientCapabilities"]["terminal"], false);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let new = sent(&rx).remove(0);
        let server = &new["params"]["mcpServers"][0];
        assert_eq!(server["type"], "http");
        assert_eq!(server["headers"][0]["value"], "Bearer tok");
        assert_eq!(server["headers"][1], json!({ "name": "X-Villain-Pane", "value": "pane-1" }));
        assert_eq!(new["params"]["cwd"], "/work");
    }

    #[test]
    fn an_agent_with_no_http_mcp_is_started_without_the_tools_and_says_so() {
        let (c, rx, seen) = conn(Pick::New, None);
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], json!({ "agentCapabilities": {} }));
        let new = sent(&rx).remove(0);
        assert_eq!(new["params"]["mcpServers"], json!([]));
        assert!(seen.lock().text.contains("cannot use the app's tools"));
    }

    #[test]
    fn the_opening_prompt_waits_for_the_conversation_and_then_it_works_until_done() {
        let (c, rx, seen) = opened(Some("Fix the login bug"));
        let prompt = sent(&rx).remove(0);
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(prompt["params"]["sessionId"], "s-1");
        assert_eq!(prompt["params"]["prompt"][0]["text"], "Fix the login bug");
        assert_eq!(seen.lock().session.as_deref(), Some("s-1"));
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Working));
        reply(&c, &prompt["id"], json!({ "stopReason": "end_turn" }));
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Done));
        assert!(!c.view(None).busy);
    }

    #[test]
    fn a_prompt_longer_than_a_terminal_takes_is_sent_whole() {
        let (c, rx, _) = opened(None);
        let long = "x".repeat(10_000);
        c.prompt(&long).unwrap();
        assert_eq!(sent(&rx)[0]["params"]["prompt"][0]["text"].as_str().map(str::len), Some(10_000));
    }

    #[test]
    fn a_prompt_sent_during_a_turn_waits_for_it_to_end() {
        let (c, rx, _) = opened(None);
        c.prompt("one").unwrap();
        let first = sent(&rx).remove(0);
        c.prompt("two").unwrap();
        assert!(sent(&rx).is_empty());
        assert_eq!(c.view(None).queued, 1);
        reply(&c, &first["id"], json!({ "stopReason": "end_turn" }));
        let second = sent(&rx).remove(0);
        assert_eq!(second["params"]["prompt"][0]["text"], "two");
    }

    fn png() -> Image {
        Image { mime_type: "image/png".into(), data: "iVBORw0KGgo=".into() }
    }

    /// Open, with an agent that says it takes images, as `claude-agent-acp` does.
    fn opened_seeing() -> (Conn, Receiver<String>) {
        let (c, rx, _) = conn(Pick::New, None);
        let init = sent(&rx).remove(0);
        let mut caps: Value = serde_json::from_str(CAPS).unwrap();
        caps["agentCapabilities"]["promptCapabilities"] = json!({ "image": true });
        reply(&c, &init["id"], caps);
        let new = sent(&rx).remove(0);
        reply(&c, &new["id"], json!({ "sessionId": "s-1" }));
        (c, rx)
    }

    #[test]
    fn a_pasted_image_goes_to_the_agent_as_an_image_not_as_its_link() {
        let (c, rx) = opened_seeing();
        assert!(c.view(None).images);
        c.prompt_with("What is wrong here?", vec![png()]).unwrap();
        let prompt = &sent(&rx)[0]["params"]["prompt"];
        assert_eq!(prompt[0], json!({ "type": "image", "mimeType": "image/png", "data": "iVBORw0KGgo=" }));
        assert_eq!(prompt[1], json!({ "type": "text", "text": "What is wrong here?" }));
        let shown = c.view(None).changes.entries.remove(0).body;
        assert_eq!(shown, Body::User { text: "What is wrong here?\n[image]".into(), queued: false, dropped: false });
    }

    #[test]
    fn an_image_alone_is_a_prompt() {
        let (c, rx) = opened_seeing();
        c.prompt_with("  ", vec![png(), png()]).unwrap();
        let prompt = &sent(&rx)[0]["params"]["prompt"];
        assert_eq!(prompt.as_array().map(Vec::len), Some(2), "no empty text block: {prompt}");
        let shown = c.view(None).changes.entries.remove(0).body;
        assert_eq!(shown, Body::User { text: "[image] [image]".into(), queued: false, dropped: false });
    }

    #[test]
    fn an_agent_that_does_not_take_images_refuses_the_prompt_rather_than_dropping_them() {
        let (c, rx, _) = opened(None);
        assert!(!c.view(None).images);
        let refused = c.prompt_with("Look", vec![png()]);
        assert!(refused.is_err_and(|e| e.to_string().contains("does not take images")));
        assert!(sent(&rx).is_empty());
        assert!(c.view(None).changes.entries.is_empty(), "nothing shown as sent");
    }

    #[test]
    fn only_an_image_type_the_models_read_is_sent() {
        let (c, rx) = opened_seeing();
        let tiff = Image { mime_type: "image/tiff".into(), data: "AA==".into() };
        assert!(c.prompt_with("Look", vec![tiff]).is_err());
        assert!(sent(&rx).is_empty());
    }

    /// Claude's adapter counts each prompt's tokens afresh, so a run's are
    /// the sum of its turns'. A turn stopped early says nothing.
    #[test]
    fn the_tokens_of_each_finished_turn_add_up_for_the_run() {
        let (c, rx, _) = opened(None);
        assert_eq!(c.tokens(), None, "an agent that never says has none, not zero");
        let usage = |input: u64| json!({ "inputTokens": input, "outputTokens": 20, "cachedReadTokens": 300, "cachedWriteTokens": 4, "totalTokens": input + 324 });
        c.prompt("one").unwrap();
        reply(&c, &sent(&rx)[0]["id"], json!({ "stopReason": "end_turn", "usage": usage(10) }));
        c.prompt("two").unwrap();
        reply(&c, &sent(&rx)[0]["id"], json!({ "stopReason": "end_turn", "usage": usage(5) }));
        c.prompt("three").unwrap();
        reply(&c, &sent(&rx)[0]["id"], json!({ "stopReason": "cancelled" }));
        assert_eq!(
            c.tokens(),
            Some(crate::runs::Tokens { input: 15, output: 40, cached_read: 600, cached_write: 8 })
        );
    }

    #[test]
    fn a_question_is_asking_until_answered_and_the_answer_goes_back_under_its_id() {
        let (c, rx, seen) = opened(Some("go"));
        let _prompt = sent(&rx);
        c.handle_line(r#"{"jsonrpc":"2.0","id":"q7","method":"session/request_permission","params":{"sessionId":"s-1",
            "toolCall":{"toolCallId":"t1","title":"Write hello.txt","kind":"edit"},
            "options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"reject","name":"Reject","kind":"reject_once"}]}}"#);
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Asking));
        // A phone's "2" picks the second option.
        assert!(c.key("2"));
        let answer = sent(&rx).remove(0);
        assert_eq!(answer, json!({ "jsonrpc": "2.0", "id": "q7", "result": { "outcome": { "outcome": "selected", "optionId": "reject" } } }));
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Working));
        assert!(!c.key("1"), "nothing is open any more");
    }

    #[test]
    fn stopping_cancels_the_turn_and_answers_open_questions_cancelled() {
        let (c, rx, seen) = opened(Some("go"));
        let prompt = sent(&rx).remove(0);
        c.handle_line(r#"{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{"sessionId":"s-1",
            "toolCall":{"toolCallId":"t1"},"options":[{"optionId":"a","name":"Allow","kind":"allow_once"}]}}"#);
        c.prompt("then this").unwrap();
        assert!(c.key("\u{1b}"));
        let out = sent(&rx);
        assert_eq!(out[0]["method"], "session/cancel");
        assert_eq!(out[1]["result"]["outcome"]["outcome"], "cancelled");
        assert_eq!(c.view(None).queued, 0, "what waited is not sent after a stop");
        reply(&c, &prompt["id"], json!({ "stopReason": "cancelled" }));
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Idle));
        assert!(sent(&rx).is_empty());
    }

    #[test]
    fn a_conversation_is_picked_up_by_its_id_and_its_replay_is_kept() {
        let (c, rx, seen) = conn(Pick::Id("s-9".into()), None);
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let load = sent(&rx).remove(0);
        assert_eq!(load["method"], "session/load");
        assert_eq!(load["params"]["sessionId"], "s-9");
        c.handle_line(r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-9","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"earlier"}}}}"#);
        reply(&c, &load["id"], json!({}));
        assert_eq!(seen.lock().session.as_deref(), Some("s-9"));
        assert_eq!(c.view(None).changes.entries.len(), 1);
        // Picked up, not given work: idle, not done.
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Idle));
    }

    /// OpenCode, picked up with an opening prompt: the replayed reply and the
    /// new one ran together as "pongpong", because the prompt went into the
    /// conversation before the replay did, and the replay into it.
    #[test]
    fn a_prompt_given_while_a_conversation_is_picked_up_goes_after_its_replay() {
        let (c, rx, _) = conn(Pick::Id("s-9".into()), Some("and now?"));
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let load = sent(&rx).remove(0);
        let update = |kind: &str, text: &str| {
            c.handle_line(&json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s-9",
                "update": { "sessionUpdate": kind, "content": { "type": "text", "text": text } } } }).to_string());
        };
        update("user_message_chunk", "say pong");
        update("agent_message_chunk", "pong");
        reply(&c, &load["id"], json!({}));
        let prompt = sent(&rx).remove(0);
        assert_eq!(prompt["params"]["prompt"][0]["text"], "and now?");
        update("agent_message_chunk", "pong again");
        let texts: Vec<String> = c.view(None).changes.entries.iter().map(|e| format!("{:?}", e.body)).collect();
        assert_eq!(texts.len(), 4, "{texts:?}");
        assert!(texts[0].contains("say pong") && texts[2].contains("and now?") && texts[3].contains("pong again"), "{texts:?}");
    }

    #[test]
    fn the_newest_conversation_is_the_newest_the_agent_lists_for_the_folder() {
        let (c, rx, _) = conn(Pick::Newest, None);
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let list = sent(&rx).remove(0);
        assert_eq!(list["method"], "session/list");
        reply(&c, &list["id"], json!({ "sessions": [
            { "sessionId": "old", "cwd": "/work", "updatedAt": "2026-10-01T10:00:00Z" },
            { "sessionId": "elsewhere", "cwd": "/other", "updatedAt": "2026-10-08T10:00:00Z" },
            { "sessionId": "new", "cwd": "/work", "updatedAt": "2026-10-07T10:00:00Z" }
        ] }));
        let load = sent(&rx).remove(0);
        assert_eq!(load["params"]["sessionId"], "new");
    }

    #[test]
    fn a_conversation_that_will_not_load_is_started_new_and_says_why() {
        let (c, rx, seen) = conn(Pick::Id("gone".into()), None);
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let load = sent(&rx).remove(0);
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": load["id"], "error": { "code": -32002, "message": "Session not found" } }).to_string());
        assert_eq!(sent(&rx).remove(0)["method"], "session/new");
        assert!(seen.lock().text.contains("Session not found"));
    }

    #[test]
    fn an_agent_that_will_not_open_says_how_it_wants_signing_in() {
        let (c, rx, seen) = conn(Pick::New, Some("go"));
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], json!({ "agentCapabilities": {}, "authMethods": [{ "id": "copilot-login", "name": "Log in with Copilot CLI",
            "_meta": { "terminal-auth": { "command": "copilot", "args": ["login"] } } }] }));
        let new = sent(&rx).remove(0);
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": new["id"], "error": { "code": -32000, "message": "Authentication required" } }).to_string());
        let text = seen.lock().text.clone();
        assert!(text.contains("Authentication required"), "{text}");
        assert!(text.contains("run `copilot login` in a shell"), "{text}");
        assert_eq!(c.view(None).queued, 0);
        assert!(sent(&rx).is_empty());
    }

    #[test]
    fn a_failed_turn_is_done_and_its_message_may_be_a_usage_limit() {
        let (c, rx, seen) = opened(Some("go"));
        let prompt = sent(&rx).remove(0);
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": prompt["id"], "error": { "code": -32000, "message": "Claude usage limit reached" } }).to_string());
        assert_eq!(seen.lock().failed.as_deref(), Some("Claude usage limit reached"));
        assert_eq!(seen.lock().reports.last(), Some(&Activity::Done));
    }

    #[test]
    fn a_request_the_app_does_not_offer_is_refused_not_left_waiting() {
        let (c, rx, _) = opened(None);
        c.handle_line(r#"{"jsonrpc":"2.0","id":11,"method":"fs/read_text_file","params":{"path":"/etc/hosts"}}"#);
        let refused = sent(&rx).remove(0);
        assert_eq!(refused["id"], 11);
        assert_eq!(refused["error"]["code"], -32601);
    }

    #[test]
    fn the_title_the_agent_gives_names_the_pane() {
        let (c, _rx, seen) = opened(None);
        c.handle_line(r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-1","update":{"sessionUpdate":"session_info_update","title":"Fixing the login bug"}}}"#);
        assert_eq!(seen.lock().topic.as_deref(), Some("Fixing the login bug"));
    }

    fn with_mode(mode: &str, options: Value) -> (Conn, Receiver<String>) {
        with_presets(vec![("mode".into(), mode.into())], options)
    }

    fn with_presets(presets: Vec<(String, String)>, options: Value) -> (Conn, Receiver<String>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut l = launch(Pick::New, Some("Check the thread"));
        l.presets = presets;
        let c = Conn::new(l, "/work", "pane-1", tx);
        c.begin(Box::new(FakePane(Default::default())));
        let init = sent(&rx).remove(0);
        reply(&c, &init["id"], serde_json::from_str(CAPS).unwrap());
        let new = sent(&rx).remove(0);
        reply(&c, &new["id"], json!({ "sessionId": "s-1", "configOptions": options }));
        (c, rx)
    }

    fn modes(current: &str) -> Value {
        json!([{ "id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": current,
            "options": [{ "value": "default", "name": "Default" }, { "value": "auto", "name": "Auto" }] }])
    }

    #[test]
    fn a_mode_asked_for_is_set_before_the_opening_prompt_goes() {
        let (c, rx) = with_mode("auto", modes("default"));
        let set = sent(&rx);
        assert_eq!(set.len(), 1, "the prompt waits: {set:?}");
        assert_eq!(set[0]["method"], "session/set_config_option");
        assert_eq!(set[0]["params"]["configId"], "mode");
        assert_eq!(set[0]["params"]["value"], "auto");
        reply(&c, &set[0]["id"], json!({ "configOptions": modes("auto") }));
        let prompt = sent(&rx).remove(0);
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(c.view(None).settings[0].current, "auto");
    }

    #[test]
    fn a_mode_and_a_model_are_both_set_one_after_the_other_before_the_prompt() {
        let model = json!({ "id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "sonnet",
            "options": [{ "value": "sonnet", "name": "Sonnet" }, { "value": "opus", "name": "Opus" }] });
        let mut options = modes("default");
        options.as_array_mut().unwrap().push(model.clone());
        let presets = vec![("mode".into(), "auto".into()), ("model".into(), "opus".into())];
        let (c, rx) = with_presets(presets, options);
        let set = sent(&rx);
        assert_eq!((set.len(), set[0]["params"]["configId"].as_str()), (1, Some("mode")), "{set:?}");
        let mut after = modes("auto");
        after.as_array_mut().unwrap().push(model);
        reply(&c, &set[0]["id"], json!({ "configOptions": after }));
        let set = sent(&rx);
        assert_eq!((set[0]["params"]["configId"].as_str(), set[0]["params"]["value"].as_str()), (Some("model"), Some("opus")));
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": set[0]["id"], "error": { "code": -32603, "message": "no such model" } }).to_string());
        assert_eq!(sent(&rx).remove(0)["method"], "session/prompt", "a model refused still lets the prompt go");
        assert!(c.view(None).changes.entries.iter().any(|e| format!("{e:?}").contains("would not change its model")));
    }

    #[test]
    fn a_mode_the_agent_lacks_or_is_in_already_holds_nothing_up() {
        for (mode, options) in [("auto", modes("auto")), ("yolo", modes("default")), ("auto", json!([]))] {
            let (_c, rx) = with_mode(mode, options);
            assert_eq!(sent(&rx).remove(0)["method"], "session/prompt", "{mode}");
        }
    }

    #[test]
    fn a_mode_refused_is_said_and_the_prompt_still_goes() {
        let (c, rx) = with_mode("auto", modes("default"));
        let set = sent(&rx).remove(0);
        c.handle_line(&json!({ "jsonrpc": "2.0", "id": set["id"], "error": { "code": -32603, "message": "no" } }).to_string());
        assert_eq!(sent(&rx).remove(0)["method"], "session/prompt");
    }
}
