//! What the bot does with a message it was sent: open a chat for a thread
//! (BOT-3), or pass the message to the chat the thread already has (BOT-5).

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::card::Click;
use super::event::{self, Action, Message, Opening};
use super::Bot;
use crate::commands::{AppState, Resume, CHAT_TASK_ID};
use crate::config::{SavedPane, SlackBotConfig, SlackThread};
use crate::error::{Error, Result};
use crate::integrations::slack_bot::{from_slack, BotApi, ThreadMessage, THREAD_CAP};
use crate::messages::{self, Kind, Level, New};
use crate::pty::PaneInfo;
use crate::target::Target;

/// Members looked up by name for one thread, at most: a long thread in a
/// big channel would otherwise be a call for each of its authors.
const NAMES_MAX: usize = 30;
/// Threads kept in `config.json`. The oldest go first; a reply in one that
/// went starts nothing, as in any thread without the bot.
const THREADS_KEPT: usize = 200;

pub async fn event(app: AppHandle, event: Value) {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    let _one_at_a_time = bot.serial.lock().await;
    let Some(cfg) = state.config.with(|c| c.slack_bot.clone()) else { return };
    let Some(msg) = event::read(&event, &cfg.bot_user) else { return };
    let handed = thread_of(&state, &msg).is_some();
    let action = event::decide(&msg, &cfg.owner, &cfg.bot_user, handed);
    if action == Action::Ignore || !bot.first_time(&msg.channel, &msg.ts) {
        return;
    }
    if action == Action::Refuse {
        tauri::async_runtime::spawn(super::refuse::refuse(app.clone(), msg));
        return;
    }
    let Some(api) = bot.api() else { return };
    let done = match action {
        Action::Start => start(&app, &api, &msg, &cfg).await,
        Action::Pass => pass(&app, &api, &msg).await,
        Action::Refuse | Action::Ignore => Ok(()),
    };
    if let Err(e) = done {
        let text = format!(":warning: I could not take that: {e}");
        if api.reply(&msg.channel, &msg.thread, &text).await.is_err() {
            bot.tell_error("The Slack bot could not take a message".into(), e.to_string());
        }
    }
}

/// A button on a permission card was clicked (BOT-7): the owner's click
/// answers the question, as the chat in the app would.
pub async fn click(app: AppHandle, payload: Value) {
    if payload.get("type").and_then(Value::as_str) != Some("block_actions") {
        return;
    }
    let state = app.state::<AppState>();
    let Some(cfg) = state.config.with(|c| c.slack_bot.clone()) else { return };
    let Some(api) = state.slack_bot.api() else { return };
    let s = |path: &str| payload.pointer(path).and_then(Value::as_str).unwrap_or_default().to_string();
    let (user, channel) = (s("/user/id"), s("/channel/id"));
    let thread = Some(s("/container/thread_ts")).filter(|t| !t.is_empty()).unwrap_or_else(|| s("/message/thread_ts"));
    let Some(click) = Click::read(&s("/actions/0/value")) else { return };
    let tell = |text: String| {
        let (api, channel, user, thread) = (api.clone(), channel.clone(), user.clone(), thread.clone());
        async move {
            let _ = api.ephemeral(&channel, &user, &thread, &text).await;
        }
    };
    if user != cfg.owner {
        tell(format!("Only <@{}> can answer this.", cfg.owner)).await;
        return;
    }
    // A card the bot posted names a chat it handed a thread to; nothing else
    // is answered from Slack.
    if !state.config.with(|c| c.slack_threads.iter().any(|t| t.room == click.room)) {
        return;
    }
    let Some(pane) = live_chat(&state, &click.room) else {
        tell("That chat is closed, and its question went with it.".into()).await;
        return;
    };
    if let Err(e) = state.ptys.acp_answer(&pane.id, click.entry, &click.option) {
        tell(format!("Could not answer: {e}")).await;
    }
}

/// Slack has no message where an event said one was (BOT-8). A warning in
/// Slack would have no thread to go in, and landed at the top of the
/// channel, so it is told in the app, with what to look for.
fn not_found(bot: &Bot, m: &Message) {
    bot.tell_error("The Slack bot was sent a message Slack cannot find".into(), m.summary());
}

fn thread_of(state: &AppState, m: &Message) -> Option<SlackThread> {
    state.config.with(|c| c.slack_threads.iter().find(|t| t.channel == m.channel && t.ts == m.thread).cloned())
}

