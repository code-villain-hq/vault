//! A Slack thread's chat left alone is stopped (BOT-14), and waits to be
//! picked up again by the thread's next message (BOT-5) or from the Chat
//! view (PANE-7).
//!
//! Every new thread the bot was mentioned in opened a chat, and nothing
//! ever stopped one: an agent at its prompt never exits. Each kept a
//! process, its conversation in memory and a relay, took a place under
//! `MAX_PANES`, and came back at the next launch as one of the
//! `RESTORE_LIMIT` panes put back, ahead of the user's own. A week of
//! mentions was a week of agents, and the app slowed with them.

use std::time::Duration;

use chrono::{DateTime, Utc};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppState;
use crate::config::ConfigStore;
use crate::error::Result;
use crate::pty::acp::Quiet;
use crate::pty::{Activity, PtyManager};

/// How long a Slack thread's chat may sit idle before its agent is stopped.
///
/// Long enough for a conversation in the thread: a reply read and answered
/// over a coffee finds the agent still there. Picking it up again later
/// costs a few seconds of start-up and nothing of the conversation, so
/// there is no reason to hold it longer.
pub const IDLE_STOP: Duration = Duration::from_secs(30 * 60);

/// How soon a chat that is not idle is looked at again. A turn ending wakes
/// the relay anyway; looking away from it in the app does not.
const LOOK_AGAIN: Duration = Duration::from_secs(5 * 60);

/// How long until the chat may be stopped: zero when it may be now. One at
/// work, asking, or on screen in the app is not idle, whatever the time.
pub fn stop_in(q: &Quiet, now: DateTime<Utc>) -> Duration {
    if q.watched || !matches!(q.activity, Activity::Idle | Activity::Done) {
        return LOOK_AGAIN;
    }
    IDLE_STOP.saturating_sub((now - q.since).to_std().unwrap_or_default())
}

/// Stop the chat in `pane` if it is still idle. Under the bot's one event at
/// a time: a message from the thread arriving meanwhile either reaches the
/// chat before it stops or finds it stopped and picks it up again. True
/// when it was stopped.
pub async fn stop_if_idle(app: &AppHandle, pane: &str) -> bool {
    let state = app.state::<AppState>();
    let _one_at_a_time = state.slack_bot.serial.lock().await;
    let id = pane.to_string();
    let stopped =
        crate::commands::blocking(app.clone(), move |state| stop(&state.ptys, &state.config, &id, Utc::now())).await;
    if !matches!(stopped, Ok(true)) {
        return false;
    }
    // The chat list, and the chats waiting to be reopened, follow.
    let _ = app.emit("pty:activity", pane);
    let _ = app.emit("panes:waiting", ());
    true
}

/// Asked again here, in the same breath as the stop: the relay's timer is
/// minutes old by now.
fn stop(ptys: &PtyManager, config: &ConfigStore, pane: &str, now: DateTime<Utc>) -> Result<bool> {
    if !stop_in(&ptys.acp_quiet(pane)?, now).is_zero() {
        return Ok(false);
    }
    let session = ptys.session(pane)?;
    // Signalled, then killed if it will not go (PtyManager::stop): it is
    // marked as stopped on purpose, so its exit is no news.
    ptys.close(pane)?;
    // Waiting, not saved as open: it does not come back at launch, and its
    // conversation is what the thread's next message picks up (BOT-5).
    config.update(|c| {
        for p in c.saved_panes.iter_mut().filter(|p| p.id == pane) {
            p.waiting = true;
            p.session = session.clone().or(p.session.take());
        }
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use chrono::TimeDelta;

    use super::*;
    use crate::config::{AppConfig, SavedPane};
    use crate::pty::{PaneKind, SpawnOptions};

    fn quiet(activity: Activity, watched: bool, ago: i64) -> (Quiet, DateTime<Utc>) {
        let now = Utc::now();
        (Quiet { activity, watched, since: now - TimeDelta::minutes(ago) }, now)
    }

    #[test]
    fn a_chat_is_stopped_only_after_half_an_hour_of_nothing() {
        let (q, now) = quiet(Activity::Done, false, 31);
        assert!(stop_in(&q, now).is_zero());
        let (q, now) = quiet(Activity::Idle, false, 10);
        assert_eq!(stop_in(&q, now).as_secs() / 60, 20, "the rest of the half hour");
    }

    #[test]
    fn a_chat_at_work_asking_or_on_screen_is_never_stopped() {
        for (activity, watched) in [(Activity::Working, false), (Activity::Asking, false), (Activity::Idle, true)] {
            let (q, now) = quiet(activity, watched, 600);
            assert_eq!(stop_in(&q, now), LOOK_AGAIN, "{activity:?}, on screen: {watched}");
        }
    }

    #[test]
    fn an_idle_chat_stops_and_waits_to_be_picked_up_by_its_conversation() {
        let dir = std::env::temp_dir().join(format!("vl-idle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = tauri::test::mock_app();
        let ptys = PtyManager::default();
        let opts = SpawnOptions {
            task_id: crate::commands::CHAT_TASK_ID.into(),
            checkout_id: None,
            cwd: dir.to_string_lossy().to_string(),
            kind: PaneKind::Agent,
            title: "Fake".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), crate::pty::acp::tests::FAKE_AGENT.into()],
            agent_id: Some("fake".into()),
            rows: None,
            cols: None,
            initial_input: None,
            prompted: true,
            env: Vec::new(),
            title_activity: None,
            title_topic: None,
        };
        let launch =
            crate::acp::Launch { mcp: None, pick: crate::acp::Pick::New, prompt: Some("ping".into()), presets: Vec::new(), on_session: None };
        let pane = ptys.spawn_acp(app.handle(), opts, launch).unwrap();
        let saved = SavedPane {
            id: pane.id.clone(),
            task_id: pane.task_id.clone(),
            checkout_id: None,
            kind: "agent".into(),
            agent_id: Some("fake".into()),
            cwd: Some(pane.cwd.clone()),
            failed: 0,
            waiting: false,
            acp: true,
            session: None,
            on_loop: None,
        };
        let config = ConfigStore::for_tests(dir.join("config.json"), AppConfig { saved_panes: vec![saved], ..Default::default() });
        let deadline = Instant::now() + Duration::from_secs(10);
        while ptys.info(&pane.id).unwrap().activity != Activity::Done && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let later = Utc::now() + TimeDelta::minutes(31);

        assert!(!stop(&ptys, &config, &pane.id, Utc::now()).unwrap(), "it has only just answered");
        ptys.acp_view(&pane.id, None).unwrap();
        assert!(!stop(&ptys, &config, &pane.id, later).unwrap(), "it is on screen");
        ptys.detach(&pane.id);
        assert!(stop(&ptys, &config, &pane.id, later).unwrap());

        assert!(ptys.list(None).is_empty(), "the agent is gone");
        let row = &config.read().saved_panes[0];
        assert!(row.waiting, "it waits to be reopened rather than coming back at launch");
        assert_eq!(row.session.as_deref(), Some("s-1"), "on the conversation it was in");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
