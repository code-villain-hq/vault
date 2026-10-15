//! Slack's Web API as the bot (§24) uses it: who the bot is, where its
//! Socket Mode connection goes, a thread's messages, names, posting and
//! reacting. The bot token (`xoxb-`) does all of it but the one call that
//! opens the socket, which takes the app-level token (`xapp-`).

use serde_json::{json, Value};

use super::{http_client, str_at};
use crate::error::{Error, Result};

/// The most of a thread an agent is given (BOT-3).
pub const THREAD_CAP: usize = 200;
/// What one reply message holds. Slack takes 12,000 characters of markdown
/// blocks in one message; this leaves room for the note on a cut reply.
pub const PART_MAX: usize = 11_000;
/// A reply longer than this many messages is cut (BOT-6): a runaway reply
/// should not fill somebody else's channel.
pub const PARTS_MAX: usize = 3;

#[derive(Clone)]
pub struct BotApi {
    token: String,
    client: reqwest::Client,
}

/// The bot, as Slack knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Me {
    /// Its member id, `U…`: what a mention of it looks like (`<@U…>`).
    pub user: String,
    /// Its handle, what people type after the @.
    pub name: String,
    /// The workspace's address, `https://acme.slack.com/`.
    pub team_url: String,
}

/// One message of a thread, as the agent is given it.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMessage {
    /// A member id, or empty for a bot or an integration.
    pub user: String,
    /// The name Slack gives a bot's or an integration's message.
    pub bot_name: Option<String>,
    pub text: String,
    pub ts: String,
}

impl BotApi {
    pub fn new(token: &str) -> Self {
        Self { token: token.trim().to_string(), client: http_client() }
    }

    /// A read: Slack's read methods take their arguments as a query, not as
    /// JSON.
    async fn get(&self, method: &str, query: &[(&str, &str)]) -> Result<Value> {
        let res = self
            .client
            .get(format!("https://slack.com/api/{method}"))
            .bearer_auth(&self.token)
            .query(query)
            .send()
            .await?;
        answer(method, res).await
    }

    /// Every post and change the bot makes goes through here, so nothing
    /// it says names a home folder on this Mac (BOT-18).
    async fn post(&self, method: &str, body: &Value) -> Result<Value> {
        let mut body = body.clone();
        super::hide_home_in(&mut body);
        let res = self
            .client
            .post(format!("https://slack.com/api/{method}"))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await?;
        answer(method, res).await
    }