/// Open a chat for the thread, given the thread and the request (BOT-3).
async fn start(app: &AppHandle, api: &BotApi, m: &Message, cfg: &SlackBotConfig) -> Result<()> {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    let _ = api.react(&m.channel, &m.ts, "eyes").await;
    let Some((thread, more)) = api.replies(&m.channel, &m.thread, "", THREAD_CAP).await? else {
        not_found(bot, m);
        return Ok(());
    };
    let seen = thread.iter().fold(m.ts.as_str(), |seen, t| event::newest(seen, &t.ts)).to_string();
    let mentioned = event::mentioned(&m.text, &cfg.bot_user);
    let authors = thread.iter().map(|t| t.user.as_str()).chain([m.user.as_str()]);
    learn_names(bot, api, authors.chain(mentioned.iter().map(String::as_str))).await;
    let place = match api.channel_label(&m.channel).await {
        Ok(Some(label)) => label,
        // A direct message, or a channel the bot may not look up.
        Ok(None) | Err(_) => "a direct message".to_string(),
    };
    let link = api.permalink(&m.channel, &m.thread).await.unwrap_or_default();

    let name = |id: &str| bot.names.lock().get(id).cloned();
    let owner = name(&m.user).unwrap_or_else(|| "The owner".into());
    let request = event::request(&m.text, &cfg.bot_user, |id| as_named(bot, id));
    let opening = Opening {
        owner: &owner,
        request: &request,
        place: &place,
        link: &link,
        thread: &thread,
        more,
        personality: &cfg.personality,
    };
    let prompt = opening.prompt(|t| speaker(bot, t), |text| from_slack(text, name));

    let handle = app.clone();
    let agent = cfg.agent_id.clone();
    let (channel, ts, label) = (m.channel.clone(), m.thread.clone(), place.clone());
    // The thread is recorded before its chat opens: opening it asks whether
    // it is one from Slack, for the mode it starts in (BOT-12).
    let (pane, record) = crate::commands::blocking(app.clone(), move |state| {
        let room = crate::commands::chat_room(state, &uuid::Uuid::new_v4().to_string()[..8])?;
        let record = SlackThread { channel, ts, room: room.to_string_lossy().to_string(), label, link, seen };
        state.config.update(|c| {
            c.slack_threads.retain(|t| t.room != record.room);
            c.slack_threads.push(record.clone());
            let len = c.slack_threads.len();
            if len > THREADS_KEPT {
                c.slack_threads.drain(..len - THREADS_KEPT);
            }
        })?;
        match crate::commands::open_chat(&handle, state, agent, Some(prompt), Some(room), Resume::No, true) {
            Ok(pane) => Ok((pane, record)),
            Err(e) => {
                let _ = state.config.update(|c| c.slack_threads.retain(|t| t.room != record.room));
                Err(e)
            }
        }
    })
    .await?;
    // Opening the chat looked for its thread before there was one.
    bot.adopt(&pane, std::slice::from_ref(&record));
    shown(app, &pane);
    messages::record_all(
        app,
        vec![New {
            kind: Kind::Agent,
            level: Level::Info,
            title: format!("From Slack, in {place}"),
            body: request,
            target: Some(Target::Pane { task: CHAT_TASK_ID, pane: &pane.id }.to_string()),
        }],
    );
    bot.changed();
    Ok(())
}

