//! Newsletters: mail that offers to unsubscribe (`List-Unsubscribe`, or a
//! mailing service's header with the offer in its text, as Coursera's by
//! SparkPost) and went to many skips the Inbox for a folder of its own, by
//! a rule on the account's server ([`When::Unsubscribe`], written after
//! yours, so a rule of yours still wins). Postrider writes that rule once
//! for each account, after its first sync, and moves the Inbox's
//! newsletters with it ([`Mail::set_newsletters_aside`]); a rule you take
//! away stays away. When what makes a newsletter changes, or how the passes
//! sort, it sorts the account's mail again by it, once
//! ([`Mail::sort_again`]).
//!
//! Whether a mail went to many is read in what its header says, in no
//! language: a mailing list's `List-Id`, `Precedence: bulk` or `list`, or a
//! mailing service's marketing stream (Postmark's `broadcast`, Mailchimp's
//! campaigns; not Mandrill, SES or SendGrid alone, which send orders too).
//! A machine that says it sent the mail by itself (`Auto-Submitted`, RFC
//! 3834), but not to many (newsletters say so too, to keep auto-replies
//! away, and then offer one click to leave or say `Precedence: bulk`),
//! sends notices. Mail that offers to unsubscribe and says neither stays in
//! the Inbox, and Claude Code reads it ([`crate::judging`]): a sender it
//! finds writing only to many is named in the rule ([`Rule::senders`]), so
//! its mail skips the Inbox from then on, with the Mac off too. Big senders
//! put `List-Id` or `Precedence: bulk` on notices about your account too,
//! so the header alone sorts but doesn't decide: Claude Code reads what it
//! sorted as well, and a notice goes back. Keep in Inbox is there for the
//! senders this still gets wrong, and takes one off the rule.
//!
//! `GET /api/w/{workspace}/newsletters` reads all of each account's
//! Newsletters, by sender: how often each writes, when it last did, and how
//! much of it you opened (`\Seen`), the least read first. Read once, only
//! what came since is read again.
//!
//! Unsubscribing asks nothing. It archives what the sender sent at once,
//! and sends the request [`WAIT`] later, so Undo really undoes it: the
//! one-click POST of RFC 8058, else a mail to its `mailto:` over the
//! account's SMTP. A sender with only a page to unsubscribe on gets its page
//! opened in the browser. Keep in Inbox writes a rule that keeps a sender's
//! mail there, ahead of the newsletters' rule, and moves its mail back.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use code_villain_net::Reach;
use futures::future::join_all;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use mail_parser::MessageParser;
use postrider_domain::{
    AccountState, AccountStatus, FolderRole, KeptInInbox, Listing, MailAccount, Newsletter,
    NewsletterRef, Newsletters, SenderProblem, Senders, SettingAside, UnsubscribeSent,
    Unsubscribed, Unsubscribing, Validate,
};
use reqwest::Url;
use serde::Deserialize;
use tokio::sync::watch;
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::auth::Member;
use crate::conversations;
use crate::error::AppError;
use crate::folders::listing;
use crate::imap::{self, Failure, Headed, Place};
use crate::judging;
use crate::mail::Mail;
use crate::rules::{self, Moved, Rule, When};
use crate::send::{is_this_mac, send};
use crate::state::AppState;

/// What the page counts as lately: "They sent you … in the last month."
const RECENT: i64 = 30;

/// How long a request waits before it goes, for Undo.
const WAIT: Duration = Duration::from_secs(10);

/// How long a sender's server may take to answer one.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The header fields a newsletter is read by.
const FIELDS: &str = "FROM LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST";