    /// Who the token speaks for. Refuses a token that is not a bot's: a
    /// user token would read and post as the person.
    pub async fn whoami(&self) -> Result<Me> {
        let body = self.get("auth.test", &[]).await?;
        if body.get("bot_id").and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(Error::Other(
                "that is not a bot token. Copy the Bot User OAuth Token (xoxb-…) from OAuth & Permissions.".into(),
            ));
        }
        Ok(Me {
            user: str_at(&body, "user_id"),
            name: str_at(&body, "user"),
            team_url: str_at(&body, "url"),
        })
    }

    /// Where the Socket Mode connection goes this time: each address is good
    /// for one connection. Takes the app-level token, not the bot's.
    pub async fn socket_url(app_token: &str) -> Result<String> {
        let res = http_client()
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(app_token.trim())
            .send()
            .await?;
        let body = answer("apps.connections.open", res).await?;
        let url = str_at(&body, "url");
        if !url.starts_with("wss://") {
            return Err(Error::Other("Slack gave no Socket Mode address".into()));
        }
        Ok(url)
    }

    /// A thread's messages, oldest first, at most `cap`, and whether there
    /// were more. With `oldest`, the ones after it (and Slack adds the
    /// thread's first message). None when Slack has no such message.
    pub async fn replies(&self, channel: &str, thread: &str, oldest: &str, cap: usize) -> Result<Option<(Vec<ThreadMessage>, bool)>> {
        let mut out = Vec::new();
        let mut cursor = String::new();
        loop {
            let mut query = vec![("channel", channel), ("ts", thread), ("limit", "200")];
            if !oldest.is_empty() {
                query.push(("oldest", oldest));
            }
            if !cursor.is_empty() {
                query.push(("cursor", cursor.as_str()));
            }
            let body = match self.get("conversations.replies", &query).await {
                Ok(body) => body,
                Err(Error::NotFound(_)) => return Ok(None),
                Err(e) => return Err(e),
            };
            for m in body.get("messages").and_then(Value::as_array).into_iter().flatten() {
                if out.len() == cap {
                    return Ok(Some((out, true)));
                }
                out.push(ThreadMessage {
                    user: str_at(m, "user"),
                    bot_name: m.get("username").and_then(Value::as_str).map(str::to_string),
                    text: str_at(m, "text"),
                    ts: str_at(m, "ts"),
                });
            }
            cursor = body
                .pointer("/response_metadata/next_cursor")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if cursor.is_empty() {
                return Ok(Some((out, false)));
            }
        }
    }

    /// A member's name as people see it, the display name they chose or
    /// their full name, and whether it is a bot or an app.
    pub async fn member(&self, user: &str) -> Result<(String, bool)> {
        let body = self.get("users.info", &[("user", user)]).await?;
        let bot = body.pointer("/user/is_bot").and_then(Value::as_bool) == Some(true);
        let profile = body.pointer("/user/profile");
        let pick = |key: &str| {
            profile
                .and_then(|p| p.get(key))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        };
        let name = pick("display_name")
            .or_else(|| pick("real_name"))
            .unwrap_or_else(|| str_at(&body.get("user").cloned().unwrap_or_default(), "name"));
        Ok((name, bot))
    }

    /// `#name` for a channel, or None for a direct message.
    pub async fn channel_label(&self, channel: &str) -> Result<Option<String>> {
        let body = self.get("conversations.info", &[("channel", channel)]).await?;
        let c = body.get("channel").cloned().unwrap_or_default();
        if c.get("is_im").and_then(Value::as_bool) == Some(true) {
            return Ok(None);
        }
        Ok(Some(str_at(&c, "name")).filter(|n| !n.is_empty()).map(|n| format!("#{n}")))
    }

    pub async fn permalink(&self, channel: &str, ts: &str) -> Result<String> {
        let body = self.get("chat.getPermalink", &[("channel", channel), ("message_ts", ts)]).await?;
        Ok(str_at(&body, "permalink"))
    }

    /// Post into a thread. Markdown goes in Slack's `markdown` block, which
    /// reads what agents write (`**bold**`, `[a](b)`, tables) as they mean
    /// it. `mrkdwn` would show half of it as punctuation.
    pub async fn reply(&self, channel: &str, thread: &str, markdown: &str) -> Result<()> {
        let body = json!({
            "channel": channel,
            "thread_ts": thread,
            // What a notification and a screen reader show.
            "text": clip(markdown, 300),
            "blocks": [{ "type": "markdown", "text": markdown }],
            "unfurl_links": false,
        });
        match self.post("chat.postMessage", &body).await {
            // A workspace that does not take the block yet still gets the
            // words, in Slack's own markup.
            Err(Error::Other(e)) if e.contains("invalid_blocks") => {
                let plain = json!({ "channel": channel, "thread_ts": thread, "text": markdown, "unfurl_links": false });
                self.post("chat.postMessage", &plain).await.map(drop)
            }
            other => other.map(drop),
        }
    }

    /// Post Block Kit `blocks` into a thread, with `shown` for notifications
    /// and screen readers. Returns the message's timestamp, to change it by.
    pub async fn post_blocks(&self, channel: &str, thread: &str, shown: &str, blocks: &Value) -> Result<String> {
        let body = json!({ "channel": channel, "thread_ts": thread, "text": shown, "blocks": blocks, "unfurl_links": false });
        let answer = self.post("chat.postMessage", &body).await?;
        Ok(str_at(&answer, "ts"))
    }

    /// Change a message the bot posted.
    pub async fn update_blocks(&self, channel: &str, ts: &str, shown: &str, blocks: &Value) -> Result<()> {
        let body = json!({ "channel": channel, "ts": ts, "text": shown, "blocks": blocks });
        self.post("chat.update", &body).await.map(drop)
    }

    /// A message only `user` sees, in a thread.
    pub async fn ephemeral(&self, channel: &str, user: &str, thread: &str, shown: &str) -> Result<()> {
        let mut body = json!({ "channel": channel, "user": user, "text": shown });
        if !thread.is_empty() {
            body["thread_ts"] = json!(thread);
        }
        self.post("chat.postEphemeral", &body).await.map(drop)
    }

    /// Mark a message with an emoji, by its name without colons.
    pub async fn react(&self, channel: &str, ts: &str, name: &str) -> Result<()> {
        let body = json!({ "channel": channel, "timestamp": ts, "name": name });
        match self.post("reactions.add", &body).await {
            Err(Error::Other(e)) if e.contains("already_reacted") => Ok(()),
            other => other.map(drop),
        }
    }
}

