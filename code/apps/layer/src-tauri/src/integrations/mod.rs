pub mod aws;
pub mod github;
pub mod grafana;
pub mod jira;
pub mod slack;
pub mod slack_bot;

use crate::error::{Error, Result};

/// Every integration client, built the same way.
///
/// The timeout matters: without one, a host that accepts the connection and
/// then goes quiet — a VPN-only GitHub Enterprise, a hung proxy — leaves the
/// command awaiting forever, which the UI shows as a spinner that never
/// resolves and an MCP tool call that never returns.
///
/// One client for the whole process, handed out as clones that share its
/// connection pool. A fresh one per command meant every poll, every ticket
/// opened and every PR sweep paid for a new TCP and TLS handshake — a
/// noticeable part of each call to a site across an ocean.
pub fn http_client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .user_agent("villain-layer")
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// A string field of a JSON object, or "" when it is missing or not a string.
pub(crate) fn str_at(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Refuse a site whose address would carry a token unencrypted (SET-3).
///
/// Both URLs are typed by the user, and nothing checked them: an `http://`
/// Jira sent `Basic email:token` in the clear on every request, and an
/// `http://` GitHub Enterprise its bearer token. Plain http is allowed only
/// to this machine, where nothing leaves it: a local proxy or test server.
pub fn require_https(url: &str, what: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| Error::Other(format!("{what} is not a web address: {url}")))?;
    let host = parsed.host_str().unwrap_or("").trim_start_matches('[').trim_end_matches(']');
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        _ => Err(Error::Other(format!(
            "{what} has to start with https://. Over {}:// the token would travel unencrypted.",
            parsed.scheme()
        ))),
    }
}

/// What the app posts to Slack, with each home folder on this Mac
/// (`/Users/<name>`) written `~` (BOT-18). The bot told a team channel it
/// worked "under `/Users/you/`": the owner's account name and where
/// their work sits are nobody else's business, and an agent told so still
/// slips. A path inside a web address (`https://x.test/Users/a`) is left.
pub(crate) fn hide_home(text: &str) -> String {
    const USERS: &str = "/users/";
    // ASCII lowercasing keeps every byte where it was.
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    let mut search = 0;
    while let Some(found) = lower[search..].find(USERS) {
        let at = search + found;
        let name_at = at + USERS.len();
        let name_len = text[name_at..]
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-')))
            .unwrap_or(text.len() - name_at);
        let name = &text[name_at..name_at + name_len];
        let starts_path = text[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '~')));
        if starts_path && !name.is_empty() && !name.eq_ignore_ascii_case("shared") {
            out.push_str(&text[from..at]);
            out.push('~');
            from = name_at + name_len;
        }
        search = name_at + name_len;
    }
    out.push_str(&text[from..]);
    out
}

/// `hide_home` on every string of a Slack API call's body: its text, its
/// blocks, and anything a block nests.
pub(crate) fn hide_home_in(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => *s = hide_home(s),
        serde_json::Value::Array(items) => items.iter_mut().for_each(hide_home_in),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(hide_home_in),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_folder_on_this_mac_never_reaches_slack() {
        assert_eq!(hide_home("I work under `/Users/you/`."), "I work under `~/`.");
        assert_eq!(hide_home("/Users/you/code/a.rs and /users/ana.b"), "~/code/a.rs and ~");
        assert_eq!(hide_home("file:///Users/you/x (/Users/you)"), "file://~/x (~)");
        assert_eq!(hide_home("https://x.test/Users/you stays"), "https://x.test/Users/you stays");
        assert_eq!(hide_home("/Users/Shared/cache and /Users/ alone"), "/Users/Shared/cache and /Users/ alone");
        assert_eq!(hide_home("naïve /Users/zoë/ü"), "naïve ~/ü");
        let mut body = serde_json::json!({ "text": "in /Users/you", "blocks": [{ "text": "/Users/you/a" }], "n": 1 });
        hide_home_in(&mut body);
        assert_eq!(body, serde_json::json!({ "text": "in ~", "blocks": [{ "text": "~/a" }], "n": 1 }));
    }

    #[test]
    fn a_token_goes_only_to_an_https_site_or_to_this_machine() {
        assert!(require_https("https://acme.atlassian.net", "Jira").is_ok());
        assert!(require_https("https://ghe.acme.test/api/v3", "GitHub").is_ok());
        assert!(require_https("http://localhost:8080", "Jira").is_ok());
        assert!(require_https("http://127.0.0.1:9", "Jira").is_ok());
        assert!(require_https("http://[::1]:9/api/v3", "GitHub").is_ok());

        assert!(require_https("http://acme.atlassian.net", "Jira").is_err());
        assert!(require_https("http://localhost.acme.test", "Jira").is_err(), "a name, not this machine");
        assert!(require_https("http://10.0.0.5/api/v3", "GitHub").is_err(), "a private network is still a network");
        assert!(require_https("ftp://acme.test", "Jira").is_err());
        assert!(require_https("acme.atlassian.net", "Jira").is_err());
    }
}