/// Header fields that say how a mail was sent, besides a mailing service's
/// own ([`SERVICE_FIELDS`]).
const SENT_BY: &[&str] = &[
    "List-Unsubscribe",
    "List-Unsubscribe-Post",
    "List-Id",
    "Precedence",
    "Auto-Submitted",
    "X-PM-Message-Stream",
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

/// Domains a mailing service sends from for many companies, where the
/// domain says nothing of who writes: the same name there joins no
/// addresses.
const SERVICES: &[&str] = &[
    "amazonses.com",
    "beehiiv.com",
    "brevosend.com",
    "ccsend.com",
    "cmail19.com",
    "cmail20.com",
    "convertkit-mail.com",
    "convertkit-mail2.com",
    "createsend.com",
    "customeriomail.com",
    "exacttarget.com",
    "ghost.io",
    "googlegroups.com",
    "groups.io",
    "hubspotemail.net",
    "intercom-mail.com",
    "klaviyomail.com",
    "list-manage.com",
    "mailchimpapp.net",
    "mailerlite.com",
    "mailgun.net",
    "mailgun.org",
    "mailjet.com",
    "mandrillapp.com",
    "mcdlv.net",
    "mcsv.net",
    "mktomail.com",
    "mlsend.com",
    "postmarkapp.com",
    "rsgsv.net",
    "sendgrid.net",
    "sendinblue.com",
    "sparkpostmail.com",
    "substack.com",
    "teamtailor-mail.com",
];

/// What a domain may have between its organisation and its top-level
/// domain: `co` in `amazon.co.uk`.
const SECOND_LEVEL: &[&str] = &["ac", "co", "com", "edu", "gov", "ne", "net", "or", "org"];

/// What `Precedence` says of mail sent to many.
const BULK: &[&str] = &["bulk", "list"];

/// Header fields a mailing service puts on what it sends for others:
/// SparkPost's, SendGrid's, Amazon SES's, Mailgun's, Mailchimp's and
/// Mandrill's, Mailjet's, and the Certified Senders Alliance's.
const SERVICE_FIELDS: &[&str] = &[
    "X-MSFBL",
    "X-SG-EID",
    "X-SES-Outgoing",
    "X-Mailgun-Sid",
    "X-MC-User",
    "X-Mandrill-User",
    "X-Mailjet-Campaign",
    "X-CSA-Complaints",
];

/// How a newsletter's text offers to unsubscribe, in English, German or
/// Swedish.
const UNSUBSCRIBE_WORDS: &[&str] = &[
    "unsubscribe",
    "abmelden",
    "abbestellen",
    "avregistrera",
    "avprenumerera",
    "avsluta prenumeration",
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
    /// Who sends it, by its name and domain ([`sender_of`]): one sender's
    /// addresses are one row on the page.
    sender: Option<String>,
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
    /// Sets the account's newsletters aside: the first time, writes their
    /// rule and moves the Inbox's newsletters to Newsletters, but those from
    /// senders kept in the Inbox. Once what makes a newsletter changes, it
    /// sorts them again ([`Mail::sort_again`]).
    pub(crate) async fn set_newsletters_aside(&self, account: &MailAccount) -> Result<(), Failure> {
        let sorted_by: Option<Option<String>> =
            sqlx::query_scalar("SELECT sorted_by FROM newsletters_set_aside WHERE account_id = $1")
                .bind(account.id)
                .fetch_optional(&self.pool)
                .await?;
        let by = sorting();
        match sorted_by {
            Some(Some(sorted_by)) if sorted_by == by => return Ok(()),
            Some(_) => return self.sort_again(account, &by).await,
            None => {}
        }
        let place = self.folder_for(account, FolderRole::Newsletters).await?;
        let mut rules = self.rules(account).await?;
        if !rules.iter().any(Rule::is_newsletters) {
            rules.push(Rule::newsletters(&place));
            self.put_rules(account, &rules).await?;
        }
        let moved = self.set_aside(account, &rules, &place).await?;
        self.subscribe_folders(account, &rules).await?;
        sqlx::query(
            "INSERT INTO newsletters_set_aside (account_id, sorted_by) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(account.id)
        .bind(&by)
        .execute(&self.pool)
        .await?;
        tracing::info!(address = %account.address, "set {moved} newsletters aside");
        Ok(())
    }

    /// Sorts the account's mail again by what makes a newsletter now
    /// ([`sorting`]), once for each change to it: writes the rule again,
    /// moves what a machine sent by itself from Newsletters back to the
    /// Inbox, and sets aside the Inbox's newsletters, but those from senders
    /// kept there. It reads all of Newsletters, but what a rule of yours
    /// sorts there, whatever the server's SEARCH finds. What its header
    /// can't tell, Claude Code reads there once (`judge_again`,
    /// [`crate::judging`]), and the notices come back. A rule you took away
    /// stays away. It subscribes the folders Postrider sorts into too, so
    /// the account's other mail apps show them.
    async fn sort_again(&self, account: &MailAccount, by: &str) -> Result<(), Failure> {
        let rules = self.rules(account).await?;
        let sets_aside = rules.iter().any(Rule::is_newsletters);
        if sets_aside {
            self.put_rules(account, &rules).await?;
        }
        let mut to_read = 0;
        if let Some(place) = self.newsletters_folder(account).await? {
            let there = self
                .told_in(
                    account,
                    &place.path,
                    &but_yours(&rules, &place.path, "ALL")?,
                    &named(&rules),
                )
                .await?;
            let notices = of(&there, Said::Notice);
            let unsure = of(&there, Said::Unsure).len();
            to_read = there.len() - notices.len();
            let back = self
                .move_out(account, &place.path, &notices, &[], "INBOX")
                .await?;
            let aside = if sets_aside {
                self.set_aside(account, &rules, &place).await?
            } else {
                0
            };
            tracing::info!(
                address = %account.address,
                "sorted again: of the {} in Newsletters, {} a machine sent by itself, \
                 {} went back; {} say they went to many and {unsure} neither, for \
                 Claude Code to read; {aside} newsletters aside",
                there.len(),
                notices.len(),
                back.count,
                to_read - unsure
            );
        } else {
            tracing::info!(address = %account.address, "sorted again: no Newsletters");
        }
        self.subscribe_folders(account, &rules).await?;
        sqlx::query(
            "UPDATE newsletters_set_aside SET sorted_by = $2, judge_again = $3 \
             WHERE account_id = $1",
        )
        .bind(account.id)
        .bind(by)
        .bind(to_read > 0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Moves the Inbox's mail that the newsletters' rule would have to
    /// Newsletters, but what a rule of yours keeps there, and what was
    /// judged already: it's where that, or you, put it. How much.
    async fn set_aside(
        &self,
        account: &MailAccount,
        rules: &[Rule],
        place: &Place,
    ) -> Result<usize, Failure> {
        let query = but_yours(rules, "INBOX", &rules::search(&[When::Unsubscribe], "")?)?;
        let told = self
            .told_in(account, "INBOX", &query, &named(rules))
            .await?;
        let judged = judging::judged_already(&self.pool, account, &told, false).await?;
        let many: Vec<u32> = told
            .iter()
            .filter(|mail| mail.said == Said::Many && !judged.contains(&mail.message_id))
            .map(|mail| mail.uid)
            .collect();
        let moved = self
            .move_out(account, "INBOX", &many, &[], &place.path)
            .await?;
        Ok(moved.count)
    }

    /// The mail `query` finds in the folder at `path`, as the newsletters'
    /// rule reads it with `senders` named.
    pub(crate) async fn told_in(
        &self,
        account: &MailAccount,
        path: &str,
        query: &str,
        senders: &[String],
    ) -> Result<Vec<Told>, Failure> {
        let headed = self
            .on_server(account, |session| {
                let (path, query) = (path.to_string(), query.to_string());
                Box::pin(async move { imap::headers(session, &path, &query, &told_by()).await })
            })
            .await?;
        Ok(headed
            .iter()
            .filter_map(|mail| told(mail, senders))
            .collect())
    }

    /// Subscribes the folders Postrider puts mail in: Archive, Trash, Junk,
    /// Newsletters and those its rules sort into. One it makes is
    /// subscribed as it's made; one it found may not be.
    async fn subscribe_folders(
        &self,
        account: &MailAccount,
        rules: &[Rule],
    ) -> Result<(), Failure> {
        let roles = [
            FolderRole::Archive,
            FolderRole::Trash,
            FolderRole::Junk,
            FolderRole::Newsletters,
        ];
        let mut paths: Vec<String> = self
            .places(account)
            .await?
            .into_iter()
            .filter(|place| place.role.is_some_and(|role| roles.contains(&role)))
            .map(|place| place.path)
            .collect();
        for rule in rules {
            if rule.path != "INBOX" && !paths.contains(&rule.path) {
                paths.push(rule.path.clone());
            }
        }
        let count = self
            .on_server(account, |session| {
                let paths = paths.clone();
                Box::pin(async move { imap::subscribe(session, &paths).await })
            })
            .await?;
        if count > 0 {
            tracing::info!(address = %account.address, "subscribed {count} folders");
        }
        Ok(())
    }

    /// The account's Newsletters, if it has one.
    pub(crate) async fn newsletters_folder(
        &self,
        account: &MailAccount,
    ) -> Result<Option<Place>, Failure> {
        Ok(self
            .places(account)
            .await?
            .into_iter()
            .find(|place| place.role == Some(FolderRole::Newsletters)))
    }

    /// A sender's newest mail in Newsletters, from every account it writes
    /// to, and from every address it writes from there.
    pub async fn sender_mail(
        &self,
        workspace_id: Uuid,
        address: &str,
    ) -> Result<Listing, AppError> {
        let accounts = self.accounts(workspace_id).await?;
        let address = &address.trim().to_lowercase();
        imap::from_query(address).map_err(|failure| AppError::invalid(failure.message()))?;
        let read = join_all(accounts.iter().map(|account| async move {
            let Some(place) = self.newsletters_folder(account).await? else {
                return Ok(Vec::new());
            };
            let when: Vec<When> = self
                .addresses_of(account, &place, address)
                .await?
                .into_iter()
                .map(When::From)
                .collect();
            let query = rules::search(&when, "")?;
            self.read(account, &place, Some(&query)).await
        }))
        .await;
        Ok(listing(&accounts, read, HashMap::new()))
    }

    /// Every address the sender at `address` writes from to the account's
    /// Newsletters, as the page counts them: by what it read last, else by
    /// reading it now.
    async fn addresses_of(
        &self,
        account: &MailAccount,
        place: &Place,
        address: &str,
    ) -> Result<Vec<String>, Failure> {
        let now = Utc::now();
        let kept = self
            .lock_newsletter_headers()
            .get(&(account.id, place.path.clone()))
            .cloned();
        let newsletters = match kept {
            Some(kept) => {
                let local = is_this_mac(&account.host);
                let sent = kept
                    .mails
                    .into_values()
                    .filter_map(|h| read(h, local))
                    .collect();
                by_sender(account.id, sent, now)
            }
            None => self.newsletters_of(account, now).await?,
        };
        Ok(newsletters
            .into_iter()
            .find(|n| n.addresses.iter().any(|a| a == address))
            .map_or_else(|| vec![address.to_string()], |n| n.addresses))
    }

    /// All of the account's newsletters, by sender: what the page read
    /// before, and what came since.
    async fn newsletters_of(
        &self,
        account: &MailAccount,
        now: DateTime<Utc>,
    ) -> Result<Vec<Newsletter>, Failure> {
        let Some(place) = self.newsletters_folder(account).await? else {
            return Ok(Vec::new());
        };
        let key = (account.id, place.path.clone());
        let kept = self
            .lock_newsletter_headers()
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let headers = self
            .on_server(account, |session| {
                let (path, kept) = (place.path.clone(), kept.clone());
                Box::pin(async move { imap::all_headers(session, &path, FIELDS, kept).await })
            })
            .await?;
        self.lock_newsletter_headers().insert(key, headers.clone());
        let local = is_this_mac(&account.host);
        let sent = headers
            .mails
            .into_values()
            .filter_map(|h| read(h, local))
            .collect();
        Ok(by_sender(account.id, sent, now))
    }

    fn lock_newsletter_headers(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<(Uuid, String), imap::KeptHeaders>> {
        self.newsletter_headers.lock().expect("never poisoned")
    }

    /// What these senders sent to the account's Newsletters, by address.
    async fn theirs(
        &self,
        account: &MailAccount,
        place: &Place,
        addresses: &[String],
    ) -> Result<HashMap<String, Theirs>, Failure> {
        let when: Vec<When> = addresses.iter().cloned().map(When::From).collect();
        let query = rules::search(&when, "")?;
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
                // The address the sender's name resolves to is checked too,
                // so a public name that leads to this Mac goes nowhere.
                let local = is_this_mac(&account.host);
                if !reachable(url, local) {
                    return Err(format!(
                        "{name}'s way to unsubscribe isn't on the internet."
                    ));
                }
                let client = code_villain_net::client(reach(local))
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

/// What a mail's header says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Said {
    /// Sent by a machine by itself, and not to many: a notice.
    Notice,
    /// Sent to many, or from a sender the rule names: a newsletter.
    Many,
    /// Neither: Claude Code reads it ([`crate::judging`]).
    Unsure,
}

/// A mail as the newsletters' rule reads it, by its [`told_by`] fields.
#[derive(Debug, Clone)]
pub(crate) struct Told {
    pub uid: u32,
    pub address: String,
    pub subject: String,
    /// As the Inbox's copy keeps it: without its brackets.
    pub message_id: String,
    pub said: Said,
    /// What says it went to many: its header's field ("List-Id",
    /// "Precedence"…), or "named" for a sender the rule names.
    pub by: Option<&'static str>,
    /// The fields that say how it was sent, for Claude Code:
    /// "List-Unsubscribe", "Precedence: bulk".
    pub fields: Vec<String>,
}

/// The header fields a mail is told by.
pub(crate) fn told_by() -> String {
    format!(
        "FROM SUBJECT MESSAGE-ID {} {}",
        SENT_BY.join(" "),
        SERVICE_FIELDS.join(" ")
    )
}

/// A mail by its [`told_by`] fields, as the rule reads it with `senders`
/// named ([`test`]); `None` without a sender.
pub(crate) fn told(headed: &Headed, senders: &[String]) -> Option<Told> {
    let message = MessageParser::new().parse_headers(&headed.header)?;
    let address = message
        .from()
        .and_then(|list| list.first())
        .and_then(|from| from.address())?
        .trim()
        .to_lowercase();
    let raw = |field: &str| {
        message.headers().get(field).map(|header| {
            String::from_utf8_lossy(header.raw_value())
                .trim()
                .to_string()
        })
    };
    let is = |field: &str, values: &[&str]| {
        raw(field).is_some_and(|said| values.iter().any(|v| said.eq_ignore_ascii_case(v)))
    };
    let bulk = is("Precedence", BULK);
    let by = if raw("List-Id").is_some() {
        Some("List-Id")
    } else if bulk {
        Some("Precedence")
    } else if raw("X-PM-Message-Stream")
        .is_some_and(|stream| stream.to_lowercase().contains("broadcast"))
    {
        Some("X-PM-Message-Stream")
    } else if raw("X-MC-User").is_some() && raw("X-Mandrill-User").is_none() {
        Some("X-MC-User")
    } else if senders.contains(&address) {
        Some("named")
    } else {
        None
    };
    let machine = raw("Auto-Submitted").is_some()
        && !is("Auto-Submitted", &["no"])
        && raw("List-Unsubscribe-Post").is_none()
        && !bulk;
    let fields = SENT_BY
        .iter()
        .chain(SERVICE_FIELDS)
        .filter_map(|field| {
            let value = raw(field)?;
            Some(match *field {
                "Precedence" | "Auto-Submitted" | "X-PM-Message-Stream" => {
                    format!("{field}: {}", value.chars().take(60).collect::<String>())
                }
                _ => (*field).to_string(),
            })
        })
        .collect();
    Some(Told {
        uid: headed.uid,
        address,
        subject: message.subject().unwrap_or_default().to_string(),
        message_id: message.message_id().unwrap_or_default().to_string(),
        said: if machine {
            Said::Notice
        } else if by.is_some() {
            Said::Many
        } else {
            Said::Unsure
        },
        by: by.filter(|_| !machine),
        fields,
    })
}

/// Strings as a Sieve list's items: `"bulk", "list"`.
fn strings(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The newsletters' rule in Sieve, as [`told`] reads a mail: one with a way
/// to unsubscribe, that went to many or comes from one of `senders`, and
/// that no machine sent by itself. `bodies` when the server reads mail's
/// text, for a mailing service's offer in it ([`from_a_service_test`]).
pub(crate) fn test(bodies: bool, senders: &[String]) -> String {
    let link = if bodies {
        format!(
            "anyof (exists \"list-unsubscribe\", {})",
            from_a_service_test()
        )
    } else {
        "exists \"list-unsubscribe\"".into()
    };
    let bulk = strings(BULK);
    let mut many = format!(
        "exists \"list-id\", header :is \"precedence\" [{bulk}], \
         header :contains \"x-pm-message-stream\" \"broadcast\", \
         allof (exists \"x-mc-user\", not exists \"x-mandrill-user\")"
    );
    if !senders.is_empty() {
        let senders: Vec<String> = senders.iter().map(|s| rules::sieve_string(s)).collect();
        many = format!("{many}, address :is \"from\" [{}]", senders.join(", "));
    }
    format!(
        "allof ({link}, anyof ({many}), not allof (exists \"auto-submitted\", \
         not header :is \"auto-submitted\" \"no\", not exists \"list-unsubscribe-post\", \
         not header :is \"precedence\" [{bulk}]))"
    )
}

/// How the passes sort, besides what the newsletters' rule says: counted up
/// when they change, so each account's mail is sorted again by them.
const PASSES: u32 = 4;

/// What makes a newsletter or a notice, as the newsletters' rule says it in
/// Sieve, and how the passes sort ([`PASSES`]): when it changes, each
/// account's mail is sorted again.
fn sorting() -> String {
    format!("{PASSES} {}", When::Unsubscribe.test(true))
}

/// Mail from a mailing service that offers to unsubscribe in its text,
/// where its header doesn't: Coursera's, by SparkPost. In Sieve, for the
/// newsletters' rule on a server that reads mail's text.
pub(crate) fn from_a_service_test() -> String {
    let fields = SERVICE_FIELDS
        .iter()
        .map(|field| format!("exists \"{}\"", field.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "allof (anyof ({fields}), body :text :contains [{}])",
        strings(UNSUBSCRIBE_WORDS)
    )
}

/// [`from_a_service_test`] as IMAP SEARCH keys.
pub(crate) fn from_a_service_search() -> String {
    let keys = |items: &[&str], key: &dyn Fn(&str) -> String| {
        rules::any(&items.iter().map(|item| key(item)).collect::<Vec<_>>())
    };
    format!(
        "{} {}",
        keys(SERVICE_FIELDS, &|field| format!("HEADER {field} \"\"")),
        keys(UNSUBSCRIBE_WORDS, &|word| format!("BODY \"{word}\""))
    )
}

/// The mail that `what` finds in the folder at `path`, but not what a rule
/// of yours sorts there: from senders kept in the Inbox, or sorted into
/// Newsletters.
pub(crate) fn but_yours(rules: &[Rule], path: &str, what: &str) -> Result<String, Failure> {
    let yours: Vec<When> = rules
        .iter()
        .filter(|rule| rule.path == path && !rule.is_newsletters())
        .flat_map(|rule| rule.when.clone())
        .collect();
    if yours.is_empty() {
        Ok(what.to_string())
    } else {
        rules::search(&yours, &format!("{what} NOT "))
    }
}

/// The senders the newsletters' rule names.
pub(crate) fn named(rules: &[Rule]) -> Vec<String> {
    rules
        .iter()
        .find(|rule| rule.is_newsletters())
        .map(|rule| rule.senders.clone())
        .unwrap_or_default()
}

/// The UIDs of the mail whose header says `said`.
pub(crate) fn of(told: &[Told], said: Said) -> Vec<u32> {
    told.iter()
        .filter(|mail| mail.said == said)
        .map(|mail| mail.uid)
        .collect()
}

/// A newsletter from its header fields; `None` without a sender.
fn read(headed: Headed, local: bool) -> Option<Sent> {
    let message = MessageParser::new().parse_headers(&headed.header)?;
    let from = message.from().and_then(|list| list.first());
    let address = from.as_ref()?.address()?.trim().to_lowercase();
    let name = sender_name(from.as_ref().and_then(|f| f.name()), &address);
    let sender = sender_of(&name, &address);
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
        sender,
        way,
    })
}

/// Who sends a mail, by the name the page calls it ([`sender_name`]) and
/// the domain its owner registered (`example.co.uk` of
/// `news.example.co.uk`, by the public suffix list), so the addresses one
/// sender writes from count as one. Never by the name alone: an "Info" is
/// its organisation's, at its domain. `None` where the address alone says
/// who: at a domain many send from, a mailing service's or a free mail one.
fn sender_of(name: &str, address: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let domain = psl::domain_str(address.rsplit_once('@')?.1)?;
    if SERVICES.contains(&domain) || conversations::SHARED.contains(&domain) {
        return None;
    }
    Some(format!("{name} {domain}"))
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

/// Whether a request may go there: only to the internet, where newsletters
/// live, not this Mac or a network of its own; from the stand-ins on this
/// Mac, there too, but never to a link-local or cloud-metadata address. A
/// name is checked again on the addresses it resolves to, as the request
/// connects ([`reach`]).
fn reachable(url: &Url, local: bool) -> bool {
    reach(local).allows_url(url)
}

/// Where one-click requests may go: the internet only, unless the account
/// is this Mac's stand-ins (`code/kit/net`).
fn reach(local: bool) -> Reach {
    if local {
        Reach::OwnNetworks
    } else {
        Reach::Public
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
/// and how often it writes. Addresses whose newest mail gives the same
/// name at the same domain are one sender.
fn by_sender(account_id: Uuid, sent: Vec<Sent>, now: DateTime<Utc>) -> Vec<Newsletter> {
    let mut by_address: HashMap<String, Vec<Sent>> = HashMap::new();
    for mail in sent {
        by_address
            .entry(mail.address.clone())
            .or_default()
            .push(mail);
    }
    let mut senders: HashMap<String, Vec<Sent>> = HashMap::new();
    for (address, mails) in by_address {
        let sender = by_newest(&mails)
            .first()
            .and_then(|mail| mail.sender.clone())
            .unwrap_or(address);
        senders.entry(sender).or_default().extend(mails);
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    senders
        .into_values()
        .filter_map(|mails| {
            let newest = mails.iter().max_by_key(|mail| (mail.arrived, mail.uid))?;
            let oldest = mails.iter().map(|mail| mail.arrived).min()?;
            // A week at least, or one mail yesterday reads as daily.
            let weeks = (now - oldest).num_minutes().max(7 * 24 * 60) / 60;
            let sent = count(mails.len());
            let mut names: Vec<String> = Vec::new();
            let mut addresses: Vec<String> = Vec::new();
            for mail in by_newest(&mails) {
                if !names.contains(&mail.name) {
                    names.push(mail.name.clone());
                }
                if !addresses.contains(&mail.address) {
                    addresses.push(mail.address.clone());
                }
            }
            Some(Newsletter {
                account_id,
                name: newest.name.clone(),
                names,
                address: newest.address.clone(),
                addresses,
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

/// A sender's mail, the newest first.
fn by_newest(mails: &[Sent]) -> Vec<&Sent> {
    let mut sorted: Vec<&Sent> = mails.iter().collect();
    sorted.sort_by_key(|mail| std::cmp::Reverse((mail.arrived, mail.uid)));
    sorted
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
    let rules = join_all(accounts.iter().map(|account| state.mail.rules(account))).await;
    let named = accounts
        .iter()
        .zip(rules)
        .flat_map(|(account, rules)| {
            named(&rules.unwrap_or_default())
                .into_iter()
                .map(|address| NewsletterRef {
                    account_id: account.id,
                    address,
                })
        })
        .collect();
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
    let mut not_sorted = Vec::new();
    for (account, read) in accounts.iter().zip(read) {
        match read {
            Ok(found) => {
                newsletters.extend(found);
                if !set_aside.contains(&account.id) {
                    setting_aside.push(SettingAside {
                        account_id: account.id,
                        problem: failed.get(&account.id).cloned(),
                    });
                } else if let Some(problem) = failed.get(&account.id) {
                    not_sorted.push(SettingAside {
                        account_id: account.id,
                        problem: Some(problem.clone()),
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
        not_sorted,
        named,
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
/// The newsletters' rule names them no more, and what Claude Code made of
/// their mail is forgotten.
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
            for rule in rules.iter_mut().filter(|rule| rule.is_newsletters()) {
                rule.senders.retain(|sender| !addresses.contains(sender));
            }
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
                    senders: Vec::new(),
                }),
            }
            state.mail.put_rules(&account, &rules).await?;
            undo.rules.push((account.clone(), before));
            sqlx::query(
                "DELETE FROM newsletter_verdicts WHERE account_id = $1 AND address = ANY($2)",
            )
            .bind(account.id)
            .bind(&addresses)
            .execute(&state.mail.pool)
            .await?;
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

    /// What [`told`] makes of a mail with these header lines, from
    /// `from`, when the rule names `senders`.
    fn said(from: &str, lines: &str, senders: &[&str]) -> Said {
        let header = format!("From: {from}\nSubject: Hello\nMessage-ID: <1@x>\n{lines}\n")
            .replace('\n', "\r\n");
        let headed = crate::imap::Headed {
            uid: 1,
            arrived: None,
            seen: false,
            header: header.into_bytes(),
        };
        let senders: Vec<String> = senders.iter().map(|s| (*s).to_string()).collect();
        told(&headed, &senders).unwrap().said
    }

    #[test]
    fn a_header_tells_mail_sent_to_many_in_no_language() {
        let offer = "List-Unsubscribe: <https://x/u>";
        let said = |lines: &str| said("news@shop.test", &format!("{offer}\n{lines}"), &[]);
        assert_eq!(said("List-Id: <news.shop.test>"), Said::Many);
        assert_eq!(said("Precedence: bulk"), Said::Many);
        assert_eq!(said("Precedence: List"), Said::Many);
        assert_eq!(said("X-PM-Message-Stream: Broadcast"), Said::Many);
        // Mailchimp's own, but not its Mandrill, which sends orders too.
        assert_eq!(said("X-MC-User: 3f2a"), Said::Many);
        assert_eq!(said("X-MC-User: 3f2a\nX-Mandrill-User: md_1"), Said::Unsure);
        assert_eq!(said("X-PM-Message-Stream: outbound"), Said::Unsure);
        // A way to unsubscribe alone says neither: Claude Code reads it.
        assert_eq!(said(""), Said::Unsure);
        assert_eq!(said("Precedence: first-class"), Said::Unsure);
    }

    #[test]
    fn the_log_names_what_said_it_went_to_many() {
        let by = |lines: &str, senders: &[String]| {
            let header = format!(
                "From: noreply-accounts@google.test\nSubject: You shared some data\n\
                 Message-ID: <1@x>\nList-Unsubscribe: <https://x/u>\n{lines}\n"
            )
            .replace('\n', "\r\n");
            let headed = crate::imap::Headed {
                uid: 1,
                arrived: None,
                seen: false,
                header: header.into_bytes(),
            };
            told(&headed, senders).unwrap().by
        };
        assert_eq!(by("List-Id: <x>\nPrecedence: bulk", &[]), Some("List-Id"));
        assert_eq!(by("Precedence: bulk", &[]), Some("Precedence"));
        assert_eq!(
            by("", &["noreply-accounts@google.test".into()]),
            Some("named")
        );
        assert_eq!(by("", &[]), None);
        // A notice says nothing went to many.
        assert_eq!(
            by("Auto-Submitted: auto-generated\nList-Id: <x>", &[]),
            None
        );
    }

    #[test]
    fn a_machine_that_sent_by_itself_is_a_notice() {
        let offer = "List-Unsubscribe: <https://x/u>";
        let said = |lines: &str| said("noreply@accounts.test", &format!("{offer}\n{lines}"), &[]);
        // A sign-in that says where to unsubscribe.
        assert_eq!(said("Auto-Submitted: auto-generated"), Said::Notice);
        assert_eq!(
            said("Auto-Submitted: auto-generated\nList-Id: <x>"),
            Said::Notice
        );
        assert_eq!(said("Auto-Submitted: no"), Said::Unsure);
        // Newsletters say it too, to keep auto-replies away, and then say
        // they went to many.
        assert_eq!(
            said("Auto-Submitted: auto-generated\nPrecedence: bulk"),
            Said::Many
        );
        assert_eq!(
            said(
                "Auto-Submitted: auto-generated\nList-Unsubscribe-Post: List-Unsubscribe=One-Click"
            ),
            Said::Unsure
        );
        assert_eq!(
            said(
                "Auto-Submitted: auto-generated\nList-Id: <x>\n\
                 List-Unsubscribe-Post: List-Unsubscribe=One-Click"
            ),
            Said::Many
        );
    }

    #[test]
    fn a_sender_the_rule_names_writes_to_many() {
        let offer = "List-Unsubscribe: <https://x/u>";
        assert_eq!(
            said(
                "Coursera <No-Reply@M.Coursera.test>",
                offer,
                &["no-reply@m.coursera.test"]
            ),
            Said::Many
        );
        assert_eq!(
            said(
                "no-reply@coursera.test",
                offer,
                &["no-reply@m.coursera.test"]
            ),
            Said::Unsure
        );
        // A machine that sent by itself still goes to the Inbox.
        assert_eq!(
            said(
                "no-reply@m.coursera.test",
                &format!("{offer}\nAuto-Submitted: auto-replied"),
                &["no-reply@m.coursera.test"]
            ),
            Said::Notice
        );
    }

    #[test]
    fn the_rule_names_its_senders_beside_the_header() {
        let named = test(false, &["a@x.test".into(), "b@y.test".into()]);
        assert!(named.contains(
            "header :contains \"x-pm-message-stream\" \"broadcast\", \
             allof (exists \"x-mc-user\", not exists \"x-mandrill-user\"), \
             address :is \"from\" [\"a@x.test\", \"b@y.test\"])"
        ));
        assert!(!test(false, &[]).contains("address :is"));
    }

    #[test]
    fn newsletters_are_read_whole_but_what_your_rules_sort_there() {
        let rule = |path: &str, from: &str| Rule {
            folder: path.into(),
            path: path.into(),
            when: vec![When::From(from.into())],
            senders: Vec::new(),
        };
        let newsletters = Place {
            path: "INBOX.Newsletters".into(),
            name: "Newsletters".into(),
            role: Some(FolderRole::Newsletters),
        };
        let rules = vec![
            rule("INBOX", "ica.se"),
            rule("INBOX.Newsletters", "shop.test"),
            Rule::newsletters(&newsletters),
        ];
        let but_yours = |rules: &[Rule], path: &str, what: &str| {
            but_yours(rules, path, what).unwrap_or_default()
        };
        assert_eq!(
            but_yours(&rules, "INBOX.Newsletters", "ALL"),
            "ALL NOT FROM \"shop.test\""
        );
        assert_eq!(
            but_yours(&rules, "INBOX", "HEADER List-Unsubscribe \"\""),
            "HEADER List-Unsubscribe \"\" NOT FROM \"ica.se\""
        );
        assert_eq!(but_yours(&rules[2..], "INBOX.Newsletters", "ALL"), "ALL");
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
        for not in [
            "https://localhost/u",
            "https://100.100.1.2/u",
            "https://[::ffff:10.0.0.1]/u",
            "https://2852039166/u",
        ] {
            assert_eq!(
                way(&format!("<{not}>"), post, false),
                Some(Way::Page(url(not))),
                "{not}"
            );
        }
        // The stand-ins on this Mac reach it, never cloud metadata.
        assert_eq!(
            way("<http://169.254.169.254/u>", post, true),
            Some(Way::Page(url("http://169.254.169.254/u")))
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
            sender: None,
            way: None,
        }
    }

    /// A mail from `address`, giving `name` as its sender's.
    fn from(uid: u32, days_ago: i64, seen: bool, name: &str, address: &str) -> Sent {
        Sent {
            name: sender_name(Some(name), address),
            sender: sender_of(&sender_name(Some(name), address), address),
            ..mail(uid, days_ago, seen, address, Utc::now())
        }
    }

    #[test]
    fn a_sender_is_its_name_at_the_domain_its_owner_registered() {
        let who = |name, address| sender_of(&sender_name(name, address), address);
        let deep = who(
            Some("Deep Teaching Solutions"),
            "news@mail.deepteaching.com",
        );
        assert_eq!(
            deep.as_deref(),
            Some("deepteachingsolutions deepteaching.com")
        );
        assert_eq!(
            who(Some("Deep  teaching solutions"), "hello@deepteaching.com"),
            deep
        );
        assert_eq!(
            who(Some("Handelsbanken"), "news@shop.handelsbanken.co.uk"),
            who(Some("Handelsbanken"), "info@handelsbanken.co.uk")
        );
        assert_ne!(
            who(Some("Shop"), "news@shop.co.uk"),
            who(Some("Shop"), "news@other.co.uk")
        );
        assert_eq!(
            who(Some("Kino"), "news@kino.test").as_deref(),
            Some("kino kino.test")
        );
        // A name that says nothing is its organisation's, never one of its own.
        assert_eq!(
            who(Some("Info"), "info@deepteaching.com"),
            who(None, "news@mail.deepteaching.com")
        );
        assert_ne!(
            who(Some("Info"), "info@deepteaching.com"),
            who(Some("Info"), "info@other.com")
        );
        // A domain many send from says no one.
        assert_eq!(
            who(Some("Kustom"), "no-reply@kustom.teamtailor-mail.com"),
            None
        );
        assert_eq!(who(Some("Info"), "info@sendgrid.net"), None);
        assert_eq!(who(Some("Anna Berg"), "anna@gmail.com"), None);
    }

    #[test]
    fn the_addresses_one_sender_writes_from_are_one_row() {
        let now = Utc::now();
        let sent = vec![
            from(
                1,
                9,
                true,
                "Deep Teaching Solutions",
                "news@mail.deepteaching.com",
            ),
            from(
                2,
                2,
                false,
                "Deep Teaching Solutions",
                "events@deepteaching.com",
            ),
            from(
                3,
                5,
                false,
                "Deep Teaching Solutions",
                "news@mail.deepteaching.com",
            ),
            from(4, 1, false, "Info", "info@deepteaching.com"),
            from(5, 3, false, "Info", "info@news.deepteaching.com"),
            from(6, 2, false, "Info", "info@other.com"),
            from(7, 4, false, "Kustom", "jobs@kustom.teamtailor-mail.com"),
            from(8, 4, false, "Kustom", "no-reply@kustom.teamtailor-mail.com"),
        ];
        let newsletters = by_sender(Uuid::nil(), sent, now);
        assert_eq!(newsletters.len(), 5);
        let deep = newsletters
            .iter()
            .find(|n| n.name == "Deep Teaching Solutions")
            .unwrap();
        assert_eq!(
            deep.addresses,
            ["events@deepteaching.com", "news@mail.deepteaching.com"]
        );
        assert_eq!(deep.address, "events@deepteaching.com");
        assert_eq!((deep.sent, deep.opened, deep.newest_uid), (3, 1, 2));
        // The page calls an "Info" by its organisation: one row for each.
        let info = newsletters
            .iter()
            .find(|n| n.name == "Deepteaching")
            .unwrap();
        assert_eq!(
            info.addresses,
            ["info@deepteaching.com", "info@news.deepteaching.com"]
        );
        for n in newsletters
            .iter()
            .filter(|n| !["Deep Teaching Solutions", "Deepteaching"].contains(&n.name.as_str()))
        {
            assert_eq!(n.addresses, std::slice::from_ref(&n.address));
        }
    }

    #[test]
    fn a_sender_that_writes_as_several_keeps_every_name() {
        let now = Utc::now();
        let as_who = |uid, days_ago, name: &str| Sent {
            name: name.into(),
            ..mail(
                uid,
                days_ago,
                true,
                "messaging-digest-noreply@linkedin.com",
                now,
            )
        };
        let newsletters = by_sender(
            Uuid::nil(),
            vec![
                as_who(1, 1, "Dimitrios Kombas via LinkedIn"),
                as_who(2, 0, "Anna Berg via LinkedIn"),
                as_who(3, 2, "Dimitrios Kombas via LinkedIn"),
            ],
            now,
        );
        let [linkedin] = newsletters.as_slice() else {
            panic!("one sender");
        };
        assert_eq!(linkedin.name, "Anna Berg via LinkedIn");
        assert_eq!(
            linkedin.names,
            ["Anna Berg via LinkedIn", "Dimitrios Kombas via LinkedIn"]
        );
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
