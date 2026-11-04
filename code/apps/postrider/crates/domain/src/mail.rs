//! Mail accounts and the calendar server, which sync, and what Postrider
//! reads from them live: the Inbox and today's events.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::account::{clean_name, normalize_email};
use crate::model::{Validate, ValidationError};

/// How many data hues there are (`--data-1` to `--data-6`).
pub const HUES: i32 = 6;

/// A mail account Postrider reads over IMAP. Its password is in the macOS
/// keychain, never here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MailAccount {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// What Postrider calls it: "Code Villain".
    pub name: String,
    /// The address its mail comes to.
    pub address: String,
    /// The IMAP server: `styx.uberspace.de`.
    pub host: String,
    /// 993, over TLS.
    pub port: i32,
    /// What it signs in with, usually the address.
    pub username: String,
    /// The data hue (1 to 6) its senders and its calendar wear.
    pub hue: i32,
    /// Where it sits among the accounts.
    pub position: i32,
    /// What mail sent from it is signed with, below a "-- " line. Empty:
    /// nothing.
    pub signature: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A new account, as the server keeps it once it signed in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MailAccountCreate {
    pub name: String,
    pub address: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub hue: i32,
    /// After the last account when left out.
    pub position: Option<i32>,
}

impl Validate for MailAccountCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        account_name(&self.name)?;
        normalize_email(&self.address)?;
        host(&self.host)?;
        port(self.port)?;
        hue(self.hue)
    }
}

/// What a client may change of an account: its name, its hue and its
/// signature.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MailAccountPatch {
    pub name: Option<String>,
    pub hue: Option<i32>,
    pub position: Option<i32>,
    pub signature: Option<String>,
}

impl Validate for MailAccountPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.name.as_deref().map(account_name).transpose()?;
        self.hue.map(hue).transpose()?;
        self.signature.as_deref().map(signature).transpose()?;
        Ok(())
    }
}

/// The most a signature may hold, in characters.
pub const SIGNATURE_CHARS: usize = 2_000;

fn signature(value: &str) -> Result<(), ValidationError> {
    if value.chars().count() > SIGNATURE_CHARS {
        return invalid(format!(
            "A signature holds up to {SIGNATURE_CHARS} characters."
        ));
    }
    Ok(())
}

/// `text` signed with `signature`: below it, after the "-- " line mail
/// apps know a signature by. An empty signature signs nothing; with no
/// text, the mail starts with it.
pub fn signed(text: &str, signature: &str) -> String {
    let signature = signature.trim_matches(['\r', '\n']).trim_end();
    if signature.trim().is_empty() {
        return text.to_string();
    }
    let text = text.trim_end();
    if text.is_empty() {
        format!("-- \n{signature}")
    } else {
        format!("{text}\n\n-- \n{signature}")
    }
}

/// "Add an account": checked by signing in before it's kept.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MailAccountConnect {
    pub name: String,
    pub address: String,
    pub host: String,
    /// 993 when left out.
    pub port: Option<i32>,
    /// The address when left out.
    pub username: Option<String>,
    /// Goes to the keychain.
    pub password: String,
}

impl MailAccountConnect {
    /// The connection, cleaned up: the address in lowercase, the port and
    /// the login filled in.
    pub fn cleaned(&self) -> Result<MailAccountConnect, ValidationError> {
        let address = normalize_email(&self.address)?;
        let username = self
            .username
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .unwrap_or(&address)
            .to_string();
        let cleaned = MailAccountConnect {
            name: account_name(&self.name)?,
            host: host(&self.host)?,
            port: Some(port(self.port.unwrap_or(993))?),
            username: Some(username),
            password: self.password.clone(),
            address,
        };
        if cleaned.password.is_empty() {
            return Err(ValidationError("The password is missing.".into()));
        }
        Ok(cleaned)
    }
}

/// The calendar server, read over CalDAV: Radicale. Its password is in the
/// keychain too. Each calendar on it wears the hue of the account the
/// person chose for it, else of the account it's named after, else of the
/// account at its domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Calendar {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// `https://calendar.lazarski.nu/`
    pub url: String,
    pub username: String,
    /// The account each calendar wears the colour of, where the person
    /// chose: by the calendar's address on the server.
    #[ts(type = "Record<string, string>")]
    pub accounts: BTreeMap<String, Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What a client may change of the calendar server: whose colour each
