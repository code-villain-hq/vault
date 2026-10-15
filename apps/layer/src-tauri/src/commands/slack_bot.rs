//! The Slack bot's settings (§24): connecting it, switching it off and on,
//! and how it is.

use serde::Serialize;
use tauri::State;

use crate::agents;
use crate::config::SlackBotConfig;
use crate::error::{Error, Result};
use crate::integrations::slack_bot::BotApi;
use crate::secrets;
use crate::slack_bot::Status;

use super::{AppState, CHAT_TASK_ID};

/// A personality goes into every chat's opening prompt and every refusal's:
/// a character sketch, not a document.
const PERSONALITY_MAX: usize = 2000;

/// The bot, as Settings and the chat view show it.
#[derive(Debug, Serialize)]
pub struct SlackBotView {
    pub status: Status,
    pub threads: Vec<ThreadView>,
}

/// A Slack thread and its chat (BOT-11).
#[derive(Debug, Serialize)]
pub struct ThreadView {
    pub room: String,
    pub label: String,
    pub link: String,
    /// The chat's pane, while it is open.
    pub pane_id: Option<String>,
}

/// Connect the bot. Both tokens are checked with Slack before anything is
/// saved (BOT-10); one left blank keeps the one saved.
#[tauri::command]
pub async fn slack_bot_connect(
    state: State<'_, AppState>,
    bot_token: String,
    app_token: String,
    owner: String,
    agent_id: String,
) -> Result<()> {
    let owner = owner.trim().to_string();
    if !member_id(&owner) {
        return Err(Error::Other(
            "a member ID looks like U0123ABCD. In Slack, open your profile, then ⋯ → Copy member ID.".into(),
        ));
    }
    conversational(&agent_id)?;
    let (bot_token, app_token) = super::off_runtime(move || -> Result<(String, String)> {
        Ok((kept_or(secrets::SLACK_BOT, &bot_token)?, kept_or(secrets::SLACK_BOT_APP, &app_token)?))
    })
    .await??;
    if !bot_token.starts_with("xoxb-") {
        return Err(Error::Other("the bot token starts with xoxb-: OAuth & Permissions → Bot User OAuth Token.".into()));
    }
    if !app_token.starts_with("xapp-") {
        return Err(Error::Other(
            "the app-level token starts with xapp-: Basic Information → App-Level Tokens, with connections:write.".into(),
        ));
    }
    let me = BotApi::new(&bot_token).whoami().await?;
    // Each address is good for one connection; this one only proves the
    // token can open one.
    BotApi::socket_url(&app_token).await?;
    super::off_runtime(move || -> Result<()> {
        secrets::set(secrets::SLACK_BOT, &bot_token)?;
        secrets::set(secrets::SLACK_BOT_APP, &app_token)?;
        // Notifications post as this bot now; their channel and switches stay.
        secrets::forget_old_slack()
    })
    .await??;
    state.config.update(|c| {
        // Connecting again changes who and how, not how it behaves.
        let auto = c.slack_bot.as_ref().is_none_or(|b| b.auto);
        let personality = c.slack_bot.as_ref().map(|b| b.personality.clone()).unwrap_or_default();
        // A model is one agent's name for it: another agent starts on its own.
        let model = c.slack_bot.as_ref().filter(|b| b.agent_id == agent_id).map(|b| b.model.clone()).unwrap_or_default();
        c.slack_bot = Some(SlackBotConfig {
            owner,
            agent_id,
            bot_user: me.user,
            bot_name: me.name,
            enabled: true,
            auto,
            personality,
            model,
        });
    })?;
    state.slack_bot.restart();
    Ok(())
}

/// The agent a thread is handed to and the model its chats run on
/// (BOT-17), empty for the agent's own. Chats already open keep theirs.
#[tauri::command]
pub fn set_slack_bot_agent(state: State<AppState>, agent_id: String, model: String) -> Result<()> {
    conversational(&agent_id)?;
    state.config.update(|c| {
        if let Some(b) = c.slack_bot.as_mut() {
            b.agent_id = agent_id;
            b.model = model.trim().to_string();
        }
    })
}

/// The bot needs an agent that runs as a conversation (ACP).
fn conversational(agent_id: &str) -> Result<()> {
    let def = agents::find(agent_id).ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
    if def.acp.is_none() {
        return Err(Error::Other(format!("{} cannot run as a conversation (ACP), which the bot needs", def.name)));
    }
    Ok(())
}

/// The bot on or off, keeping its tokens and threads.
#[tauri::command]
pub fn set_slack_bot_on(state: State<AppState>, on: bool) -> Result<()> {
    state.config.update(|c| {
        if let Some(b) = c.slack_bot.as_mut() {
            b.enabled = on;
        }
    })?;
    state.slack_bot.restart();
    Ok(())
}

/// Whether chats opened from Slack start in the agent's `auto` mode
/// (BOT-12). Chats already open keep the mode they have.
#[tauri::command]
pub fn set_slack_bot_auto(state: State<AppState>, on: bool) -> Result<()> {
    state.config.update(|c| {
        if let Some(b) = c.slack_bot.as_mut() {
            b.auto = on;
        }
    })
}

/// The bot's personality (BOT-13): the voice chats opened from now on
/// answer in.
#[tauri::command]
pub fn set_slack_bot_personality(state: State<AppState>, text: String) -> Result<()> {
    let text = text.trim();
    if text.chars().count() > PERSONALITY_MAX {
        return Err(Error::Other(format!("a personality is at most {PERSONALITY_MAX} characters")));
    }
    state.config.update(|c| {
        if let Some(b) = c.slack_bot.as_mut() {
            b.personality = text.to_string();
        }
    })
}

#[tauri::command]
pub fn slack_bot_view(state: State<AppState>) -> SlackBotView {
    let chats = state.ptys.list(Some(CHAT_TASK_ID));
    let threads = state
        .config
        .read()
        .slack_threads
        .into_iter()
        .rev()
        .map(|t| ThreadView {
            pane_id: chats.iter().find(|p| p.cwd == t.room && p.running).map(|p| p.id.clone()),
            room: t.room,
            label: t.label,
            link: t.link,
        })
        .collect();
    SlackBotView { status: state.slack_bot.status(), threads }
}

/// Forget the bot's tokens and settings (SET-2), and the notifications
/// posted as it. Its threads stay tied to their chats, for a bot connected
/// again.
pub(crate) fn disconnect_bot(state: &AppState) -> Result<()> {
    secrets::delete(secrets::SLACK_BOT)?;
    secrets::delete(secrets::SLACK_BOT_APP)?;
    secrets::forget_old_slack()?;
    state.config.update(|c| {
        c.slack_bot = None;
        c.slack = None;
    })?;
    state.slack_bot.restart();
    Ok(())
}

fn kept_or(key: &str, typed: &str) -> Result<String> {
    let typed = typed.trim();
    if !typed.is_empty() {
        return Ok(typed.to_string());
    }
    secrets::get(key)?.ok_or_else(|| Error::Other("paste both tokens to connect the bot".into()))
}

/// `U…` or, in an Enterprise Grid, `W…`.
fn member_id(id: &str) -> bool {
    id.len() >= 9
        && id.starts_with(['U', 'W'])
        && id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_id_is_checked_before_the_bot_listens_to_it() {
        assert!(member_id("U0123ABCD"));
        assert!(member_id("W0123ABCDE"));
        assert!(!member_id("U12"));
        assert!(!member_id("@michael"));
        assert!(!member_id("u0123abcd"));
        assert!(!member_id(""));
    }
}