/// `text` cut to `max` characters, with an ellipsis after if it was cut.
pub(crate) fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

/// Slack answers 200 with `ok: false` for most failures. Its codes are
/// terse, and the fix is rarely obvious from them.
async fn answer(method: &str, res: reqwest::Response) -> Result<Value> {
    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Error::Other(format!("Slack rate-limited {method}; try again shortly")));
    }
    let body: Value = res.json().await?;
    if body.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(body);
    }
    let code = body.get("error").and_then(Value::as_str).unwrap_or("unknown");
    Err(refused(method, code, body.get("needed").and_then(Value::as_str)))
}

fn refused(method: &str, code: &str, needed: Option<&str>) -> Error {
    // The message an event named is not there to read: gone already, or
    // one the bot may not see. A warning posted "under" it went to the top
    // of the channel, with nothing beside it to say what it was about.
    if code == "thread_not_found" {
        return Error::NotFound(format!("Slack has no such message for {method}"));
    }
    Error::Other(match code {
        "missing_scope" => format!(
            "the Slack app is missing the {} scope for {method}. Add it under OAuth & Permissions (or make the app again from the manifest in Settings) and reinstall it.",
            needed.unwrap_or("needed")
        ),
        "invalid_auth" | "not_authed" | "token_revoked" | "account_inactive" => {
            format!("Slack refused the token for {method} ({code}). Connect the bot again with fresh tokens.")
        }
        "not_allowed_token_type" if method == "apps.connections.open" => {
            "that is not an app-level token. Make one under Basic Information → App-Level Tokens, with the connections:write scope (it starts with xapp-).".into()
        }
        "not_in_channel" | "channel_not_found" => {
            format!("the bot cannot see that conversation ({code}). Invite it to the channel first.")
        }
        other => format!("Slack refused {method}: {other}"),
    })
}