/// calendar wears.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct CalendarPatch {
    #[ts(type = "Record<string, string>")]
    pub accounts: Option<BTreeMap<String, Uuid>>,
}

impl Validate for CalendarPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        let Some(accounts) = &self.accounts else {
            return Ok(());
        };
        if accounts.len() > 100 {
            return invalid("Postrider colours up to 100 calendars.");
        }
        if accounts
            .keys()
            .any(|href| href.is_empty() || href.len() > 1000)
        {
            return invalid("A calendar's address is missing.");
        }
        Ok(())
    }
}

/// A calendar on the calendar server, as Settings lists it: whose colour
/// it wears unless the person chooses, and its next event, to tell it by.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServerCalendar {
    /// Its address on the server, as [`Calendar::accounts`] knows it.
    pub href: String,
    /// Its name there, else the last part of its address.
    pub name: String,
    /// The account whose colour it wears unless the person chooses: the
    /// one it's named after, else the one at the server's domain, else the
    /// first.
    pub default_account: Option<Uuid>,
    /// Its next event in the coming weeks.
    pub next: Option<Event>,
}

/// The calendar server, as the server keeps it once it signed in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalendarCreate {
    pub url: String,
    pub username: String,
}

impl Validate for CalendarCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        clean_name(&self.username, "the user name").map(|_| ())
    }
}

/// "Add a calendar": checked by signing in before it's kept.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalendarConnect {
    pub url: String,
    pub username: String,
    pub password: String,
}

impl CalendarConnect {
    pub fn cleaned(&self) -> Result<CalendarConnect, ValidationError> {
        let url = self.url.trim();
        let url = if url.contains("://") {
            url.to_string()
        } else {
            format!("https://{url}")
        };
        let url = if url.ends_with('/') {
            url
        } else {
            format!("{url}/")
        };
        let local = ["http://localhost:", "http://127.0.0.1:"]
            .iter()
            .any(|p| url.starts_with(p));
        if !url.starts_with("https://") && !local {
            return Err(ValidationError(
                "A calendar address starts with https://.".into(),
            ));
        }
        let username = clean_name(&self.username, "the user name")?;
        if self.password.is_empty() {
            return Err(ValidationError("The password is missing.".into()));
        }
        Ok(CalendarConnect {
            url,
            username,
            password: self.password.clone(),
        })
    }
}

/// Where an account's mail server stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AccountState {
    /// Signing in, the first time since the app started.
    Connecting,
    /// Signed in and listening for new mail.
    Online,
    /// The server can't be reached; Postrider tries again.
    Offline,
    /// The server said no, as to a wrong password: trying again won't help.
    Refused,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccountStatus {
    pub account_id: Uuid,
    pub state: AccountState,
    /// What went wrong, in the server's words, when offline or refused.
    pub message: Option<String>,
    /// How many changes made here wait for its server, as while it can't
    /// be reached.
    #[ts(type = "number")]
    #[serde(default)]
    pub waiting: i64,
}

/// A mail, as a list shows it: the Inbox, a folder, or what a search found.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MailSummary {
    pub account_id: Uuid,
    /// The folder it's in, by its key ([`Folder::key`]); `None` for a mail
    /// in the Inbox's copy.
    pub folder: Option<String>,
    /// Its IMAP UID in that folder.
    #[ts(type = "number")]
    pub uid: i64,
    /// "Anna Weber", or the address when the mail gives no name.
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    /// The start of the text, on one line.
    pub snippet: String,
    pub date: DateTime<Utc>,
    /// Read (`\Seen`), on the server.
    pub seen: bool,
    /// Flagged (`\Flagged`), to come back to.
    #[serde(default)]
    pub flagged: bool,
    /// Who it went to, in a list of sent mail, where that's what matters.
    pub to_name: Option<String>,
    /// Sent, but waiting for its server to take it (the outbox).
    #[serde(default)]
    pub unsent: bool,
}

/// The unified Inbox: every account's newest mail, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Inbox {
    /// Goes up whenever anything here changes; `GET .../inbox?after=` waits
    /// for it.
    #[ts(type = "number")]
    pub version: i64,
    pub accounts: Vec<AccountStatus>,
    pub mails: Vec<MailSummary>,
    /// The same mail, a row for each conversation, newest first.
    pub conversations: Vec<Conversation>,
}

/// A conversation in the Inbox: its mail, oldest first. The Inbox's, and
/// what you sent and archived of it, by its folder; a mail kept in two
/// places shows once.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Conversation {
    pub mails: Vec<MailSummary>,
}

