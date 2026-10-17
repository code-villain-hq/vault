//! From a chat to its Slack thread (BOT-6, BOT-7, BOT-9): one task per chat
//! the bot handed a thread to. It waits on the pane's turns, rung by the
//! agent's own reports as a loop's are, and on its printing, which is when a
//! permission question can appear. It reads the conversation as the window
//! does, by revision, without marking it seen.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use tauri::{AppHandle, Manager};

use super::card::Asked;
use super::event::FROM_SLACK;
use crate::acp::{self, Body};
use crate::commands::AppState;
use crate::config::SlackThread;
use crate::integrations::slack_bot::{clip, split_reply, PARTS_MAX, PART_MAX};

/// A reply streams in as dozens of chunks a second. Each is a wake, and a
/// read of what changed is only worth it once they settle.
const GATHER: Duration = Duration::from_millis(400);
/// What one quoted message typed in the app may take in the thread.
const TYPED_MAX: usize = 1500;
/// The notes a progress message shows: the latest, each clipped. It says
/// where things stand, not everything that was said.
const NOTES_SHOWN: usize = 8;
const NOTE_MAX: usize = 400;

/// What a turn has for the thread.
#[derive(Debug, Default, PartialEq)]
pub struct Turn {
    /// What the owner typed into the chat itself, not from Slack.
    pub typed: Vec<String>,
    /// The agent's last message in the turn.
    pub reply: Option<String>,
    pub failed: Vec<String>,
}

pub fn turn<'a>(entries: impl IntoIterator<Item = &'a Body>) -> Turn {
    let mut t = Turn::default();
    for body in entries {
        match body {
            Body::User { text, dropped: false, .. } if !text.starts_with(FROM_SLACK) => t.typed.push(text.clone()),
            Body::Agent { text } if !text.trim().is_empty() => t.reply = Some(text.clone()),
            Body::Note { text, error: true } => t.failed.push(text.clone()),
            _ => {}
        }
    }
    t
}

