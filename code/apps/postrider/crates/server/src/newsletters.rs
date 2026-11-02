//! Newsletters: mail with a link to unsubscribe (`List-Unsubscribe`) that
//! isn't a notice ([`is_notice`]) skips the Inbox for a folder of its own,
//! by a rule on the account's server ([`When::Unsubscribe`], written after
//! yours, so a rule of yours still wins). Postrider writes that rule once
//! for each account, after its first sync, and moves the Inbox's
//! newsletters with it ([`Mail::set_newsletters_aside`]); a rule you take
//! away stays away.
//!
//! A notice is mail about something of yours: a sign-in, your account, an
//! order, a payment, a delivery. Google's say where to unsubscribe as
//! newsletters do, so the link alone doesn't tell them apart. A machine
//! that says it sent the mail by itself (`Auto-Submitted`, RFC 3834), but
//! not to many (newsletters say so too, to keep auto-replies away, and
//! then offer one click to leave or say `Precedence: bulk`), or an address
//! that names what it's about (`noreply-accounts@`, `auto-confirm@`,
//! `accounts.google.com`), sends notices; they stay in the Inbox. Keep in
//! Inbox is there for the senders this still gets wrong.
//!
//! `GET /api/w/{workspace}/newsletters` reads each account's Newsletters
//! of the last [`DAYS`] days, by sender: how often each writes, and how much
//! of it you opened (`\Seen`), the least read first.
//!
//! Unsubscribing asks nothing. It archives what the sender sent at once,
//! and sends the request [`WAIT`] later, so Undo really undoes it: the
//! one-click POST of RFC 8058, else a mail to its `mailto:` over the
//! account's SMTP. A sender with only a page to unsubscribe on gets its page
//! opened in the browser. Keep in Inbox writes a rule that keeps a sender's
//! mail there, ahead of the newsletters' rule, and moves its mail back.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use futures::future::join_all;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use mail_parser::MessageParser;
use postrider_domain::{
    AccountState, AccountStatus, FolderRole, KeptInInbox, Listing, MailAccount, Newsletter,
    Newsletters, SenderProblem, Senders, SettingAside, UnsubscribeSent, Unsubscribed,
    Unsubscribing, Validate,
};
use reqwest::Url;
use serde::Deserialize;
use tokio::sync::watch;
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::folders::listing;
use crate::imap::{self, Failure, Headed, Place};
use crate::mail::Mail;
use crate::reply::{is_this_mac, send};
use crate::rules::{self, Moved, Rule, When};
use crate::state::AppState;

/// How far back the page reads.
const DAYS: i64 = 90;

/// What the page counts as lately: "They sent you … in the last month."
const RECENT: i64 = 30;

/// How long a request waits before it goes, for Undo.
const WAIT: Duration = Duration::from_secs(10);

/// How long a sender's server may take to answer one.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The header fields a newsletter is read by.
const FIELDS: &str = "FROM LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST";

/// The header fields a notice is told by.
const NOTICE_FIELDS: &str = "FROM AUTO-SUBMITTED LIST-UNSUBSCRIBE-POST PRECEDENCE";

/// What a notice's address says it's about, in English, German or Swedish:
/// your account and signing in; an order, its bill and its payment; its
/// delivery; a booking, or a confirmation of anything; a message someone
/// wrote you, as LinkedIn's messaging-digest-noreply@ says.
const NOTICE_WORDS: &[&str] = &[
    "account",
    "konto",
    "security",
    "sicherheit",
    "sakerhet",
    "verif",
    "password",
    "passwort",
    "signin",
    "sign-in",
    "login",
    "order",
    "bestell",
    "receipt",
    "kvitto",
    "invoice",
    "rechnung",
    "faktura",
    "billing",
    "payment",
    "zahlung",
    "betalning",
    "shipping",
    "shipment",
    "versand",
    "delivery",
    "lieferung",
    "leverans",
    "tracking",
    "booking",
    "buchung",
    "confirm",
    "bestaetig",
    "bekraftelse",
    "messag",
    "inmail",
];

/// Senders' names that say nothing of who sends, written plain: the page
/// names those senders by their organisation.
const GENERIC_NAMES: &[&str] = &[
    "notification",
    "notifications",
    "noreply",
    "donotreply",
    "newsletter",
    "news",
    "info",
    "team",
    "theteam",
    "hello",
    "hi",
    "updates",
    "update",
    "marketing",
    "mailer",
    "support",
    "service",
    "contact",
    "mail",
    "email",
    "admin",
];

/// What a domain may have between its organisation and its top-level
/// domain: `co` in `amazon.co.uk`.
const SECOND_LEVEL: &[&str] = &["ac", "co", "com", "edu", "gov", "ne", "net", "or", "org"];

/// What `Precedence` says of mail sent to many.
const BULK: &[&str] = &["bulk", "list"];

/// The first part of a host that sends only notices: accounts.google.com,
/// id.apple.com.
const NOTICE_HOSTS: &[&str] = &[
    "account", "accounts", "auth", "id", "login", "security", "signin",
];

