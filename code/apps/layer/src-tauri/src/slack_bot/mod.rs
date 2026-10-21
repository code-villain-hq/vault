//! The Slack bot (§24): Slack threads handed to chats, and the chats'
//! replies posted back.
//!
//! `socket.rs` keeps the Socket Mode connection open while the bot is on.
//! Each event goes to `handle.rs`, which asks `event.rs` what it means and
//! opens a chat or passes a message to one. `relay.rs` follows each chat
//! the bot has a thread for, and posts its turns to the thread, until
//! `idle.rs` stops a chat left alone (BOT-14). Everything
//! here is HTTP and waiting, on the async runtime; opening a chat goes
//! through `commands::blocking`, as any command's does.

mod card;
mod event;
mod handle;
mod idle;
mod refuse;
mod relay;
mod socket;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::SlackThread;
use crate::integrations::slack_bot::BotApi;
use crate::pty::PaneInfo;

/// Messages remembered as acted on (BOT-8). Slack retries an event for a
/// few minutes at most, and a mention arrives twice within a second.
const SEEN_CAP: usize = 500;

/// The mode a chat from Slack starts in (BOT-12): Claude Code's name for
/// working without asking, short of what it judges dangerous.
const AUTO: &str = "auto";

/// What a chat opening in `room` is set to before its first prompt, as
/// (category, value): when it is a Slack thread's, `auto` mode while the
/// owner left that on (BOT-12), and the model chosen for the bot (BOT-17).
/// Asked of every ACP chat that opens, so one put back at launch or
/// restarted starts the same way.
pub fn presets_for(config: &crate::config::AppConfig, room: &str) -> Vec<(String, String)> {
    let Some(bot) = config.slack_bot.as_ref() else { return Vec::new() };
    if !config.slack_threads.iter().any(|t| t.room == room) {
        return Vec::new();
    }
    let mut presets = Vec::new();
    if bot.auto {
        presets.push(("mode".to_string(), AUTO.to_string()));
    }
    if !bot.model.is_empty() {
        presets.push(("model".to_string(), bot.model.clone()));
    }
    presets
}

/// Where the connection is, for Settings (BOT-10).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Status {
    /// "off", "connecting", "connected" or "failing".
    pub state: &'static str,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Bot {
    app: OnceLock<AppHandle>,
    /// Moved on each change to the bot's settings: the connection open is
    /// closed and opened again under them.
    generation: tokio::sync::watch::Sender<u64>,
    status: Mutex<Status>,
    /// The Web API under the bot token, while the bot is configured.
    api: Mutex<Option<BotApi>>,
    seen: Mutex<VecDeque<(String, String)>>,
    /// Member names already looked up, by id.
    names: Mutex<HashMap<String, String>>,
    /// Which of them are bots or apps: the bot's rivals (BOT-16).
    bots: Mutex<HashSet<String>>,
    /// Panes a relay follows.
    following: Mutex<HashSet<String>>,
    /// One event at a time: two quick mentions in one thread otherwise both
    /// found it without a chat, and opened two.
    serial: tokio::sync::Mutex<()>,
    /// Who has been told no lately (BOT-2).
    refusals: refuse::Refusals,
}

impl Bot {
    /// Start the connection, for as long as the app runs.
    pub fn spawn(&self, app: AppHandle) {
        let _ = self.app.set(app.clone());
        tauri::async_runtime::spawn(socket::run(app));
    }

    /// The bot's settings changed: connect again under them.
    pub fn restart(&self) {
        self.generation.send_modify(|g| *g += 1);
    }

    pub fn status(&self) -> Status {
        let s = self.status.lock().clone();
        if s.state.is_empty() { Status { state: "off", error: None } } else { s }
    }

    fn set_status(&self, state: &'static str, error: Option<String>) {
        let next = Status { state, error };
        let changed = {
            let mut s = self.status.lock();
            let changed = *s != next;
            *s = next;
            changed
        };
        if changed {
            self.changed();
        }
    }

    /// Settings and the chat view follow the bot (`slackbot:changed`).
    fn changed(&self) {
        if let Some(app) = self.app.get() {
            let _ = app.emit("slackbot:changed", ());
        }
    }

    fn api(&self) -> Option<BotApi> {
        self.api.lock().clone()
    }

    /// True the first time a message is seen (BOT-8).
    fn first_time(&self, channel: &str, ts: &str) -> bool {
        let mut seen = self.seen.lock();
        if seen.iter().any(|(c, t)| c == channel && t == ts) {
            return false;
        }
        seen.push_back((channel.to_string(), ts.to_string()));
        if seen.len() > SEEN_CAP {
            seen.pop_front();
        }
        true
    }

    /// A pane was opened: when it is a chat with a Slack thread, follow it
    /// (BOT-4). Called for every pane the app remembers, so a chat put back
    /// at launch, restarted or reopened is followed with no Slack event.
    pub fn adopt(&self, pane: &PaneInfo, threads: &[SlackThread]) {
        let Some(app) = self.app.get() else { return };
        if !pane.acp {
            return;
        }
        let Some(thread) = threads.iter().find(|t| t.room == pane.cwd) else { return };
        if !self.following.lock().insert(pane.id.clone()) {
            return;
        }
        tauri::async_runtime::spawn(relay::follow(app.clone(), pane.id.clone(), thread.clone()));
    }

