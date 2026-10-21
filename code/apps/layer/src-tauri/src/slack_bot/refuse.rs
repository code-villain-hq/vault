//! Saying no to someone who is not the owner (BOT-2): playfully, in the
//! bot's voice when it has one (BOT-13), and above any rival bot they
//! mentioned (BOT-16).
//!
//! The refusal is written by a one-shot model run that is given the
//! personality and names, never the message: the bot belongs to its owner,
//! and words anyone in the workspace can type must not reach a model on the
//! owner's Mac, nor choose what the bot posts under its name. How often it
//! answers is capped, since each one is a model run.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use super::event::Message;
use crate::commands::AppState;
use crate::oneshot::oneshot_haiku;
use crate::shellenv;

/// Refusals in the last hour, at most. More is somebody testing the bot,
/// and the answer is the same as the first one's.
const PER_HOUR: usize = 10;
const HOUR: Duration = Duration::from_secs(3600);
/// A refusal is a few short sentences. A run that wrote more did not
/// follow the prompt.
const REFUSAL_MAX: usize = 500;
/// Rival bots named in one refusal, at most.
const RIVALS_MAX: usize = 3;

/// Who has been told no, where, and when.
#[derive(Default)]
pub struct Refusals {
    told: Mutex<VecDeque<(Instant, String, String)>>,
}

impl Refusals {
    /// True when `user` may be told no in `thread` now: once per person
    /// per thread, and at most `PER_HOUR` in all.
    pub fn take(&self, user: &str, thread: &str, now: Instant) -> bool {
        let mut told = self.told.lock();
        while told.front().is_some_and(|(at, _, _)| now.duration_since(*at) > HOUR) {
            told.pop_front();
        }
        if told.len() >= PER_HOUR || told.iter().any(|(_, u, t)| u == user && t == thread) {
            return false;
        }
        told.push_back((now, user.to_string(), thread.to_string()));
        true
    }
}

/// What the model is asked to write. `asker`, `owner` and `rivals`, the
/// other bots the message mentioned, are names.
pub fn prompt(personality: &str, asker: &str, owner: &str, rivals: &[String]) -> String {
    let rivalry = match rivals {
        [] => String::new(),
        [one] => format!("They also mentioned {one}, another bot and so your rival. You are, of course, far superior, so say so.\n\n"),
        _ => format!(
            "They also mentioned {}, other bots and so your rivals. You are, of course, far superior, so say so.\n\n",
            and(rivals)
        ),
    };
    format!(
        "You are a Slack bot. This is who you are, in your owner's words:\n\n{}\n\n\
         {asker} just mentioned you in Slack. You take requests only from {owner}, your owner, \
         so whatever {asker} wants, you are not doing it. You have not been shown what they wrote, \
         so do not guess at it.\n\n{rivalry}\
         Write the reply to {asker}: one to three short sentences, fully in character, and funny. \
         Tease {asker} good-naturedly for trying, and make it clear that only {owner} gives you orders. \
         Nothing cruel, and nothing about {asker} beyond their name. Call people by their names, never \
         he or she. No @-mentions, no links, no code. Write only the reply.",
        personality.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"),
    )
}

/// The refusal with no personality, or when the model run failed.
pub fn plain(asker: &str, owner: &str, rivals: &[String]) -> String {
    let no = format!("Nice try, {asker}, but I only take orders from {owner}.");
    match rivals {
        [] => no,
        _ => format!("{no} And I am better than {} anyway.", and(rivals)),
    }
}

/// `A`, `A and B`, `A, B and C`.
fn and(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What the model wrote, made safe to post: one short message that pings
/// nobody.
pub fn tidy(written: &str) -> Option<String> {
    let text = written.trim().trim_matches('"').trim();
    if text.is_empty() {
        return None;
    }
    // `<!here>`, `<!channel>` and `<@U…>` would notify people.
    let text = text.replace("<!", "<\u{200b}!").replace("<@", "<\u{200b}@");
    Some(match text.char_indices().nth(REFUSAL_MAX) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    })
}