/// What unsubscribing and keeping in the Inbox changed, for Undo, by the id
/// the page got. It lasts until the server stops.
pub type Undos = Arc<Mutex<HashMap<Uuid, Undo>>>;

pub struct Undo {
    workspace_id: Uuid,
    /// Each account's rules before, to write back.
    rules: Vec<(MailAccount, Vec<Rule>)>,
    /// The mail moved, to move back.
    moved: Vec<(MailAccount, Moved)>,
    /// The requests, while they wait to go.
    requests: Option<AbortHandle>,
    /// How they went, once they did: those that failed, and why.
    sent: watch::Receiver<Option<Vec<SenderProblem>>>,
}

impl Undo {
    fn new(workspace_id: Uuid) -> Self {
        Undo {
            workspace_id,
            rules: Vec::new(),
            moved: Vec::new(),
            requests: None,
            sent: watch::channel(Some(Vec::new())).1,
        }
    }
}

/// How a sender takes unsubscribing.
#[derive(Debug, Clone, PartialEq)]
enum Way {
    /// A POST to this address, as RFC 8058 has it: no page, no question.
    OneClick(Url),
    /// A mail to this `mailto:` address.
    Mail(Url),
    /// A page to open, where you unsubscribe yourself.
    Page(Url),
}

/// A newsletter, as its header says.
#[derive(Debug, Clone)]
struct Sent {
    uid: u32,
    arrived: DateTime<Utc>,
    seen: bool,
    name: String,
    address: String,
    way: Option<Way>,
}

/// What one sender sent to an account's Newsletters, oldest first, and how
/// its newest mail that says one takes unsubscribing.
#[derive(Debug, Default)]
struct Theirs {
    name: String,
    uids: Vec<u32>,
    way: Option<Way>,
}

impl Mail {
    /// Sets the account's newsletters aside, once: writes their rule and
    /// moves the Inbox's newsletters to Newsletters, but those from senders
    /// kept in the Inbox.
    pub(crate) async fn set_newsletters_aside(&self, account: &MailAccount) -> Result<(), Failure> {
        let done: Option<(bool, bool)> = sqlx::query_as(
            "SELECT notices_back_at IS NOT NULL, bulk_aside_at IS NOT NULL \
             FROM newsletters_set_aside WHERE account_id = $1",
        )
        .bind(account.id)
        .fetch_optional(&self.pool)
        .await?;
        match done {
            Some((true, true)) => return Ok(()),
            Some((notices_back, _)) => return self.sort_again(account, notices_back).await,
            None => {}
        }
        let place = self.folder_for(account, FolderRole::Newsletters).await?;
        let mut rules = self.rules(account).await?;
        if !rules.iter().any(Rule::is_newsletters) {
            rules.push(Rule::newsletters(&place));
            self.put_rules(account, &rules).await?;
        }
        let query = unsubscribe_search(&rules, "");
        let headed = self
            .on_server(account, |session| {
                let query = query.clone();
                Box::pin(
                    async move { imap::headers(session, "INBOX", &query, NOTICE_FIELDS).await },
                )
            })
            .await?;
        let uids: Vec<u32> = headed
            .iter()
            .filter(|mail| !notice(mail))
            .map(|mail| mail.uid)
            .collect();
        let moved = self
            .move_out(account, "INBOX", &uids, &[], &place.path)
            .await?;
        sqlx::query(
            "INSERT INTO newsletters_set_aside (account_id, notices_back_at, bulk_aside_at) \
             VALUES ($1, now(), now()) ON CONFLICT DO NOTHING",
        )
        .bind(account.id)
        .execute(&self.pool)
        .await?;
        tracing::info!(address = %account.address, "set {} newsletters aside", moved.count);
        Ok(())
    }