/// What a folder is for, as its server says (RFC 6154) or its name does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FolderRole {
    Sent,
    Archive,
    Drafts,
    Trash,
    Junk,
    /// Where mail with an unsubscribe link goes, past the Inbox: Postrider's
    /// own, by its name.
    Newsletters,
}

impl FolderRole {
    pub const ALL: [FolderRole; 6] = [
        FolderRole::Sent,
        FolderRole::Archive,
        FolderRole::Drafts,
        FolderRole::Trash,
        FolderRole::Junk,
        FolderRole::Newsletters,
    ];

    /// Its key in addresses, where it stands for each account's own.
    pub fn key(self) -> &'static str {
        match self {
            FolderRole::Sent => "sent",
            FolderRole::Archive => "archive",
            FolderRole::Drafts => "drafts",
            FolderRole::Trash => "trash",
            FolderRole::Junk => "junk",
            FolderRole::Newsletters => "newsletters",
        }
    }
}

/// A folder, once for all the accounts that have one by its name, or for
/// its role: each account's Sent is one Sent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Folder {
    /// What addresses call it: its role's key ("sent"), else its name.
    pub key: String,
    /// "Sent", or the folder's own name: "Rechnungen", "Projects / Layer".
    pub name: String,
    pub role: Option<FolderRole>,
    /// How many of its mails are unread, in every account; in Newsletters,
    /// those that came since you last looked there.
    #[ts(type = "number")]
    pub unread: i64,
    /// How many mails it holds, in every account.
    #[ts(type = "number")]
    pub total: i64,
}

/// Mail read live from the servers: a folder's newest, or what a search
/// found, newest first. An account that couldn't be read says why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Listing {
    pub mails: Vec<MailSummary>,
    pub problems: Vec<AccountStatus>,
}

/// A mail opened from a list: its line in the list, and the whole of it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    #[serde(flatten)]
    #[ts(flatten)]
    pub summary: MailSummary,
    /// Who else it went to (To, then Cc): names, or addresses without one.
    pub to: Vec<String>,
    /// Its text: the sender's, or made from its HTML.
    pub text: String,
    /// Its HTML when it has some, with its inline pictures written in as
    /// `data:` URLs. The page shows it, without scripts, in place of `text`.
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
    /// Who a reply goes to when it isn't the sender: its Reply-To, by name
    /// or address.
    pub reply_to: Option<String>,
    /// Whom R answers, with their addresses: its Reply-To, else its sender.
    pub answer_to: Vec<Party>,
    /// Its To and Cc, with their addresses: whom else A answers.
    pub sent_to: Vec<Party>,
    pub copied: Vec<Party>,
    /// Whether its HTML loads its pictures from the web as it opens: you
    /// said so for its sender, or it's yours. Else they wait to be asked
    /// for, so its sender can't see it was read.
    pub pictures: bool,
}

/// Whether a sender's mail loads its pictures as it opens, from now on.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PictureSender {
    pub address: String,
    pub load: bool,
}

/// The senders whose mail loads its pictures as it opens, by address.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PictureSenders {
    pub senders: Vec<String>,
}

/// Someone a mail names, with their address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Party {
    /// Their name: empty when the mail gives none.
    pub name: String,
    pub address: String,
}

/// A mail you write, as the page sends it: a new one (C), a reply (R, or
/// A to all) or a forward (F).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct Outgoing {
    /// The account it goes from.
    pub from_account: Uuid,
    /// Whom it goes to: addresses, or names with them ("Anna Weber
    /// <anna@example.test>"), with commas between. Cc and Bcc the same.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub text: String,
    /// The mail it answers: it goes in that mail's thread, quoting it below.
    pub answers: Option<MailRef>,
    /// The mail it forwards: below what you wrote, with the files it keeps.
    pub forwards: Option<MailRef>,
    /// The files it takes along, in their order.
    pub files: Vec<Attach>,
    /// The draft it was kept as last: keeping it again takes that one's
    /// place, and sending it deletes it. What's below your words comes from
    /// there while it's in Drafts.
    pub draft: Option<DraftRef>,
}

/// A file a mail you write takes along.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Attach {
    /// One of the files of the mail it forwards, by its place among them.
    Kept { index: u32 },
    /// A file you added, by the id adding it gave.
    Added { upload: Uuid },
    /// One of the files of its draft, by its place among them.
    Drafted { index: u32 },
}