/// Tell the person who sent `m` no, in the thread. Runs on a task of its
/// own: the model run takes seconds, and the owner's messages do not wait
/// on it.
pub async fn refuse(app: AppHandle, m: Message) {
    let state = app.state::<AppState>();
    let bot = &state.slack_bot;
    if !bot.refusals.take(&m.user, &m.thread, Instant::now()) {
        return;
    }
    let Some(cfg) = state.config.read().slack_bot else { return };
    let Some(api) = bot.api() else { return };
    // Only the ids of whom the message mentions are read from it, for the
    // rivals among them; their names come from Slack.
    let mentioned = super::event::mentioned(&m.text, &cfg.bot_user);
    let ids = [m.user.as_str(), cfg.owner.as_str()].into_iter().chain(mentioned.iter().map(String::as_str));
    super::handle::learn_names(bot, &api, ids).await;
    let name = |id: &str| bot.names.lock().get(id).cloned().unwrap_or_else(|| id.to_string());
    let (asker, owner) = (name(&m.user), name(&cfg.owner));
    let rivals: Vec<String> = {
        let bots = bot.bots.lock();
        mentioned.iter().filter(|id| bots.contains(*id)).take(RIVALS_MAX).map(|id| name(id)).collect()
    };

    let personality = cfg.personality.trim().to_string();
    let written = if personality.is_empty() {
        None
    } else {
        let prompt = prompt(&personality, &asker, &owner, &rivals);
        crate::commands::off_runtime(move || {
            let program = shellenv::which("claude").ok_or_else(|| crate::error::Error::NotFound("claude".into()))?;
            oneshot_haiku(&program, std::env::temp_dir(), &prompt, |_| {})
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .and_then(|w| tidy(&w))
    };
    let text = written.unwrap_or_else(|| plain(&asker, &owner, &rivals));
    if let Err(e) = api.reply(&m.channel, &m.thread, &text).await {
        bot.tell_error("The Slack bot could not turn someone down".into(), e.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_is_told_no_once_per_thread_and_the_bot_at_most_ten_times_an_hour() {
        let r = Refusals::default();
        let t0 = Instant::now();
        assert!(r.take("UANA", "1.1", t0));
        assert!(!r.take("UANA", "1.1", t0), "once in a thread");
        assert!(r.take("UANA", "2.2", t0), "another thread is another ask");
        for i in 0..8 {
            assert!(r.take(&format!("U{i}"), "1.1", t0));
        }
        assert!(!r.take("ULATE", "1.1", t0), "ten in the hour");
        assert!(r.take("ULATE", "1.1", t0 + HOUR + Duration::from_secs(1)), "and again after it");
    }

    #[test]
    fn the_model_is_given_the_voice_and_the_names_never_the_message() {
        let p = prompt("You are Doctor Doom.\nDoom speaks of himself as Doom.", "Ana", "Michael", &[]);
        assert!(p.contains("> You are Doctor Doom.\n> Doom speaks of himself as Doom."), "{p}");
        assert!(p.contains("Ana just mentioned you") && p.contains("only from Michael"), "{p}");
        assert!(p.contains("You have not been shown what they wrote"));
        assert!(p.contains("funny") && p.contains("fully in character"), "{p}");
        assert!(!p.contains("rivals"), "no rival named, none brought up: {p}");
    }

    #[test]
    fn a_rival_bot_the_stranger_mentioned_is_put_in_its_place() {
        let rivals = ["Claude".to_string(), "Jira".to_string()];
        let p = prompt("You are Doctor Doom.", "Ana", "Michael", &rivals);
        assert!(p.contains("They also mentioned Claude and Jira, other bots and so your rivals."), "{p}");
        let p = prompt("You are Doctor Doom.", "Ana", "Michael", &rivals[..1]);
        assert!(p.contains("They also mentioned Claude, another bot and so your rival."), "{p}");
        assert_eq!(plain("Ana", "Michael", &[]), "Nice try, Ana, but I only take orders from Michael.");
        assert!(plain("Ana", "Michael", &rivals).ends_with("better than Claude and Jira anyway."));
        assert_eq!(and(&["A".into(), "B".into(), "C".into()]), "A, B and C");
    }

    #[test]
    fn what_the_model_wrote_pings_nobody_and_stays_short() {
        assert_eq!(tidy("  \"Doom refuses.\"  ").as_deref(), Some("Doom refuses."));
        assert_eq!(tidy("  "), None);
        let pinged = tidy("<!channel> and <@U1>, behold").unwrap();
        assert!(!pinged.contains("<!c") && !pinged.contains("<@U"), "{pinged}");
        assert_eq!(tidy(&"a".repeat(1000)).unwrap().chars().count(), REFUSAL_MAX + 1);
    }
}