    fn unfollow(&self, pane: &str) {
        self.following.lock().remove(pane);
    }

    /// Post to a thread, in order. A failure is told in the message center:
    /// the thread is somewhere nobody in the app is looking.
    async fn post(&self, thread: &SlackThread, texts: Vec<String>) {
        let Some(api) = self.api() else { return };
        for text in texts {
            if let Err(e) = api.reply(&thread.channel, &thread.ts, &text).await {
                self.tell_error(format!("Could not post to the Slack thread in {}", thread.label), e.to_string());
                return;
            }
        }
    }

    /// Post a turn's permission card (BOT-7), or change the one posted as
    /// `ts`. Returns the card's timestamp, None when it could not be posted:
    /// the next change then tries a new one.
    async fn card(&self, thread: &SlackThread, ts: Option<String>, asked: &[card::Asked]) -> Option<String> {
        let (shown, blocks) = card::render(&thread.room, asked);
        self.keep(thread, ts, &shown, &blocks, "a permission question").await
    }

    /// Post a turn's progress message (BOT-6), or change the one posted as
    /// `ts`, as `card` does.
    async fn progress(&self, thread: &SlackThread, ts: Option<String>, markdown: &str) -> Option<String> {
        let blocks = serde_json::json!([{ "type": "markdown", "text": markdown }]);
        let shown = markdown.lines().find(|l| l.starts_with('>')).unwrap_or("Working on it…").trim_start_matches("> ");
        self.keep(thread, ts, shown, &blocks, "the agent's progress").await
    }

    /// One message in a thread that is changed rather than posted again.
    async fn keep(&self, thread: &SlackThread, ts: Option<String>, shown: &str, blocks: &serde_json::Value, what: &str) -> Option<String> {
        let api = self.api()?;
        let done = match &ts {
            Some(ts) => api.update_blocks(&thread.channel, ts, shown, blocks).await.map(|()| ts.clone()),
            None => api.post_blocks(&thread.channel, &thread.ts, shown, blocks).await,
        };
        match done {
            Ok(ts) => Some(ts),
            Err(e) => {
                self.tell_error(format!("Could not show {what} in the Slack thread in {}", thread.label), e.to_string());
                ts
            }
        }
    }

    fn tell_error(&self, title: String, body: String) {
        let Some(app) = self.app.get() else { return };
        crate::messages::record_all(
            app,
            vec![crate::messages::New {
                kind: crate::messages::Kind::Error,
                level: crate::messages::Level::Error,
                title,
                body,
                target: None,
            }],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, SlackBotConfig};

    fn config(auto: bool) -> AppConfig {
        config_with(auto, "")
    }

    fn config_with(auto: bool, model: &str) -> AppConfig {
        let bot = SlackBotConfig {
            owner: "UME".into(),
            agent_id: "claude".into(),
            bot_user: "UBOT".into(),
            bot_name: "drdoom".into(),
            enabled: true,
            auto,
            personality: String::new(),
            model: model.into(),
        };
        let thread = SlackThread { channel: "C1".into(), ts: "1.1".into(), room: "/chat/a".into(), label: "#ops".into(), link: String::new(), seen: String::new() };
        AppConfig { slack_bot: Some(bot), slack_threads: vec![thread], ..Default::default() }
    }

    fn preset(category: &str, value: &str) -> (String, String) {
        (category.to_string(), value.to_string())
    }

    #[test]
    fn only_a_chat_from_slack_starts_in_auto_mode_and_only_while_that_is_on() {
        assert_eq!(presets_for(&config(true), "/chat/a"), [preset("mode", "auto")]);
        assert!(presets_for(&config(true), "/chat/b").is_empty(), "a chat of the owner's own");
        assert!(presets_for(&config(false), "/chat/a").is_empty());
        assert!(presets_for(&AppConfig::default(), "/chat/a").is_empty());
    }

    #[test]
    fn a_chat_from_slack_runs_on_the_model_chosen_for_the_bot() {
        let chosen = config_with(true, "opus");
        assert_eq!(presets_for(&chosen, "/chat/a"), [preset("mode", "auto"), preset("model", "opus")]);
        assert_eq!(presets_for(&config_with(false, "opus"), "/chat/a"), [preset("model", "opus")]);
        assert!(presets_for(&chosen, "/chat/b").is_empty(), "the owner's own chats keep theirs");
    }

    #[test]
    fn a_bot_saved_before_auto_mode_existed_starts_with_it_on() {
        let old = r#"{ "owner": "UME", "agent_id": "claude", "bot_user": "UBOT", "bot_name": "drdoom", "enabled": true }"#;
        let bot: SlackBotConfig = serde_json::from_str(old).unwrap();
        assert!(bot.auto);
    }
}