/// A draft in an account's Drafts, by its Message-ID: each time it's kept,
/// it's kept anew, with an id of its own, in place of the one before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DraftRef {
    /// The account whose Drafts holds it: the one it goes from.
    pub account_id: Uuid,
    pub message_id: String,
}

/// A draft opened from Drafts, to write on: as the page shows a mail you
/// write.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, optional_fields)]
pub struct Draft {
    pub draft: DraftRef,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    /// Your words: what it quotes or forwards below them stays as it is.
    pub text: String,
    /// Its files, in their order, as [`Attach::Drafted`] names them.
    pub files: Vec<Attachment>,
    pub answers: Option<MailRef>,
    pub forwards: Option<MailRef>,
}

impl Outgoing {
    /// Whether it fits in a mail at all.
    pub fn check_size(&self) -> Result<(), ValidationError> {
        if self.text.len() > 1_000_000
            || self.subject.len() > 1_000
            || [&self.to, &self.cc, &self.bcc]
                .iter()
                .any(|f| f.len() > 10_000)
            || self.files.len() > 100
        {
            return Err(ValidationError(
                "A mail holds up to a million characters and 100 files.".into(),
            ));
        }
        if self.answers.is_some() && self.forwards.is_some() {
            return Err(ValidationError(
                "A mail answers another or forwards it, not both.".into(),
            ));
        }
        Ok(())
    }
}

impl Validate for Outgoing {
    fn validate(&self) -> Result<(), ValidationError> {
        self.check_size()?;
        if [&self.to, &self.cc, &self.bcc]
            .iter()
            .all(|f| f.trim().is_empty())
        {
            return Err(ValidationError("Say whom it goes to.".into()));
        }
        if self.text.trim().is_empty() && self.forwards.is_none() && self.files.is_empty() {
            return Err(ValidationError("Write something first.".into()));
        }
        Ok(())
    }
}

/// A file to add to a mail you write, as the page read it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct NewUpload {
    pub name: String,
    /// Its type, as the browser says: `application/pdf`, or empty.
    pub content_type: String,
    /// Its bytes, in base64.
    pub data: String,
}

/// A file added, kept in memory until its mail goes or a day has passed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Upload {
    pub id: Uuid,
    pub name: String,
    #[ts(type = "number")]
    pub size: i64,
}

/// More professional: what you wrote, to be written again as a mail.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct Rewrite {
    pub text: String,
    /// The account it goes from.
    pub from_account: Option<Uuid>,
    /// Whom it goes to, as the page names them, for the greeting.
    pub to: Option<String>,
    pub subject: Option<String>,
    /// The mail it answers, read for what it's about and how they speak.
    pub replying: Option<MailRef>,
}

impl Validate for Rewrite {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.text.trim().is_empty() {
            return Err(ValidationError("Write something first.".into()));
        }
        if self.text.len() > 20_000 {
            return Err(ValidationError(
                "That's too long to rewrite: up to 20,000 characters.".into(),
            ));
        }
        Ok(())
    }
}

/// A mail, by where it is: its account, its folder and its UID there.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MailRef {
    pub account_id: Uuid,
    /// The folder's key: the Inbox when left out.
    pub folder: Option<String>,
    #[ts(type = "number")]
    pub uid: i64,
}

/// What you wrote, written again.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Rewritten {
    pub text: String,
}

/// A file that came with a mail.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Attachment {
    /// "Invoice.pdf", or "Attachment" when the mail gives no name.
    pub name: String,
    /// In bytes.
    #[ts(type = "number")]
    pub size: i64,
}

/// An event in a day, from the calendar server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Event {
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// All day: `start` and `end` are the day's bounds.
    pub all_day: bool,
    /// The calendar it's in, as Radicale names it.
    pub calendar: String,
    /// The data hue of the account the calendar belongs to.
    pub hue: i32,
}

/// One day's events, from every calendar on the server.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Day {
    pub events: Vec<Event>,
    /// Why the calendar couldn't be read, when it couldn't.
    pub error: Option<String>,
    /// When it couldn't: when `events` were read, the last time it could.
    pub read_at: Option<DateTime<Utc>>,
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError(message.into()))
}

fn account_name(value: &str) -> Result<String, ValidationError> {
    clean_name(value, "An account's name")
}

fn hue(value: i32) -> Result<(), ValidationError> {
    if !(1..=HUES).contains(&value) {
        return invalid(format!("A hue is 1 to {HUES}."));
    }
    Ok(())
}