/// Pass a message to the thread's chat (BOT-5), with what the thread said
/// since the chat last heard from it, opening the chat again in its room
/// when it was closed.
async fn pass(app: &AppHandle, api: &BotApi, m: &Message) -> Result<()> {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    let thread = thread_of(&state, m).ok_or_else(|| Error::Other("that thread has no chat any more".into()))?;
    let agent_id = state.config.with(|c| c.slack_bot.as_ref().map(|b| b.agent_id.clone())).unwrap_or_default();
    let _ = api.react(&m.channel, &m.ts, "eyes").await;
    let bot_user = state.config.with(|c| c.slack_bot.as_ref().map(|b| b.bot_user.clone())).unwrap_or_default();
    // A thread with no mark is read from its start, for the owner's last
    // word; only the owner's messages are passed, so that is who sent this.
    let Some((said, more)) = api.replies(&m.channel, &m.thread, &thread.seen, THREAD_CAP).await? else {
        not_found(bot, m);
        return Ok(());
    };
    let gap = event::gap(said, &thread.seen, &m.ts, &bot_user, &m.user);
    let mentioned = event::mentioned(&m.text, &bot_user);
    let authors = gap.iter().map(|t| t.user.as_str()).chain([m.user.as_str()]);
    learn_names(bot, api, authors.chain(mentioned.iter().map(String::as_str))).await;
    let request = event::request(&m.text, &bot_user, |id| as_named(bot, id));
    let name = |id: &str| bot.names.lock().get(id).cloned();
    let prompt = event::passed(&request, &gap, more, |t| speaker(bot, t), |text| from_slack(text, name));

    let pane = match live_chat(&state, &thread.room) {
        Some(pane) => pane,
        None => {
            let handle = app.clone();
            let room = thread.room.clone();
            let pane = crate::commands::blocking(app.clone(), move |state| reopen(&handle, state, agent_id, &room)).await?;
            shown(app, &pane);
            pane
        }
    };
    // A conversation still opening takes it once it is ready (ACP-4).
    state.ptys.acp_prompt(&pane.id, &prompt, Vec::new())?;
    let (channel, ts, seen) = (m.channel.clone(), m.thread.clone(), m.ts.clone());
    let marked = crate::commands::blocking(app.clone(), move |state| {
        state.config.update(|c| {
            for t in c.slack_threads.iter_mut().filter(|t| t.channel == channel && t.ts == ts) {
                t.seen = event::newest(&t.seen, &seen).to_string();
            }
        })
    })
    .await;
    // The message reached the chat: a warning in the thread would say it had not.
    if let Err(e) = marked {
        bot.tell_error("The Slack bot could not note what its chat has read".into(), e.to_string());
    }
    Ok(())
}

/// Open the thread's chat again in its room: on the conversation it was in
/// when it was stopped for being idle (BOT-14), with the agent it was in,
/// or else on the room's newest.
fn reopen(app: &AppHandle, state: &AppState, agent_id: String, room: &str) -> Result<PaneInfo> {
    let waiting = |p: &SavedPane| p.waiting && p.acp && p.task_id == CHAT_TASK_ID && p.cwd.as_deref() == Some(room);
    let stopped = state.config.read().saved_panes.into_iter().rfind(|p| waiting(p));
    let (agent_id, resume) = match stopped {
        Some(SavedPane { agent_id: Some(agent), session: Some(session), .. }) => {
            (agent, Resume::Restart { cwd: room.to_string(), session: Some(session) })
        }
        _ => (agent_id, Resume::Newest),
    };
    let pane = crate::commands::open_chat(app, state, agent_id, None, Some(PathBuf::from(room)), resume, true)?;
    // Open again, so no longer waiting to be.
    state.config.update(|c| c.saved_panes.retain(|p| !waiting(p)))?;
    let _ = app.emit("panes:waiting", ());
    Ok(pane)
}

/// The chat running in `room`, if one is.
fn live_chat(state: &AppState, room: &str) -> Option<PaneInfo> {
    state.ptys.list(Some(CHAT_TASK_ID)).into_iter().find(|p| p.cwd == room && p.running && p.acp)
}

/// The window lists panes it is told about.
fn shown(app: &AppHandle, pane: &PaneInfo) {
    let _ = app.emit("pty:activity", &pane.id);
}

/// Who wrote a thread's message, as its chat is told.
fn speaker(bot: &Bot, t: &ThreadMessage) -> String {
    let name = bot.names.lock().get(&t.user).cloned();
    t.bot_name.clone().or(name).unwrap_or_else(|| if t.user.is_empty() { "a bot".into() } else { t.user.clone() })
}

/// A member as a request names them: a bot or an app is marked as one, so
/// the chat knows a rival when it is asked about one (BOT-16).
fn as_named(bot: &Bot, id: &str) -> Option<String> {
    let name = bot.names.lock().get(id).cloned()?;
    Some(if bot.bots.lock().contains(id) { format!("{name} {}", event::ANOTHER_BOT) } else { name })
}

/// Look up the names of members not yet known, a few at a time, and which
/// of them are bots.
pub(super) async fn learn_names<'a>(bot: &Bot, api: &BotApi, users: impl IntoIterator<Item = &'a str>) {
    let wanted: HashSet<String> = {
        let known = bot.names.lock();
        users.into_iter().filter(|u| !u.is_empty() && !known.contains_key(*u)).map(str::to_string).collect()
    };
    for user in wanted.into_iter().take(NAMES_MAX) {
        if let Ok((name, is_bot)) = api.member(&user).await {
            if is_bot {
                bot.bots.lock().insert(user.clone());
            }
            bot.names.lock().insert(user, name);
        }
    }
}