/// The messages a turn becomes, in the order they are posted.
pub fn posts(t: &Turn) -> Vec<String> {
    let mut out = Vec::new();
    if !t.typed.is_empty() {
        let quoted: Vec<String> = t
            .typed
            .iter()
            .map(|m| clip(m, TYPED_MAX).lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"))
            .collect();
        out.push(format!("_Typed in Layer:_\n{}", quoted.join("\n>\n")));
    }
    if let Some(reply) = &t.reply {
        out.extend(split_reply(reply, PART_MAX, PARTS_MAX));
    }
    out.extend(t.failed.iter().map(|f| format!(":warning: {f}")));
    out
}

/// What the agent said between its tools in a turn (BOT-6): each of its
/// messages that something came after. The last may still be streaming.
fn notes<'a>(kept: impl IntoIterator<Item = &'a Body>) -> Vec<String> {
    let bodies: Vec<&Body> = kept.into_iter().collect();
    bodies
        .iter()
        .enumerate()
        .filter(|(i, _)| i + 1 < bodies.len())
        .filter_map(|(_, b)| match b {
            Body::Agent { text } if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
        .collect()
}

/// A turn's progress message: its notes, the latest last, and whether the
/// turn is over.
pub fn progress_text(notes: &[String], done: bool) -> String {
    let head = if done { "_Along the way:_" } else { ":hourglass_flowing_sand: _Working on it…_" };
    let skipped = notes.len().saturating_sub(NOTES_SHOWN);
    let mut out = head.to_string();
    if skipped > 0 {
        out.push_str(&format!("\n\n_…{skipped} earlier_"));
    }
    for n in &notes[skipped..] {
        out.push_str("\n\n");
        out.push_str(&clip(n, NOTE_MAX).lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"));
    }
    out
}

/// What a relay has read of a conversation, and what it still owes the
/// thread.
#[derive(Default)]
pub struct Reader {
    rev: Option<u64>,
    /// What is new since the bot began watching, by entry (BOT-9). None
    /// until the conversation is ready.
    from: Option<usize>,
    kept: BTreeMap<usize, Body>,
    /// The turn's permission questions (BOT-7), as its card shows them.
    asked: BTreeMap<usize, Asked>,
    /// The turn's notes its progress message shows (BOT-6).
    noted: usize,
}

/// What a read of a conversation has for its thread.
#[derive(Debug, Default, PartialEq)]
pub struct Out {
    /// The turn's card, when it changed: posted the first time, changed
    /// after.
    pub card: Option<Vec<Asked>>,
    /// The turn's progress message, when it changed: posted the first
    /// time, changed after.
    pub progress: Option<String>,
    /// Messages to post, after the card and the progress.
    pub posts: Vec<String>,
    /// The turn is over: the next question starts a card of its own.
    pub turn_over: bool,
}

impl Reader {
    /// The revision to ask the conversation for changes after.
    pub fn since(&self) -> Option<u64> {
        self.rev
    }

    /// Take in what changed; `ended` says a turn ended before it was read.
    pub fn read(&mut self, view: acp::View, ended: bool) -> Out {
        self.rev = Some(view.changes.rev);
        if view.changes.reset {
            self.kept.clear();
        }
        // A conversation picked up replays its history before it is ready:
        // none of that is news to the thread.
        if self.from.is_none() && view.ready {
            self.from = Some(view.changes.len);
        }
        let Some(start) = self.from else { return Out::default() };
        let mut card_changed = false;
        for e in view.changes.entries.into_iter().filter(|e| e.index >= start) {
            if let Body::Question { title, options, answer } = &e.body {
                let asked = Asked { entry: e.index, title: title.clone(), options: options.clone(), answer: answer.clone() };
                if self.asked.get(&e.index) != Some(&asked) {
                    self.asked.insert(e.index, asked);
                    card_changed = true;
                }
            }
            self.kept.insert(e.index, e.body);
        }
        let mut out = Out { card: card_changed.then(|| self.asked.values().cloned().collect()), ..Out::default() };
        let notes = notes(self.kept.values());
        if ended {
            let t = turn(self.kept.values());
            // The reply is posted on its own: the notes are what led to it.
            // A turn that never needed a progress message gets none now.
            if self.noted > 0 {
                let before: Vec<String> = notes.into_iter().filter(|n| Some(n.as_str()) != t.reply.as_deref().map(str::trim)).collect();
                out.progress = Some(progress_text(&before, true));
            }
            out.posts = posts(&t);
            out.turn_over = true;
            self.noted = 0;
            self.from = Some(view.changes.len);
            self.kept.clear();
            self.asked.clear();
        } else if notes.len() > self.noted {
            self.noted = notes.len();
            out.progress = Some(progress_text(&notes, false));
        }
        out
    }
}

/// Follow a chat's pane until it exits or is stopped for being idle
/// (BOT-14), posting each turn to `thread`.
pub async fn follow(app: AppHandle, pane: String, thread: SlackThread) {
    let state = app.state::<AppState>();
    if let (Ok(mut turns), Ok(mut feed)) = (state.ptys.turns(&pane), state.ptys.feed(&pane)) {
        let mut seen = *turns.borrow_and_update();
        let mut reader = Reader::default();
        // The turn's card and progress message, once posted.
        let mut card: Option<String> = None;
        let mut progress: Option<String> = None;
        loop {
            // The turn first, then what it said: read the other way round,
            // the end of a reply could land between the two.
            let now = *turns.borrow_and_update();
            let ended = now != seen;
            seen = now;
            let Ok(view) = state.ptys.acp_read(&pane, reader.since()) else { break };
            let exited = view.exited;
            let out = reader.read(view, ended);
            if let Some(asked) = &out.card {
                card = state.slack_bot.card(&thread, card.take(), asked).await;
            }
            if let Some(text) = &out.progress {
                progress = state.slack_bot.progress(&thread, progress.take(), text).await;
            }
            if !out.posts.is_empty() {
                state.slack_bot.post(&thread, out.posts).await;
            }
            if out.turn_over {
                card = None;
                progress = None;
            }
            // An exit is the end of this chat: a restart is a new pane, which
            // the bot follows anew. Saying so in the thread would say it for
            // every chat each time the app quits.
            if exited {
                break;
            }
            // Anything the chat does wakes the loop and starts the wait
            // again; only one left alone sleeps it out (BOT-14).
            let idle = state.ptys.acp_quiet(&pane).map(|q| super::idle::stop_in(&q, Utc::now()));
            tokio::select! {
                r = turns.changed() => if r.is_err() { break },
                _ = feed.printed() => tokio::time::sleep(GATHER).await,
                _ = tokio::time::sleep(idle.unwrap_or(super::idle::IDLE_STOP)) => {
                    if super::idle::stop_if_idle(&app, &pane).await {
                        break;
                    }
                }
            }
        }
    }
    state.slack_bot.unfollow(&pane);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> Body {
        Body::User { text: text.into(), queued: false, dropped: false }
    }

    fn agent(text: &str) -> Body {
        Body::Agent { text: text.into() }
    }

    #[test]
    fn a_turn_posts_its_last_reply_and_what_was_typed_in_the_app() {
        let entries = [
            user("[Slack] and the logs?"),
            user("also check staging"),
            agent("Looking."),
            Body::Thought { text: "hmm".into() },
            agent("Staging is fine; prod has 500s since 9:02."),
        ];
        let t = turn(&entries);
        assert_eq!(t.typed, vec!["also check staging".to_string()]);
        assert_eq!(t.reply.as_deref(), Some("Staging is fine; prod has 500s since 9:02."));
        let p = posts(&t);
        assert_eq!(p, vec![
            "_Typed in Layer:_\n> also check staging".to_string(),
            "Staging is fine; prod has 500s since 9:02.".to_string(),
        ]);
    }

    #[test]
    fn a_turn_with_nothing_to_say_posts_nothing_and_a_failure_says_so() {
        assert!(posts(&turn(&[user("[Slack] go"), Body::Plan { steps: vec![] }])).is_empty());
        let dropped = Body::User { text: "never sent".into(), queued: false, dropped: true };
        assert!(turn(&[dropped]).typed.is_empty());
        let failed = turn(&[Body::Note { text: "Stopped: the reply reached the model's length limit.".into(), error: true }]);
        assert_eq!(posts(&failed), vec![":warning: Stopped: the reply reached the model's length limit.".to_string()]);
    }

    fn view(rev: u64, len: usize, ready: bool, entries: Vec<(usize, Body)>) -> acp::View {
        acp::View {
            changes: acp::Changes {
                rev,
                base: 0,
                len,
                reset: false,
                entries: entries.into_iter().map(|(index, body)| acp::Entry { index, rev, body }).collect(),
            },
            agent: None,
            ready,
            busy: false,
            queued: 0,
            images: false,
            settings: vec![],
            commands: vec![],
            usage: None,
            exited: false,
        }
    }

    #[test]
    fn a_picked_up_conversation_does_not_post_its_history_again() {
        let mut r = Reader::default();
        // Replayed while it opens: not ready yet.
        let replay = vec![(0, user("[Slack] old")), (1, agent("an old answer"))];
        assert_eq!(r.read(view(2, 2, false, replay), false), Out::default());
        // Ready, and a turn that ended before the relay looked is still old.
        assert!(r.read(view(3, 2, true, vec![]), true).posts.is_empty());
        // What comes after is news.
        let now = vec![(2, user("[Slack] and now?")), (3, agent("Now it is fixed."))];
        assert!(r.read(view(5, 4, true, now), false).posts.is_empty(), "nothing until the turn ends");
        assert_eq!(r.read(view(5, 4, true, vec![]), true).posts, vec!["Now it is fixed.".to_string()]);
        assert!(r.read(view(5, 4, true, vec![]), true).posts.is_empty(), "a turn is posted once");
    }

    fn question(answer: Option<&str>) -> Body {
        Body::Question { title: "Edit main.rs".into(), options: vec![], answer: answer.map(str::to_string) }
    }

    #[test]
    fn a_turns_questions_share_one_card_changed_only_when_they_change() {
        let mut r = Reader::default();
        assert_eq!(r.read(view(1, 0, true, vec![]), false), Out::default());
        let first = r.read(view(2, 1, true, vec![(0, question(None))]), false).card.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(r.read(view(3, 2, true, vec![(1, agent("still"))]), false).card, None, "nothing new to show");
        let second = r.read(view(4, 3, true, vec![(2, question(None))]), false).card.unwrap();
        assert_eq!(second.len(), 2, "the same card, with both");
        let answered = r.read(view(5, 3, true, vec![(0, question(Some("allow_once")))]), false).card.unwrap();
        assert_eq!(answered[0].answer.as_deref(), Some("allow_once"));
    }

    #[test]
    fn the_next_turns_questions_start_a_card_of_their_own() {
        let mut r = Reader::default();
        r.read(view(1, 0, true, vec![]), false);
        r.read(view(2, 1, true, vec![(0, question(None))]), false);
        let end = r.read(view(3, 2, true, vec![(0, question(Some("cancelled"))), (1, agent("done"))]), true);
        assert_eq!(end.card.unwrap()[0].answer.as_deref(), Some("cancelled"), "the old card shows how it ended");
        assert!(end.turn_over);
        let next = r.read(view(4, 3, true, vec![(2, question(None))]), false).card.unwrap();
        assert_eq!(next.len(), 1, "only the new turn's question");
    }

    fn tool(id: &str) -> Body {
        Body::Tool { id: id.into(), title: id.into(), tool: "fetch".into(), status: "completed".into(), content: vec![], locations: vec![] }
    }

    #[test]
    fn what_the_agent_says_between_its_tools_reaches_the_thread_while_it_works() {
        let mut r = Reader::default();
        r.read(view(1, 0, true, vec![]), false);
        let first = vec![(0, user("[Slack] are you there?")), (1, tool("read_thread")), (2, agent("Still here, pulling the logs.")), (3, tool("grafana_logs"))];
        let out = r.read(view(2, 4, true, first), false);
        assert_eq!(out.progress.as_deref(), Some(":hourglass_flowing_sand: _Working on it…_\n\n> Still here, pulling the logs."));
        assert!(out.posts.is_empty(), "the reply waits for the turn's end");
        // Still streaming: nothing came after it yet.
        assert_eq!(r.read(view(3, 5, true, vec![(4, agent("Found 4 fail"))]), false).progress, None);
        let later = r.read(view(4, 6, true, vec![(4, agent("Found 4 failures.")), (5, tool("s3_logs_query"))]), false);
        assert!(later.progress.unwrap().ends_with("> Still here, pulling the logs.\n\n> Found 4 failures."));
        assert_eq!(r.read(view(5, 6, true, vec![]), false).progress, None, "unchanged, not posted again");
        let end = r.read(view(6, 7, true, vec![(6, agent("All 4 trips failed at issuance."))]), true);
        assert!(end.progress.unwrap().starts_with("_Along the way:_"));
        assert_eq!(end.posts, vec!["All 4 trips failed at issuance.".to_string()]);
    }

    #[test]
    fn a_turn_that_only_answers_gets_no_progress_message() {
        let mut r = Reader::default();
        r.read(view(1, 0, true, vec![]), false);
        let out = r.read(view(2, 2, true, vec![(0, user("[Slack] hi")), (1, agent("Hello."))]), true);
        assert_eq!(out.progress, None);
        assert_eq!(out.posts, vec!["Hello.".to_string()]);
    }

    #[test]
    fn a_long_turn_shows_its_latest_notes() {
        let notes: Vec<String> = (0..12).map(|i| format!("note {i}")).collect();
        let text = progress_text(&notes, false);
        assert!(text.contains("_…4 earlier_") && text.ends_with("> note 11") && !text.contains("note 3\n"), "{text}");
    }
}