fn port(value: i32) -> Result<i32, ValidationError> {
    if !(1..=65535).contains(&value) {
        return invalid("A port is 1 to 65535.");
    }
    Ok(value)
}

/// A host name: letters, digits, dots and dashes.
fn host(value: &str) -> Result<String, ValidationError> {
    let value = value.trim().to_lowercase();
    let valid = !value.is_empty()
        && value.len() <= 253
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !value.starts_with(['.', '-']);
    if !valid {
        return invalid("The server is a host name, like styx.uberspace.de.");
    }
    Ok(value)
}

/// What you ask Postrider in the field at the foot of every view, or tell
/// it to do.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Asked {
    pub text: String,
}

/// What Postrider answered, and what it changed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Answer {
    /// What Undo names to put back what it changed; `None` when it changed
    /// nothing.
    pub id: Option<Uuid>,
    /// Its words, with names in `**bold**`.
    pub text: String,
    /// What the rules it made or changed look for:
    /// "From notifications@github.com".
    pub chips: Vec<String>,
    /// The folder it sorted into, to open.
    pub folder: Option<FolderLink>,
}

/// A folder to open: its key and its name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FolderLink {
    pub key: String,
    pub name: String,
}

/// A folder's unread mail, marked read: how many, and the id Undo takes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MarkedRead {
    pub id: Uuid,
    #[ts(type = "number")]
    pub count: i64,
}

/// A folder's new name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RenameFolder {
    pub name: String,
}

/// A mail marked read or unread, flagged or not: what's left out stays
/// as it is.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MarkMail {
    #[ts(optional)]
    pub seen: Option<bool>,
    #[ts(optional)]
    pub flagged: Option<bool>,
}

/// Where a mail goes: a folder's key, or the name of a new one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MoveMail {
    pub to: String,
}

/// A sender whose mail carries an unsubscribe link, as the Newsletters page
/// shows it: how often it writes, and how much of that you open.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Newsletter {
    pub account_id: Uuid,
    /// "Zalando", or the address when its mail gives no name: as its
    /// newest mail says.
    pub name: String,
    /// Every name its mail gives, the newest first: LinkedIn's
    /// messaging-digest-noreply@ writes as each person who wrote you.
    pub names: Vec<String>,
    /// The address its newest mail came from: its view's, by which the
    /// page knows it.
    pub address: String,
    /// Every address it writes from, the newest first: one sender writes
    /// as the same name from news.example.com and mail.example.com.
    pub addresses: Vec<String>,
    /// How many of its mails are in Newsletters, and how many of those you
    /// opened, from all of them.
    pub sent: u32,
    pub opened: u32,
    /// How many of them came in the last 30 days.
    pub recent: u32,
    /// How many it sends a week, since its first mail there.
    pub per_week: f64,
    /// Its newest mail in Newsletters, to open.
    #[ts(type = "number")]
    pub newest_uid: i64,
    pub newest: DateTime<Utc>,
}

/// The Newsletters page: every account's, least read first. An account
/// that couldn't be read says why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Newsletters {
    pub newsletters: Vec<Newsletter>,
    pub problems: Vec<AccountStatus>,
    /// The accounts whose Inbox newsletters aren't here yet: the first time
    /// an account comes online, Postrider moves them.
    pub setting_aside: Vec<SettingAside>,
    /// The accounts whose mail Postrider couldn't sort again, by what makes
    /// a newsletter or a notice now, and why. It tries again the next time
    /// each connects.
    pub not_sorted: Vec<SettingAside>,
    /// The senders the newsletters' rule names, since their mail doesn't
    /// say it went to many and Claude Code found it does.
    pub named: Vec<NewsletterRef>,
}

/// An account whose newsletters are still being set aside, or sorted
/// again, and why that failed, when it did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingAside {
    pub account_id: Uuid,
    pub problem: Option<String>,
}

/// A sender in an account's Newsletters.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewsletterRef {
    pub account_id: Uuid,
    pub address: String,
}

/// The senders to unsubscribe from, or to keep in the Inbox.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Senders {
    pub senders: Vec<NewsletterRef>,
}

impl Validate for Senders {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.senders.is_empty() {
            return Err(ValidationError("Name a sender.".into()));
        }
        if self.senders.len() > 100 {
            return Err(ValidationError(
                "That's more than 100 senders at once.".into(),
            ));
        }
        if self.senders.iter().any(|s| s.address.trim().is_empty()) {
            return Err(ValidationError("A sender has no address.".into()));
        }
        Ok(())
    }
}

