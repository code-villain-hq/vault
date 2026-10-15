//! The Socket Mode connection (BOT-1, BOT-10): an outgoing websocket to
//! Slack, opened again whenever it drops, for as long as the bot is on.
//!
//! Slack sends each event in an envelope that has to be acknowledged within
//! three seconds, or it sends it again. The acknowledgement goes first, and
//! the event is handled after, on a task of its own: opening a chat takes
//! longer than three seconds.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio_tungstenite::tungstenite::Message as Frame;

use crate::commands::AppState;
use crate::integrations::slack_bot::BotApi;
use crate::secrets;

/// The longest wait before trying again (BOT-10).
const WAIT_MAX: Duration = Duration::from_secs(60);
/// Slack pings a connection now and then. With nothing heard for this long
/// the app asks; a Mac that slept leaves a socket that looks open and
/// carries nothing.
const QUIET: Duration = Duration::from_secs(60);
/// Asked and still nothing: the connection is gone.
const DEAD: Duration = Duration::from_secs(90);

enum Ended {
    /// The settings changed.
    Restart,
    /// Slack asked for a new connection, as it does every few hours.
    Refresh,
    Failed(String),
}

/// Keep the connection open while the bot is configured and on.
pub async fn run(app: AppHandle) {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    let mut generation = bot.generation.subscribe();
    let mut wait = Duration::from_secs(1);
    loop {
        generation.borrow_and_update();
        let on = state.config.read().slack_bot.is_some_and(|b| b.enabled);
        if !on {
            // Off is silent both ways: a chat's turns are not posted either.
            *bot.api.lock() = None;
            bot.set_status("off", None);
            if generation.changed().await.is_err() {
                return;
            }
            continue;
        }
        match connect(&app, &mut generation, &mut wait).await {
            Ended::Restart | Ended::Refresh => continue,
            Ended::Failed(e) => {
                bot.set_status("failing", Some(e));
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    r = generation.changed() => if r.is_err() { return },
                }
                wait = (wait * 2).min(WAIT_MAX);
            }
        }
    }
}

async fn connect(app: &AppHandle, generation: &mut tokio::sync::watch::Receiver<u64>, wait: &mut Duration) -> Ended {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    bot.set_status("connecting", None);
    // The keychain can ask for a password, which waits on the person.
    let tokens = crate::commands::off_runtime(|| {
        // A bot already connected takes over notifications' old token's work.
        let _ = secrets::forget_old_slack();
        Ok::<_, crate::error::Error>((secrets::get(secrets::SLACK_BOT)?, secrets::get(secrets::SLACK_BOT_APP)?))
    })
    .await;
    let (bot_token, app_token) = match tokens {
        Ok(Ok((Some(b), Some(a)))) => (b, a),
        Ok(Ok(_)) => return Ended::Failed("its tokens are not in the keychain; connect the bot again".into()),
        Ok(Err(e)) | Err(e) => return Ended::Failed(e.to_string()),
    };
    *bot.api.lock() = Some(BotApi::new(&bot_token));

    let url = match BotApi::socket_url(&app_token).await {
        Ok(url) => url,
        Err(e) => return Ended::Failed(e.to_string()),
    };
    let mut ws = match tokio_tungstenite::connect_async(url.as_str()).await {
        Ok((ws, _)) => ws,
        Err(e) => return Ended::Failed(format!("could not reach Slack: {e}")),
    };

    let mut heard = tokio::time::Instant::now();
    let mut asked = false;
    loop {
        let quiet = if asked { DEAD } else { QUIET };
        let frame = tokio::select! {
            f = ws.next() => f,
            r = generation.changed() => {
                let _ = ws.close(None).await;
                return if r.is_err() { Ended::Failed("the app is closing".into()) } else { Ended::Restart };
            }
            _ = tokio::time::sleep_until(heard + quiet) => {
                if asked {
                    return Ended::Failed("Slack stopped answering".into());
                }
                asked = true;
                if ws.send(Frame::Ping(Default::default())).await.is_err() {
                    return Ended::Failed("the connection to Slack broke".into());
                }
                continue;
            }
        };
        heard = tokio::time::Instant::now();
        asked = false;
        let text = match frame {
            Some(Ok(Frame::Text(t))) => t,
            Some(Ok(Frame::Close(_))) | None => return Ended::Failed("Slack closed the connection".into()),
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Ended::Failed(format!("the connection to Slack broke: {e}")),
        };
        let Ok(envelope) = serde_json::from_str::<Value>(text.as_str()) else { continue };
        match envelope.get("type").and_then(Value::as_str).unwrap_or_default() {
            "hello" => {
                bot.set_status("connected", None);
                *wait = Duration::from_secs(1);
            }
            "disconnect" => {
                return match envelope.get("reason").and_then(Value::as_str) {
                    Some("link_disabled") => Ended::Failed("Socket Mode is off for this Slack app; turn it on under Socket Mode".into()),
                    _ => Ended::Refresh,
                };
            }
            _ => {
                if let Some(id) = envelope.get("envelope_id").and_then(Value::as_str) {
                    let ack = json!({ "envelope_id": id }).to_string();
                    if ws.send(Frame::Text(ack.into())).await.is_err() {
                        return Ended::Failed("the connection to Slack broke".into());
                    }
                }
                match envelope.get("type").and_then(Value::as_str) {
                    Some("events_api") => {
                        if let Some(event) = envelope.pointer("/payload/event").cloned() {
                            tauri::async_runtime::spawn(super::handle::event(app.clone(), event));
                        }
                    }
                    // A button on a permission card (BOT-7).
                    Some("interactive") => {
                        if let Some(payload) = envelope.get("payload").cloned() {
                            tauri::async_runtime::spawn(super::handle::click(app.clone(), payload));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