    /// For an account set aside before Postrider told mail apart as it does
    /// now, once: writes the rule again, so that it tells them apart too.
    /// Set aside when every mail with a link to unsubscribe was a newsletter
    /// (`notices_back` not yet), it moves the notices back to the Inbox;
    /// else it sets aside the newsletters it took for notices, since they
    /// said a machine sent them.
    async fn sort_again(&self, account: &MailAccount, notices_back: bool) -> Result<(), Failure> {
        let rules = self.rules(account).await?;
        let place = self.newsletters_folder(account).await?;
        if rules.iter().any(Rule::is_newsletters) {
            self.put_rules(account, &rules).await?;
            if notices_back && let Some(place) = &place {
                let query = unsubscribe_search(&rules, "HEADER Auto-Submitted \"\" ");
                let headed = self
                    .on_server(account, |session| {
                        let query = query.clone();
                        Box::pin(async move {
                            imap::headers(session, "INBOX", &query, NOTICE_FIELDS).await
                        })
                    })
                    .await?;
                let uids: Vec<u32> = headed
                    .iter()
                    .filter(|mail| !notice(mail))
                    .map(|mail| mail.uid)
                    .collect();
                let moved = self
                    .move_out(account, "INBOX", &uids, &[], &place.path)
                    .await?;
                tracing::info!(address = %account.address, "set {} newsletters aside again", moved.count);
            }
        }
        if !notices_back && let Some(place) = place {
            let query = rules::search(&[When::Unsubscribe], "");
            let headed = self
                .on_server(account, |session| {
                    let (path, query) = (place.path.clone(), query.clone());
                    Box::pin(
                        async move { imap::headers(session, &path, &query, NOTICE_FIELDS).await },
                    )
                })
                .await?;
            let uids: Vec<u32> = headed
                .iter()
                .filter(|mail| notice(mail))
                .map(|mail| mail.uid)
                .collect();
            let moved = self
                .move_out(account, &place.path, &uids, &[], "INBOX")
                .await?;
            tracing::info!(address = %account.address, "moved {} notices back", moved.count);
        }
        sqlx::query(
            "UPDATE newsletters_set_aside \
             SET notices_back_at = coalesce(notices_back_at, now()), bulk_aside_at = now() \
             WHERE account_id = $1",
        )
        .bind(account.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The account's Newsletters, if it has one.
    async fn newsletters_folder(&self, account: &MailAccount) -> Result<Option<Place>, Failure> {
        Ok(self
            .places(account)
            .await?
            .into_iter()
            .find(|place| place.role == Some(FolderRole::Newsletters)))
    }

    /// A sender's newest mail in Newsletters, from every account it writes
    /// to.
    pub async fn sender_mail(
        &self,
        workspace_id: Uuid,
        address: &str,
    ) -> Result<Listing, AppError> {
        let accounts = self.accounts(workspace_id).await?;
        let query = &imap::from_query(address);
        let read = join_all(accounts.iter().map(|account| async move {
            match self.newsletters_folder(account).await? {
                Some(place) => self.read(account, &place, Some(query)).await,
                None => Ok(Vec::new()),
            }
        }))
        .await;
        Ok(listing(&accounts, read, HashMap::new()))
    }

    /// The account's newsletters of the last [`DAYS`] days, by sender.
    async fn newsletters_of(
        &self,
        account: &MailAccount,
        now: DateTime<Utc>,
    ) -> Result<Vec<Newsletter>, Failure> {
        let Some(place) = self.newsletters_folder(account).await? else {
            return Ok(Vec::new());
        };
        let query = format!(
            "SINCE {}",
            (now - TimeDelta::days(DAYS)).format("%-d-%b-%Y")
        );
        let headed = self
            .on_server(account, |session| {
                let (path, query) = (place.path.clone(), query.clone());
                Box::pin(async move { imap::headers(session, &path, &query, FIELDS).await })
            })
            .await?;
        let local = is_this_mac(&account.host);
        let sent = headed.into_iter().filter_map(|h| read(h, local)).collect();
        Ok(by_sender(account.id, sent, now))
    }

    /// What these senders sent to the account's Newsletters, by address.
    async fn theirs(
        &self,
        account: &MailAccount,
        place: &Place,
        addresses: &[String],
    ) -> Result<HashMap<String, Theirs>, Failure> {
        let when: Vec<When> = addresses.iter().cloned().map(When::From).collect();
        let query = rules::search(&when, "");
        let headed = self
            .on_server(account, |session| {
                let (path, query) = (place.path.clone(), query.clone());
                Box::pin(async move { imap::headers(session, &path, &query, FIELDS).await })
            })
            .await?;
        let local = is_this_mac(&account.host);
        // FROM finds the address inside others too.
        let mut sent: Vec<Sent> = headed
            .into_iter()
            .filter_map(|h| read(h, local))
            .filter(|mail| addresses.contains(&mail.address))
            .collect();
        sent.sort_by_key(|mail| (mail.arrived, mail.uid));
        let mut theirs: HashMap<String, Theirs> = HashMap::new();
        for mail in sent {
            let sender = theirs.entry(mail.address).or_default();
            sender.uids.push(mail.uid);
            sender.name = mail.name;
            if mail.way.is_some() {
                sender.way = mail.way;
            }
        }
        Ok(theirs)
    }

    /// Asks a sender to stop: what went wrong, in a sentence, if it did.
    async fn request(
        &self,
        account: &MailAccount,
        user_id: Uuid,
        name: &str,
        way: &Way,
    ) -> Result<(), String> {
        match way {
            Way::OneClick(url) => {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(TIMEOUT)
                    .build()
                    .map_err(|e| format!("Couldn't ask {name} to stop: {e}."))?;
                let answer = client
                    .post(url.clone())
                    .header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body("List-Unsubscribe=One-Click")
                    .send()
                    .await
                    .map_err(|e| {
                        format!("Couldn't reach {name} to unsubscribe: {}.", e.without_url())
                    })?;
                let status = answer.status();
                if status.is_success() || status.is_redirection() {
                    Ok(())
                } else {
                    Err(format!(
                        "{name} didn't take the request to unsubscribe: it answered {status}."
                    ))
                }
            }
            Way::Mail(url) => {
                let to = percent_decoded(url.path());
                let to: Mailbox = to
                    .parse()
                    .map_err(|_| format!("{name}'s address to unsubscribe at isn't one: {to}."))?;
                let said = |key: &str| {
                    url.query_pairs()
                        .find(|(k, _)| k.eq_ignore_ascii_case(key))
                        .map(|(_, value)| value.into_owned())
                        .filter(|value| !value.trim().is_empty())
                };
                let from = self.sender(account, user_id).await.map_err(|e| e.message)?;
                let message = lettre::Message::builder()
                    .from(from)
                    .to(to)
                    .subject(said("subject").unwrap_or_else(|| "Unsubscribe".into()))
                    .header(ContentType::TEXT_PLAIN)
                    .body(said("body").unwrap_or_else(|| "Unsubscribe".into()))
                    .map_err(|e| format!("Couldn't write {name} the mail to unsubscribe: {e}."))?;
                let password = self.password(account).await.map_err(|f| f.message())?;
                send(account, password, message)
                    .await
                    .map_err(|e| format!("Couldn't unsubscribe from {name}: {}", e.message))
            }
            Way::Page(_) => Ok(()),
        }
    }
}

/// Whether mail from `address`, with this `Auto-Submitted`, is a notice:
/// sent by a machine by itself, and not to many (`bulk`), or from an
/// address that says what about.
pub(crate) fn is_notice(address: &str, auto_submitted: Option<&str>, bulk: bool) -> bool {
    let address = address.trim().to_lowercase();
    let (local, host) = address.rsplit_once('@').unwrap_or((&address, ""));
    (!bulk && auto_submitted.is_some_and(|said| !said.trim().eq_ignore_ascii_case("no")))
        || NOTICE_WORDS.iter().any(|word| local.contains(word))
        || host
            .split_once('.')
            .is_some_and(|(first, _)| NOTICE_HOSTS.contains(&first))
}

/// [`is_notice`] in Sieve, for the newsletters' rule.
pub(crate) fn notice_test() -> String {
    let strings = |items: &[&str], after: &str| {
        items
            .iter()
            .map(|item| format!("\"{item}{after}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "anyof (allof (exists \"auto-submitted\", not header :is \"auto-submitted\" \"no\", \
         not exists \"list-unsubscribe-post\", not header :is \"precedence\" [{}]), \
         address :localpart :contains \"from\" [{}], \
         address :domain :matches \"from\" [{}])",
        strings(BULK, ""),
        strings(NOTICE_WORDS, ""),
        strings(NOTICE_HOSTS, ".*")
    )
}

/// The Inbox's mail with a link to unsubscribe that `also` finds, but not
/// from the senders kept in the Inbox.
fn unsubscribe_search(rules: &[Rule], also: &str) -> String {
    let kept: Vec<When> = rules
        .iter()
        .filter(|rule| rule.path == "INBOX")
        .flat_map(|rule| rule.when.clone())
        .collect();
    if kept.is_empty() {
        rules::search(&[When::Unsubscribe], also)
    } else {
        rules::search(&kept, &format!("HEADER List-Unsubscribe \"\" {also}NOT "))
    }
}

/// Whether a mail, by its [`NOTICE_FIELDS`], is a notice.
fn notice(headed: &Headed) -> bool {
    let Some(message) = MessageParser::new().parse_headers(&headed.header) else {
        return false;
    };
    let Some(address) = message
        .from()
        .and_then(|list| list.first())
        .and_then(|from| from.address())
    else {
        return false;
    };
    let raw = |field: &str| {
        message
            .headers()
            .get(field)
            .map(|header| String::from_utf8_lossy(header.raw_value()).into_owned())
    };
    let bulk = raw("List-Unsubscribe-Post").is_some()
        || raw("Precedence")
            .is_some_and(|said| BULK.iter().any(|b| said.trim().eq_ignore_ascii_case(b)));
    is_notice(address, raw("Auto-Submitted").as_deref(), bulk)
}

/// A newsletter from its header fields; `None` without a sender.
fn read(headed: Headed, local: bool) -> Option<Sent> {
    let message = MessageParser::new().parse_headers(&headed.header)?;
    let from = message.from().and_then(|list| list.first());
    let address = from.as_ref()?.address()?.trim().to_lowercase();
    let name = sender_name(from.as_ref().and_then(|f| f.name()), &address);
    let raw = |field: &str| {
        message
            .headers()
            .get(field)
            .map(|header| String::from_utf8_lossy(header.raw_value()).into_owned())
    };
    let way = raw("List-Unsubscribe")
        .and_then(|links| way(&links, raw("List-Unsubscribe-Post").as_deref(), local));
    Some(Sent {
        uid: headed.uid,
        arrived: headed.arrived.unwrap_or_default(),
        seen: headed.seen,
        name,
        address,
        way,
    })
}

/// What the page calls a sender: the name it gives, unless it gives none
/// or one that says nothing ("Notification", "Info"), then its
/// organisation.
fn sender_name(name: Option<&str>, address: &str) -> String {
    let plain = |name: &str| {
        name.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase()
    };
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) if !GENERIC_NAMES.contains(&plain(name).as_str()) => name.to_string(),
        _ => organisation(address).unwrap_or_else(|| address.to_string()),
    }
}

/// The organisation a mail address belongs to, by the part of its domain
/// before the suffix: `weave.engineering` is Weave, `m.learn.coursera.org`
/// Coursera, `amazon.co.uk` Amazon.
fn organisation(address: &str) -> Option<String> {
    let host = address.rsplit_once('@')?.1.to_ascii_lowercase();
    let mut labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    labels.pop()?;
    if labels.len() > 1
        && labels
            .last()
            .is_some_and(|label| SECOND_LEVEL.contains(label))
    {
        labels.pop();
    }
    let mut chars = labels.pop()?.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// The best way `List-Unsubscribe` offers: one click, else a mail, else a
/// page. One click needs `https`, or `http` from this Mac's stand-ins.
fn way(links: &str, post: Option<&str>, local: bool) -> Option<Way> {
    let links: Vec<Url> = links
        .split('<')
        .skip(1)
        .filter_map(|part| part.split_once('>'))
        .filter_map(|(link, _)| Url::parse(&link.split_whitespace().collect::<String>()).ok())
        .collect();
    let one_click = post.is_some_and(|post| {
        post.to_ascii_lowercase()
            .contains("list-unsubscribe=one-click")
    });
    if one_click
        && let Some(url) = links.iter().find(|url| {
            (url.scheme() == "https" || (local && url.scheme() == "http")) && reachable(url, local)
        })
    {
        return Some(Way::OneClick(url.clone()));
    }
    if let Some(url) = links
        .iter()
        .find(|url| url.scheme() == "mailto" && !url.path().is_empty())
    {
        return Some(Way::Mail(url.clone()));
    }
    links
        .iter()
        .find(|url| matches!(url.scheme(), "https" | "http"))
        .map(|url| Way::Page(url.clone()))
}

/// Whether a request may go there: not to this Mac or a network of its
/// own, where no newsletter lives, but from the stand-ins on this Mac.
fn reachable(url: &Url, local: bool) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if local {
        return true;
    }
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            !(ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified())
        }
        Ok(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            !(ip.is_loopback()
                || ip.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80)
        }
        Err(_) => host != "localhost" && !host.ends_with(".localhost") && !host.ends_with(".local"),
    }
}

/// `%40` read as `@`, and the like.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Newsletters by sender: how much each sent, how much of it was opened,
/// and how often it writes.
fn by_sender(account_id: Uuid, sent: Vec<Sent>, now: DateTime<Utc>) -> Vec<Newsletter> {
    let mut senders: HashMap<String, Vec<Sent>> = HashMap::new();
    for mail in sent {
        senders.entry(mail.address.clone()).or_default().push(mail);
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    senders
        .into_iter()
        .filter_map(|(address, mails)| {
            let newest = mails.iter().max_by_key(|mail| (mail.arrived, mail.uid))?;
            let oldest = mails.iter().map(|mail| mail.arrived).min()?;
            // A week at least, or one mail yesterday reads as daily.
            let weeks = (now - oldest).num_minutes().max(7 * 24 * 60) / 60;
            let sent = count(mails.len());
            Some(Newsletter {
                account_id,
                name: newest.name.clone(),
                address,
                sent,
                opened: count(mails.iter().filter(|mail| mail.seen).count()),
                recent: count(
                    mails
                        .iter()
                        .filter(|mail| now - mail.arrived <= TimeDelta::days(RECENT))
                        .count(),
                ),
                per_week: f64::from(sent) * 168.0
                    / f64::from(u32::try_from(weeks).unwrap_or(u32::MAX)),
                newest_uid: i64::from(newest.uid),
                newest: newest.arrived,
            })
        })
        .collect()
}

/// The least read first; of those alike, who sends most.
fn least_read_first(newsletters: &mut [Newsletter]) {
    let read = |n: &Newsletter| f64::from(n.opened) / f64::from(n.sent.max(1));
    newsletters.sort_by(|a, b| {
        read(a)
            .total_cmp(&read(b))
            .then(b.sent.cmp(&a.sent))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// The senders by account, each account once.
async fn by_account(
    mail: &Mail,
    workspace_id: Uuid,
    senders: &Senders,
) -> Result<Vec<(MailAccount, Vec<String>)>, AppError> {
    let mut grouped: Vec<(MailAccount, Vec<String>)> = Vec::new();
    for sender in &senders.senders {
        let address = sender.address.trim().to_lowercase();
        match grouped
            .iter_mut()
            .find(|(account, _)| account.id == sender.account_id)
        {
            Some((_, addresses)) if addresses.contains(&address) => {}
            Some((_, addresses)) => addresses.push(address),
            None => grouped.push((
                mail.account(workspace_id, sender.account_id).await?,
                vec![address],
            )),
        }
    }
    Ok(grouped)
}

/// Puts back what unsubscribing or keeping changed: the rules, then the
/// mail.
async fn put_back(mail: &Mail, undo: &Undo) -> Result<(), Failure> {
    for (account, rules) in &undo.rules {
        mail.put_rules(account, rules).await?;
    }
    for (account, moved) in &undo.moved {
        mail.move_back(account, moved).await?;
    }
    Ok(())
}

/// `GET /api/w/{workspace}/newsletters`: every account's, the least read
/// first.
pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Newsletters>, AppError> {
    let accounts = state.mail.accounts(member.actor.workspace_id).await?;
    let now = Utc::now();
    let read = join_all(
        accounts
            .iter()
            .map(|account| state.mail.newsletters_of(account, now)),
    )
    .await;
    let set_aside: Vec<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM newsletters_set_aside WHERE account_id = ANY($1)",
    )
    .bind(accounts.iter().map(|a| a.id).collect::<Vec<_>>())
    .fetch_all(&state.mail.pool)
    .await?;
    let failed = state
        .mail
        .set_aside_failed
        .lock()
        .expect("never poisoned")
        .clone();
    let mut newsletters = Vec::new();
    let mut problems = Vec::new();
    let mut setting_aside = Vec::new();
    for (account, read) in accounts.iter().zip(read) {
        match read {
            Ok(found) => {
                newsletters.extend(found);
                if !set_aside.contains(&account.id) {
                    setting_aside.push(SettingAside {
                        account_id: account.id,
                        problem: failed.get(&account.id).cloned(),
                    });
                }
            }
            Err(failure) => problems.push(AccountStatus {
                account_id: account.id,
                state: match failure {
                    Failure::Offline(_) => AccountState::Offline,
                    Failure::Refused(_) => AccountState::Refused,
                },
                message: Some(failure.message()),
            }),
        }
    }
    least_read_first(&mut newsletters);
    Ok(Json(Newsletters {
        newsletters,
        problems,
        setting_aside,
    }))
}

#[derive(Deserialize)]
pub struct SenderParams {
    address: String,
}

/// `GET /api/w/{workspace}/newsletters/sender?address=`: what a sender
/// sent, newest first.
pub async fn sender(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<SenderParams>,
) -> Result<Json<Listing>, AppError> {
    Ok(Json(
        state
            .mail
            .sender_mail(member.actor.workspace_id, &params.address)
            .await?,
    ))
}

/// `POST /api/w/{workspace}/newsletters/unsubscribe`: archives what the
/// senders sent, and asks them to stop in [`WAIT`], unless Undo comes first.
pub async fn unsubscribe(
    State(state): State<AppState>,
    member: Member,
    Json(senders): Json<Senders>,
) -> Result<Json<Unsubscribed>, AppError> {
    senders.validate()?;
    let workspace_id = member.actor.workspace_id;
    let grouped = by_account(&state.mail, workspace_id, &senders).await?;
    let mut undo = Undo::new(workspace_id);
    let mut requests = Vec::new();
    let mut gone = Vec::new();
    let archived = async {
        for (account, addresses) in grouped {
            let place = state.mail.newsletters_folder(&account).await?;
            let mut theirs = match &place {
                Some(place) => state.mail.theirs(&account, place, &addresses).await?,
                None => HashMap::new(),
            };
            let uids: Vec<u32> = theirs.values().flat_map(|t| t.uids.clone()).collect();
            if let Some(place) = place.filter(|_| !uids.is_empty()) {
                let archive = state.mail.folder_for(&account, FolderRole::Archive).await?;
                let moved = state
                    .mail
                    .move_out(&account, &place.path, &uids, &[], &archive.path)
                    .await?;
                undo.moved.push((account.clone(), moved));
            }
            for address in addresses {
                let sender = theirs.remove(&address).unwrap_or_default();
                let mut gone_one = Unsubscribing {
                    account_id: account.id,
                    address: address.clone(),
                    archived: u32::try_from(sender.uids.len()).unwrap_or(u32::MAX),
                    asked: false,
                    page: None,
                };
                match sender.way {
                    Some(Way::Page(url)) => gone_one.page = Some(url.to_string()),
                    Some(way) => {
                        gone_one.asked = true;
                        requests.push((account.clone(), address, sender.name, way));
                    }
                    None => {}
                }
                gone.push(gone_one);
            }
        }
        Ok::<_, Failure>(())
    }
    .await;
    if let Err(failure) = archived {
        if let Err(again) = put_back(&state.mail, &undo).await {
            tracing::warn!(
                "couldn't put back what unsubscribing moved: {}",
                again.message()
            );
        }
        return Err(AppError::refused(failure.message()));
    }
    let waiting = !requests.is_empty();
    if waiting {
        let (tell, sent) = watch::channel(None);
        let mail = Arc::clone(&state.mail);
        let user_id = member.actor.user_id;
        let task = tokio::spawn(async move {
            tokio::time::sleep(WAIT).await;
            let went = join_all(requests.iter().map(|(account, address, name, way)| {
                let mail = &mail;
                async move {
                    let message = mail.request(account, user_id, name, way).await.err()?;
                    tracing::info!("unsubscribing: {message}");
                    Some(SenderProblem {
                        account_id: account.id,
                        address: address.clone(),
                        message,
                    })
                }
            }))
            .await;
            tell.send_replace(Some(went.into_iter().flatten().collect()));
        });
        undo.requests = Some(task.abort_handle());
        undo.sent = sent;
    }
    let id = Uuid::now_v7();
    state.newsletters.lock().unwrap().insert(id, undo);
    Ok(Json(Unsubscribed {
        id,
        senders: gone,
        waiting,
    }))
}

/// `GET /api/w/{workspace}/newsletters/unsubscribed/{id}`: how the requests
/// went, once they have.
pub async fn unsubscribed(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UnsubscribeSent>, AppError> {
    let mut sent = {
        let undos = state.newsletters.lock().unwrap();
        match undos.get(&id) {
            Some(undo) if undo.workspace_id == member.actor.workspace_id => undo.sent.clone(),
            _ => return Err(AppError::not_found("That's been undone.")),
        }
    };
    let went = tokio::time::timeout(WAIT + TIMEOUT * 3, sent.wait_for(Option::is_some)).await;
    match went {
        Ok(Ok(failed)) => Ok(Json(UnsubscribeSent {
            failed: failed.clone().unwrap_or_default(),
        })),
        Ok(Err(_)) => Err(AppError::not_found("That's been undone.")),
        Err(_) => Err(AppError::refused(
            "Unsubscribing is taking long. Look again in a moment.",
        )),
    }
}

/// `POST /api/w/{workspace}/newsletters/keep`: keeps the senders' mail in
/// the Inbox, by a rule on the server, and moves what they sent back there.
pub async fn keep(
    State(state): State<AppState>,
    member: Member,
    Json(senders): Json<Senders>,
) -> Result<Json<KeptInInbox>, AppError> {
    senders.validate()?;
    let workspace_id = member.actor.workspace_id;
    let grouped = by_account(&state.mail, workspace_id, &senders).await?;
    let mut undo = Undo::new(workspace_id);
    let kept = async {
        for (account, addresses) in grouped {
            let before = state.mail.rules(&account).await?;
            let mut rules = before.clone();
            let when: Vec<When> = addresses.iter().cloned().map(When::From).collect();
            match rules.iter_mut().find(|rule| rule.path == "INBOX") {
                Some(rule) => {
                    for w in when {
                        if !rule.when.iter().any(|had| had.same(&w)) {
                            rule.when.push(w);
                        }
                    }
                }
                None => rules.push(Rule {
                    folder: "Inbox".into(),
                    path: "INBOX".into(),
                    when,
                }),
            }
            state.mail.put_rules(&account, &rules).await?;
            undo.rules.push((account.clone(), before));
            if let Some(place) = state.mail.newsletters_folder(&account).await? {
                let theirs = state.mail.theirs(&account, &place, &addresses).await?;
                let uids: Vec<u32> = theirs.values().flat_map(|t| t.uids.clone()).collect();
                let moved = state
                    .mail
                    .move_out(&account, &place.path, &uids, &[], "INBOX")
                    .await?;
                undo.moved.push((account, moved));
            }
        }
        Ok::<_, Failure>(())
    }
    .await;
    if let Err(failure) = kept {
        if let Err(again) = put_back(&state.mail, &undo).await {
            tracing::warn!("couldn't put back what keeping moved: {}", again.message());
        }
        return Err(AppError::refused(failure.message()));
    }
    let moved = undo
        .moved
        .iter()
        .map(|(_, moved)| moved.count)
        .sum::<usize>();
    let id = Uuid::now_v7();
    state.newsletters.lock().unwrap().insert(id, undo);
    Ok(Json(KeptInInbox {
        id,
        moved: u32::try_from(moved).unwrap_or(u32::MAX),
    }))
}

/// `POST /api/w/{workspace}/newsletters/undo/{id}`: stops the requests that
/// haven't gone, moves the mail back, and writes the rules back as they
/// were.
pub async fn undo(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let undo = {
        let mut undos = state.newsletters.lock().unwrap();
        match undos.get(&id) {
            Some(undo) if undo.workspace_id == member.actor.workspace_id => undos.remove(&id),
            _ => None,
        }
    }
    .ok_or_else(|| AppError::not_found("There's nothing to undo here any more."))?;
    if let Some(requests) = &undo.requests {
        requests.abort();
    }
    match put_back(&state.mail, &undo).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(failure) => {
            state.newsletters.lock().unwrap().insert(id, undo);
            Err(AppError::refused(failure.message()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn notices_are_told_from_newsletters() {
        // Google's notices of a sign-in say where to unsubscribe.
        assert!(is_notice("noreply-accounts@google.com", None, false));
        assert!(is_notice("no-reply@accounts.google.com", None, false));
        assert!(is_notice("appleid@id.apple.com", None, false));
        assert!(is_notice("auto-confirm@amazon.de", None, false));
        assert!(is_notice("shipment-tracking@amazon.de", None, false));
        assert!(is_notice("Buchungsbestaetigung@bahn.de", None, false));
        assert!(is_notice("kvitto@coop.se", None, false));
        assert!(is_notice(
            "noreply@forge.test",
            Some(" auto-generated"),
            false
        ));
        assert!(is_notice(
            "messaging-digest-noreply@linkedin.com",
            None,
            false
        ));
        assert!(is_notice("messages-noreply@linkedin.com", None, false));
        // Their newsletters don't.
        assert!(!is_notice("googleplay-noreply@google.com", None, false));
        assert!(!is_notice("store-news@amazon.de", None, false));
        assert!(!is_notice("newsletter@coop.se", Some("no"), false));
        assert!(!is_notice("info@fitness24seven.test", None, false));
        assert!(!is_notice("news@identity.test", None, false));
        assert!(!is_notice("updates-noreply@linkedin.com", None, false));
        // netcup's advisories come from its newsletter: that stays one.
        assert!(!is_notice("newsletter@netcup.com", None, false));
        assert!(!is_notice("notification@weave.engineering", None, false));
        // Newsletters may say a machine sent them, to keep auto-replies away,
        // and then say they went to many.
        assert!(!is_notice(
            "no-reply@m.learn.coursera.org",
            Some("auto-generated"),
            true
        ));
        assert!(is_notice(
            "noreply-accounts@google.com",
            Some("auto-generated"),
            true
        ));
    }

    #[test]
    fn a_sender_that_says_nothing_is_named_by_its_organisation() {
        let named = |name, address| sender_name(name, address);
        assert_eq!(
            named(Some("Notification"), "notification@weave.engineering"),
            "Weave"
        );
        assert_eq!(
            named(Some(" no-reply "), "no-reply@m.learn.coursera.org"),
            "Coursera"
        );
        assert_eq!(named(Some("The Team"), "team@amazon.co.uk"), "Amazon");
        assert_eq!(named(None, "newsletter@heise.test"), "Heise");
        assert_eq!(
            named(
                Some("Deep Teaching Solutions"),
                "no-reply@m.learn.coursera.org"
            ),
            "Deep Teaching Solutions"
        );
        assert_eq!(named(Some("Anna"), "anna@weber.test"), "Anna");
        assert_eq!(named(None, "root@localhost"), "root@localhost");
    }

    #[test]
    fn one_click_comes_first_then_a_mail_then_a_page() {
        let links = "<mailto:leave@news.test?subject=stop>,\r\n <https://news.test/u/1>";
        assert_eq!(
            way(links, Some("List-Unsubscribe=One-Click"), false),
            Some(Way::OneClick(url("https://news.test/u/1")))
        );
        assert_eq!(
            way(links, None, false),
            Some(Way::Mail(url("mailto:leave@news.test?subject=stop")))
        );
        assert_eq!(
            way("<https://news.test/u/1>", None, false),
            Some(Way::Page(url("https://news.test/u/1")))
        );
        assert_eq!(way("nothing here", None, false), None);
    }

    #[test]
    fn one_click_goes_nowhere_on_this_mac_but_from_the_stand_ins() {
        let post = Some("List-Unsubscribe=One-Click");
        assert_eq!(
            way("<https://127.0.0.1/u>", post, false),
            Some(Way::Page(url("https://127.0.0.1/u")))
        );
        assert_eq!(
            way("<https://192.168.1.2/u>", post, false),
            Some(Way::Page(url("https://192.168.1.2/u")))
        );
        assert_eq!(
            way("<http://news.test/u>", post, false),
            Some(Way::Page(url("http://news.test/u")))
        );
        assert_eq!(
            way("<http://127.0.0.1:7899/u>", post, true),
            Some(Way::OneClick(url("http://127.0.0.1:7899/u")))
        );
    }

    #[test]
    fn a_mailto_address_is_decoded() {
        assert_eq!(percent_decoded("leave%40news.test"), "leave@news.test");
        assert_eq!(percent_decoded("100%"), "100%");
    }

    fn mail(uid: u32, days_ago: i64, seen: bool, address: &str, now: DateTime<Utc>) -> Sent {
        Sent {
            uid,
            arrived: now - TimeDelta::days(days_ago),
            seen,
            name: address.split('@').next().unwrap().into(),
            address: address.into(),
            way: None,
        }
    }

    #[test]
    fn senders_are_counted_and_the_least_read_come_first() {
        let now = Utc::now();
        let mut sent: Vec<Sent> = (0..60)
            .map(|day| mail(day, i64::from(day), false, "daily@shop.test", now))
            .collect();
        sent.extend((0..4).map(|week| {
            mail(
                100 + week,
                i64::from(week) * 7,
                true,
                "weekly@rust.test",
                now,
            )
        }));
        sent.push(mail(200, 1, false, "once@kino.test", now));
        let mut newsletters = by_sender(Uuid::nil(), sent, now);
        least_read_first(&mut newsletters);
        let names: Vec<&str> = newsletters.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["daily", "once", "weekly"]);
        let daily = &newsletters[0];
        assert_eq!((daily.sent, daily.opened, daily.recent), (60, 0, 31));
        assert!((daily.per_week - 7.0).abs() < 0.2, "{}", daily.per_week);
        assert_eq!(daily.newest_uid, 0);
        // One mail yesterday isn't daily.
        assert!((newsletters[1].per_week - 1.0).abs() < 0.01);
        assert_eq!(newsletters[2].opened, 4);
    }
}