/// What unsubscribing did, sender by sender. The requests it sends go 10
/// seconds later, unless Undo comes first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Unsubscribed {
    /// What Undo and the requests' outcome name.
    pub id: Uuid,
    pub senders: Vec<Unsubscribing>,
    /// Whether a request waits to go.
    pub waiting: bool,
}

/// One sender unsubscribed from: its mail archived, and how it takes
/// unsubscribing.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Unsubscribing {
    pub account_id: Uuid,
    pub address: String,
    pub archived: u32,
    /// A request to stop goes to it: one click, or a mail.
    pub asked: bool,
    /// Its page, when it takes unsubscribing only there: to open.
    pub page: Option<String>,
}

/// How the requests went, once they did: those that failed, and why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnsubscribeSent {
    pub failed: Vec<SenderProblem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SenderProblem {
    pub account_id: Uuid,
    pub address: String,
    pub message: String,
}

/// What keeping senders in the Inbox did: the rule that keeps them there,
/// and their mail moved back.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeptInInbox {
    pub id: Uuid,
    pub moved: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_goes_below_the_text() {
        assert_eq!(
            signed("Thanks.\n\n", "Michael\n"),
            "Thanks.\n\n-- \nMichael"
        );
        assert_eq!(signed("", "Michael"), "-- \nMichael");
        assert_eq!(signed("Thanks.", "  \n"), "Thanks.");
        let long = "x".repeat(SIGNATURE_CHARS + 1);
        let patch = MailAccountPatch {
            signature: Some(long),
            ..Default::default()
        };
        assert!(patch.validate().is_err());
    }

    fn connect(address: &str, host: &str) -> MailAccountConnect {
        MailAccountConnect {
            name: " Lazarski ".into(),
            address: address.into(),
            host: host.into(),
            port: None,
            username: None,
            password: "secret".into(),
        }
    }

    #[test]
    fn a_new_account_signs_in_with_its_address_on_993() {
        let cleaned = connect("Michael@Lazarski.test", " Styx.Uberspace.de")
            .cleaned()
            .unwrap();
        assert_eq!(cleaned.name, "Lazarski");
        assert_eq!(cleaned.address, "michael@lazarski.test");
        assert_eq!(cleaned.host, "styx.uberspace.de");
        assert_eq!(cleaned.port, Some(993));
        assert_eq!(cleaned.username.as_deref(), Some("michael@lazarski.test"));
    }

    #[test]
    fn a_new_account_needs_a_host_and_a_password() {
        assert!(
            connect("michael@lazarski.test", "imap://x")
                .cleaned()
                .is_err()
        );
        let mut no_password = connect("michael@lazarski.test", "styx.uberspace.de");
        no_password.password.clear();
        assert!(no_password.cleaned().is_err());
    }

    #[test]
    fn a_calendar_is_https_unless_on_this_mac() {
        let calendar = |url: &str| CalendarConnect {
            url: url.into(),
            username: "michael".into(),
            password: "secret".into(),
        };
        assert_eq!(
            calendar("calendar.lazarski.test").cleaned().unwrap().url,
            "https://calendar.lazarski.test/"
        );
        assert!(calendar("http://calendar.lazarski.test").cleaned().is_err());
        assert!(calendar("http://127.0.0.1:5232").cleaned().is_ok());
    }
}

/// Where a parcel is, as the newest mail about it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ParcelState {
    /// Handed to the carrier, or on its way: what a tracking number says
    /// when its mail says nothing more.
    Shipped,
    /// With the driver, coming today.
    OutForDelivery,
    /// Waiting to be picked up: a shop, a locker.
    PickUp,
    Delivered,
}

/// A parcel a mail gave a tracking number for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Parcel {
    /// The tracking number, without spaces.
    pub number: String,
    /// "DHL", "DPD", "Hermes"…, when the mail or the number tells.
    pub carrier: Option<String>,
    /// Where to follow it: the mail's own link, else the carrier's page,
    /// else a page that finds the carrier itself.
    pub track_url: String,
    pub state: ParcelState,
    /// The newest mail that names it: who sent it, and when.
    pub mail: MailSummary,
}

/// `GET /api/w/{workspace}/parcels`: the parcels the mail on this Mac
/// names, the ones still coming first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Parcels {
    pub parcels: Vec<Parcel>,
}
