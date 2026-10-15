//! The permission card (BOT-7): one message in the thread for a turn's
//! questions, changed as they come and are answered, with a button for each
//! answer. Pure: `relay.rs` posts it and `handle.rs` takes the clicks.

use serde_json::{json, Value};

use crate::acp::Choice;
use crate::integrations::slack_bot::clip;

/// Slack takes at most 25 buttons in one actions block, and a message of
/// 50 blocks. A turn asking more than this is answered in the app.
const SHOWN_MAX: usize = 12;

/// A question a turn asked, as the card shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Its entry in the conversation: what an answer names.
    pub entry: usize,
    pub title: String,
    pub options: Vec<Choice>,
    /// The option chosen, or "cancelled"; None while it waits.
    pub answer: Option<String>,
}

/// What a button carries back when clicked: whose question, and which
/// answer. The chat is named by its room, which outlives its pane.
#[derive(Debug, Clone, PartialEq)]
pub struct Click {
    pub room: String,
    pub entry: usize,
    pub option: String,
}

impl Click {
    pub fn value(&self) -> String {
        json!({ "room": self.room, "entry": self.entry, "option": self.option }).to_string()
    }

    pub fn read(value: &str) -> Option<Click> {
        let v: Value = serde_json::from_str(value).ok()?;
        Some(Click {
            room: v.get("room")?.as_str()?.to_string(),
            entry: usize::try_from(v.get("entry")?.as_u64()?).ok()?,
            option: v.get("option")?.as_str()?.to_string(),
        })
    }
}

/// A question's title as a person reads it. An MCP tool's comes as
/// `mcp__server__tool_name`: that reads as "tool name", from "server".
pub fn readable(title: &str) -> String {
    let Some(rest) = title.strip_prefix("mcp__") else { return title.to_string() };
    let Some((server, tool)) = rest.split_once("__") else { return title.to_string() };
    let words = |s: &str| s.split(['_', '-']).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    // claude.ai's own connectors are named `claude_ai_Slack` and the like.
    let server = server.strip_prefix("claude_ai_").unwrap_or(server);
    let server = if server == crate::mcp::SERVER_NAME { "Villain Layer".to_string() } else { words(server) };
    format!("{} ({server})", words(tool))
}

/// Slack's markup takes `&`, `<` and `>` as its own.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn outcome(asked: &Asked) -> Option<String> {
    let answer = asked.answer.as_deref()?;
    if answer == "cancelled" {
        return Some("_no longer asked_".into());
    }
    let choice = asked.options.iter().find(|o| o.id == answer);
    let name = choice.map_or(answer, |c| c.name.as_str());
    let mark = if choice.is_some_and(|c| c.kind.starts_with("reject")) { ":no_entry_sign:" } else { ":white_check_mark:" };
    Some(format!("{mark} {}", escaped(name)))
}

/// The card for a turn's questions in the chat in `room`: the text a
/// notification shows, and the blocks.
pub fn render(room: &str, asked: &[Asked]) -> (String, Value) {
    let waiting = asked.iter().filter(|a| a.answer.is_none()).count();
    let head = match waiting {
        0 => "Answered.".to_string(),
        1 => "One thing waits for your go-ahead.".to_string(),
        n => format!("{n} things wait for your go-ahead."),
    };
    let mut blocks = vec![json!({ "type": "section", "text": { "type": "mrkdwn", "text": format!(":raised_hand: *{head}*") } })];
    for a in asked.iter().take(SHOWN_MAX) {
        let title = format!("*{}*", escaped(&readable(&a.title)));
        match outcome(a) {
            Some(done) => blocks.push(json!({ "type": "section", "text": { "type": "mrkdwn", "text": format!("{title}  {done}") } })),
            None => {
                blocks.push(json!({ "type": "section", "text": { "type": "mrkdwn", "text": title } }));
                let buttons: Vec<Value> = a
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| {
                        let click = Click { room: room.to_string(), entry: a.entry, option: o.id.clone() };
                        let mut b = json!({
                            "type": "button",
                            "action_id": format!("ask-{}-{i}", a.entry),
                            "text": { "type": "plain_text", "text": clip(&o.name, 70) },
                            "value": click.value(),
                        });
                        if o.kind == "allow_once" {
                            b["style"] = json!("primary");
                        } else if o.kind.starts_with("reject") {
                            b["style"] = json!("danger");
                        }
                        b
                    })
                    .collect();
                if !buttons.is_empty() {
                    blocks.push(json!({ "type": "actions", "elements": buttons }));
                }
            }
        }
    }
    if asked.len() > SHOWN_MAX {
        let more = asked.len() - SHOWN_MAX;
        blocks.push(context(&format!("…and {more} more, in Villain Layer.")));
    }
    if waiting > 0 {
        blocks.push(context("Answer here, or in the chat in Villain Layer."));
    }
    let text = format!("{head} {}", asked.iter().map(|a| readable(&a.title)).collect::<Vec<_>>().join(", "));
    (clip(&text, 300), Value::Array(blocks))
}

fn context(text: &str) -> Value {
    json!({ "type": "context", "elements": [{ "type": "mrkdwn", "text": text }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(id: &str, name: &str, kind: &str) -> Choice {
        Choice { id: id.into(), name: name.into(), kind: kind.into() }
    }

    fn asked(entry: usize, title: &str, answer: Option<&str>) -> Asked {
        Asked {
            entry,
            title: title.into(),
            options: vec![choice("allow", "Allow", "allow_once"), choice("always", "Always allow", "allow_always"), choice("deny", "Reject", "reject_once")],
            answer: answer.map(str::to_string),
        }
    }

    #[test]
    fn a_tools_name_reads_as_words_and_where_it_comes_from() {
        assert_eq!(readable("mcp__villain-layer__browser_navigate"), "browser navigate (Villain Layer)");
        assert_eq!(readable("mcp__claude_ai_Slack__slack_search_public_and_private"), "slack search public and private (Slack)");
        assert_eq!(readable("Edit src/main.rs"), "Edit src/main.rs");
        assert_eq!(readable("mcp__odd"), "mcp__odd");
    }

    #[test]
    fn a_waiting_question_has_a_button_per_answer_that_names_its_chat() {
        let (text, blocks) = render("/chat/a", &[asked(4, "mcp__villain-layer__grafana_labels", None)]);
        assert!(text.starts_with("One thing waits"), "{text}");
        let buttons = blocks.as_array().unwrap().iter().find(|b| b["type"] == "actions").unwrap()["elements"].as_array().unwrap().clone();
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[0]["style"], "primary");
        assert_eq!(buttons[2]["style"], "danger");
        let click = Click::read(buttons[1]["value"].as_str().unwrap()).unwrap();
        assert_eq!(click, Click { room: "/chat/a".into(), entry: 4, option: "always".into() });
    }

    #[test]
    fn an_answered_question_says_how_and_loses_its_buttons() {
        let (text, blocks) = render("/chat/a", &[asked(1, "Run ls", Some("deny")), asked(2, "Edit <x> & y", Some("cancelled"))]);
        assert!(text.starts_with("Answered."), "{text}");
        let all = blocks.to_string();
        assert!(!all.contains("\"actions\""), "{all}");
        assert!(all.contains(":no_entry_sign: Reject"));
        assert!(all.contains("Edit &lt;x&gt; &amp; y"));
        assert!(all.contains("no longer asked"));
    }

    #[test]
    fn a_click_from_anything_else_is_not_read_as_one() {
        assert_eq!(Click::read("not json"), None);
        assert_eq!(Click::read(r#"{"room":"/chat/a","entry":-1,"option":"x"}"#), None);
    }
}