/// Text from Slack, as a person would read it: `<@U1>` is `@Ana`, a link
/// its words and address, and `&amp;` an ampersand. `name` knows the
/// members already looked up.
pub fn from_slack(text: &str, name: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let inner = &rest[open + 1..open + close];
        let (target, label) = match inner.split_once('|') {
            Some((t, l)) => (t, Some(l)),
            None => (inner, None),
        };
        match target.chars().next() {
            Some('@') => {
                let id = &target[1..];
                let shown = label.map(str::to_string).or_else(|| name(id)).unwrap_or_else(|| id.to_string());
                out.push('@');
                out.push_str(&shown);
            }
            Some('#') => {
                out.push('#');
                out.push_str(label.unwrap_or(&target[1..]));
            }
            // <!here>, <!channel>, <!subteam^S1|@team>
            Some('!') => match label {
                Some(l) => out.push_str(l),
                None => {
                    out.push('@');
                    out.push_str(&target[1..]);
                }
            },
            _ => match label {
                Some(l) if l != target => out.push_str(&format!("{l} ({target})")),
                _ => out.push_str(target),
            },
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// A reply cut into messages Slack will take (BOT-6): at a paragraph, or
/// else a line, near `max`; a code block cut in two is closed and opened
/// again. Past `parts`, the last one says the rest is in the app.
pub fn split_reply(text: &str, max: usize, parts: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text.trim();
    let mut fence_open = false;
    while !rest.is_empty() {
        if out.len() + 1 == parts && rest.chars().count() > max {
            let mut last: String = rest.chars().take(max.saturating_sub(80)).collect();
            if fence_open != (last.matches("```").count() % 2 == 1) {
                last.push_str("\n```");
            }
            let start = if fence_open { "```\n" } else { "" };
            out.push(format!("{start}{last}\n\n_…the rest is in the chat in Villain Layer._"));
            break;
        }
        let cut = if rest.chars().count() <= max {
            rest.len()
        } else {
            let window_end = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
            let window = &rest[..window_end];
            window
                .rfind("\n\n")
                .or_else(|| window.rfind('\n'))
                .filter(|&i| i > window.len() / 2)
                .unwrap_or(window_end)
        };
        let mut part = String::new();
        if fence_open {
            part.push_str("```\n");
        }
        let piece = rest[..cut].trim_end();
        part.push_str(piece);
        if piece.matches("```").count() % 2 == 1 {
            fence_open = !fence_open;
        }
        if fence_open {
            part.push_str("\n```");
        }
        out.push(part);
        rest = rest[cut..].trim_start_matches('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slack_markup_reads_as_a_person_would_read_it() {
        let names = |id: &str| (id == "U1").then(|| "Ana".to_string());
        assert_eq!(
            from_slack("<@U1> look at <https://x.test/a|the log> in <#C9|ops> &amp; <@U2>", names),
            "@Ana look at the log (https://x.test/a) in #ops & @U2"
        );
        assert_eq!(from_slack("<!here> a &lt;b&gt; <https://x.test>", names), "@here a <b> https://x.test");
        assert_eq!(from_slack("unclosed <oops", names), "unclosed <oops");
    }

    #[test]
    fn a_message_slack_cannot_find_is_not_found_rather_than_refused() {
        assert!(matches!(refused("conversations.replies", "thread_not_found", None), Error::NotFound(_)));
        let other = refused("conversations.replies", "ratelimited", None).to_string();
        assert_eq!(other, "Slack refused conversations.replies: ratelimited");
    }

    #[test]
    fn a_short_reply_is_one_message() {
        assert_eq!(split_reply("hello\n", 100, 3), vec!["hello".to_string()]);
        assert!(split_reply("  \n", 100, 3).is_empty());
    }

    #[test]
    fn a_long_reply_is_split_at_paragraphs_and_cut_after_the_last_part() {
        let para = "word ".repeat(15).trim().to_string();
        let text = vec![para.clone(); 10].join("\n\n");
        let parts = split_reply(&text, 200, 3);
        assert_eq!(parts.len(), 3);
        assert!(parts[0].len() <= 200 && parts[0].ends_with("word"), "{:?}", parts[0]);
        assert!(parts[2].contains("the rest is in the chat"));
        let short = split_reply(&vec![para; 3].join("\n\n"), 200, 3);
        assert_eq!(short.len(), 2, "{short:?}");
        assert!(!short.concat().contains("the rest is"));
    }

    #[test]
    fn a_code_block_split_in_two_is_closed_and_opened_again() {
        let code = format!("```\n{}\n```", "let x = 1;\n".repeat(40));
        let parts = split_reply(&code, 200, 5);
        assert!(parts.len() > 1);
        for p in &parts {
            assert_eq!(p.matches("```").count() % 2, 0, "{p}");
        }
    }
}
