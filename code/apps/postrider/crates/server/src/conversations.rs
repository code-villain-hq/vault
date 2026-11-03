//! Conversations in the Inbox: its mail with what answers it and what it
//! answers, from the Inbox, Sent and Archive, so the Inbox shows a row for
//! each and the open mail shows all of it, in order.
//!
//! Mail joins first by its header: its Message-ID, In-Reply-To and
//! References. Helpdesks often break that chain and move the subject's tag
//! ("[Pen Store] Re:" against "Re: [Pen Store]"), so mail also joins when
//! its subject is the same without its Re:, Fwd:, Sv:, AW: and [tags] (as
//! [`mail_parser::thread_name`] reads it), it's with the same other party,
//! within [`WITHIN_DAYS`] of each other, and one answers another or the
//! subjects as written differ: never on the subject alone. The other party
//! is the sender of mail to you, and whom it asks answers to go to, and
//! whom you wrote to in yours: by their domain, unless many share it
//! ([`SHARED`]), where it's by their address.
//!
//! What you sent and archived is kept in Postgres (`filed_mails`), as far
//! back as each account's Inbox goes and [`BEFORE_DAYS`] more, read again
//! whenever an Inbox changes and every [`EVERY`] besides.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use postrider_domain::{AccountState, Conversation, FolderRole, MailAccount, MailSummary};
use uuid::Uuid;

use crate::imap;
use crate::mail::Mail;

/// How far apart, in days, mail may be to join on its subject.
const WITHIN_DAYS: i64 = 30;

/// How much further back than an Inbox's oldest mail, in days, its
/// account's Sent and Archive are kept.
const BEFORE_DAYS: i64 = 30;

/// The furthest back they're kept, in days.
const FURTHEST_DAYS: i64 = 365;

/// The most of each folder kept, newest first.
const AT_MOST: usize = 1000;

/// How often Sent and Archive are read again, beside each Inbox change.
const EVERY: Duration = Duration::from_secs(5 * 60);

/// How long to wait after a change, for the next to come with it.
const SETTLE: Duration = Duration::from_secs(5);

/// The folders whose mail joins the Inbox's conversations.
const FILED: [FolderRole; 2] = [FolderRole::Sent, FolderRole::Archive];

/// Domains many people share, where the domain says nothing of who writes.
const SHARED: &[&str] = &[
    "aol.com",
    "fastmail.com",
    "freenet.de",
    "gmail.com",
    "gmx.at",
    "gmx.ch",
    "gmx.de",
    "gmx.net",
    "googlemail.com",
    "hotmail.com",
    "icloud.com",
    "live.com",
    "mac.com",
    "mail.ru",
    "mailbox.org",
    "me.com",
    "msn.com",
    "outlook.com",
    "pm.me",
    "posteo.de",
    "proton.me",
    "protonmail.com",
    "t-online.de",
    "web.de",
    "yahoo.com",
    "yandex.ru",
];

/// What joins a mail to others, as it's kept: its Message-ID, the ones it
/// answers, whom it went to and whom answers go to.
pub(crate) struct Joins {
    pub message_id: String,
    pub refs: Vec<String>,
    pub to_addresses: Vec<String>,
    pub reply_to: String,
}

/// Keeps every account's Sent and Archive for conversations for as long as
/// Postrider runs: whenever an Inbox changes, and every [`EVERY`] besides.
pub async fn keep(mail: Arc<Mail>) {
    loop {
        let mut version = mail.version();
        if keep_all(&mail).await {
            mail.bump();
            version = mail.version();
        }
        let _ = tokio::time::timeout(EVERY, mail.changed_from(version)).await;
        tokio::time::sleep(SETTLE).await;
    }
}

/// Keeps what's new in each account's Sent and Archive whose server is
/// reached: one that refused isn't asked again until it's fixed. Returns
/// whether anything kept changed.
async fn keep_all(mail: &Mail) -> bool {
    let accounts: Vec<MailAccount> =
        match sqlx::query_scalar("SELECT to_jsonb(a.*) FROM mail_accounts a ORDER BY position")
            .fetch_all(&mail.pool)
            .await
        {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row: serde_json::Value| serde_json::from_value(row).ok())
                .collect(),
            Err(e) => {
                tracing::warn!("couldn't read the accounts for conversations: {e}");
                return false;
            }
        };
    let mut changed = false;
    for account in accounts {
        if mail.state_of(account.id) != Some(AccountState::Online) {
            continue;
        }
        match mail.keep_filed(&account).await {
            Ok(kept) => changed |= kept,
            Err(problem) => tracing::info!(
                address = %account.address,
                "couldn't read Sent and Archive for conversations: {problem}"
            ),
        }
    }
    changed
}

impl Mail {
    /// Brings what's kept of an account's Sent and Archive up to date.
    /// Returns whether it changed.
    async fn keep_filed(&self, account: &MailAccount) -> Result<bool, String> {
        let oldest: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT min(date) FROM inbox_mails WHERE account_id = $1")
                .bind(account.id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
        let now = Utc::now();
        let since = (oldest.unwrap_or(now).min(now) - TimeDelta::days(BEFORE_DAYS))
            .max(now - TimeDelta::days(FURTHEST_DAYS))
            .date_naive();
        let places = self.places(account).await.map_err(|f| f.message())?;
        let mut changed = false;
        let mut read = 0;
        for role in FILED {
            let key = role.key();
            let Some(place) = places.iter().find(|place| place.role == Some(role)) else {
                let gone =
                    sqlx::query("DELETE FROM filed_mails WHERE account_id = $1 AND folder = $2")
                        .bind(account.id)
                        .bind(key)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| e.to_string())?;
                changed |= gone.rows_affected() > 0;
                continue;
            };
            let kept: Vec<(i64, i64)> = sqlx::query_as(
                "SELECT uid, uid_validity FROM filed_mails WHERE account_id = $1 AND folder = $2",
            )
            .bind(account.id)
            .bind(key)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
            let validity = kept.first().map_or(-1, |(_, validity)| *validity);
            let known: HashSet<i64> = kept.into_iter().map(|(uid, _)| uid).collect();
            let filed = self
                .on_server(account, |session| {
                    let (path, known) = (place.path.clone(), known.clone());
                    Box::pin(async move {
                        imap::filed(session, &path, since, AT_MOST, &known, validity).await
                    })
                })
                .await
                .map_err(|f| f.message())?;
            let gone = sqlx::query(
                "DELETE FROM filed_mails WHERE account_id = $1 AND folder = $2 \
                 AND (uid_validity <> $3 OR NOT uid = ANY($4))",
            )
            .bind(account.id)
            .bind(key)
            .bind(filed.validity)
            .bind(&filed.there)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
            changed |= gone.rows_affected() > 0;
            for line in filed.lines {
                let summary = line.summary;
                sqlx::query(
                    "INSERT INTO filed_mails (account_id, folder, uid_validity, uid, message_id, \
                     refs, from_name, from_address, to_name, to_addresses, reply_to, subject, \
                     snippet, date, seen) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
                     ON CONFLICT (account_id, folder, uid) DO NOTHING",
                )
                .bind(account.id)
                .bind(key)
                .bind(filed.validity)
                .bind(line.uid)
                .bind(&summary.message_id)
                .bind(&summary.threading.refs)
                .bind(&summary.from_name)
                .bind(&summary.from_address)
                .bind(&summary.to_name)
                .bind(&summary.threading.to_addresses)
                .bind(&summary.threading.reply_to)
                .bind(&summary.subject)
                .bind(&summary.snippet)
                .bind(summary.date)
                .bind(line.seen)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
                changed = true;
                read += 1;
            }
        }
        if changed {
            let (sent, archived, twice): (i64, i64, i64) = sqlx::query_as(
                "SELECT count(*) FILTER (WHERE folder = 'sent'), \
                 count(*) FILTER (WHERE folder = 'archive'), \
                 (SELECT count(*) FROM (SELECT 1 FROM filed_mails \
                   WHERE account_id = $1 AND folder = 'sent' AND message_id <> '' \
                   GROUP BY message_id HAVING count(*) > 1) d) \
                 FROM filed_mails WHERE account_id = $1",
            )
            .bind(account.id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
            tracing::info!(
                address = %account.address,
                "kept {sent} mails of Sent and {archived} of Archive since {since} for \
                 conversations, {read} read now; {twice} in Sent are there twice by Message-ID"
            );
        }
        Ok(changed)
    }

    /// What's kept of the workspace's Sent and Archive.
    pub(crate) async fn filed(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<(MailSummary, Joins)>, sqlx::Error> {
        type Row = (
            Uuid,
            String,
            i64,
            String,
            Vec<String>,
            String,
            String,
            String,
            Vec<String>,
            String,
            String,
            String,
            DateTime<Utc>,
            bool,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT f.account_id, f.folder, f.uid, f.message_id, f.refs, f.from_name, \
             f.from_address, f.to_name, f.to_addresses, f.reply_to, f.subject, f.snippet, \
             f.date, f.seen FROM filed_mails f JOIN mail_accounts a ON a.id = f.account_id \
             WHERE a.workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    account_id,
                    folder,
                    uid,
                    message_id,
                    refs,
                    from_name,
                    from_address,
                    to_name,
                    to_addresses,
                    reply_to,
                    subject,
                    snippet,
                    date,
                    seen,
                )| {
                    // A list of sent mail names whom it went to.
                    let to_name = (folder == FolderRole::Sent.key()).then_some(to_name);
                    let summary = MailSummary {
                        account_id,
                        folder: Some(folder),
                        uid,
                        from_name,
                        from_address,
                        subject,
                        snippet,
                        date,
                        seen,
                        to_name,
                    };
                    let joins = Joins {
                        message_id,
                        refs,
                        to_addresses,
                        reply_to,
                    };
                    (summary, joins)
                },
            )
            .collect())
    }
}

/// The Inbox's conversations, newest first: each Inbox mail with what it
/// joins of the others and of `filed`, oldest first. A mail kept in two
/// places shows once, the Inbox's first. `own` are the workspace's
/// addresses, in lower case: mail from them is yours.
pub(crate) fn conversations(
    inbox: Vec<(MailSummary, Joins)>,
    filed: Vec<(MailSummary, Joins)>,
    own: &HashSet<String>,
) -> Vec<Conversation> {
    let mut ids: HashSet<String> = inbox
        .iter()
        .map(|(_, joins)| joins.message_id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    let mut mails = inbox;
    for (summary, joins) in filed {
        if joins.message_id.is_empty() || ids.insert(joins.message_id.clone()) {
            mails.push((summary, joins));
        }
    }
    let threads: Vec<Thread> = mails
        .iter()
        .map(|(summary, joins)| Thread::of(summary, joins, own))
        .collect();
    let mut joined: HashMap<usize, Vec<MailSummary>> = HashMap::new();
    for ((summary, _), root) in mails.into_iter().zip(group(&threads)) {
        joined.entry(root).or_default().push(summary);
    }
    let mut conversations: Vec<Conversation> = joined
        .into_values()
        .filter(|mails| mails.iter().any(|mail| mail.folder.is_none()))
        .map(|mut mails| {
            mails.sort_by_key(|mail| (mail.date, mail.uid));
            Conversation { mails }
        })
        .collect();
    let newest = |conversation: &Conversation| {
        conversation
            .mails
            .last()
            .map(|mail| (mail.date, mail.uid, mail.account_id))
    };
    conversations.sort_by_key(|conversation| std::cmp::Reverse(newest(conversation)));
    conversations
}

/// What [`group`] reads of a mail.
#[derive(Debug, Default)]
pub(crate) struct Thread {
    pub message_id: String,
    /// The Message-IDs it answers.
    pub refs: Vec<String>,
    /// Its subject as written, on one line, in lower case.
    pub subject: String,
    /// Its subject without Re:, Fwd: and [tags], in lower case.
    pub name: String,
    /// Whom it's with, by [`party`].
    pub parties: Vec<String>,
    pub date: DateTime<Utc>,
}

impl Thread {
    fn of(summary: &MailSummary, joins: &Joins, own: &HashSet<String>) -> Thread {
        let from = summary.from_address.to_lowercase();
        let yours =
            summary.folder.as_deref() == Some(FolderRole::Sent.key()) || own.contains(&from);
        let with: Vec<&str> = if yours {
            joins.to_addresses.iter().map(String::as_str).collect()
        } else {
            vec![from.as_str(), joins.reply_to.as_str()]
        };
        let mut parties: Vec<String> = Vec::new();
        for address in with {
            if address.is_empty() || own.contains(address) {
                continue;
            }
            let party = party(address);
            if !parties.contains(&party) {
                parties.push(party);
            }
        }
        Thread {
            message_id: joins.message_id.clone(),
            refs: joins.refs.clone(),
            subject: words(&summary.subject),
            name: words(mail_parser::thread_name(&summary.subject)),
            parties,
            date: summary.date,
        }
    }
}

/// Text in lower case, its words one space apart.
fn words(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Who an address is, for joining on the subject: its domain, unless many
/// share it, then the address.
fn party(address: &str) -> String {
    match address.rsplit_once('@') {
        Some((_, domain)) if !SHARED.contains(&domain) => format!("@{domain}"),
        _ => address.to_string(),
    }
}

/// Which conversation each mail is in, by one of its mails' places: mail
/// in the same conversation has the same.
pub(crate) fn group(mails: &[Thread]) -> Vec<usize> {
    let mut sets = Sets::new(mails.len());
    // A Message-ID joins every mail that is it or answers it, there or not.
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for (at, mail) in mails.iter().enumerate() {
        for id in std::iter::once(&mail.message_id).chain(&mail.refs) {
            if id.is_empty() {
                continue;
            }
            let node = *ids.entry(id).or_insert_with(|| sets.add());
            sets.join(at, node);
        }
    }
    let mut named: HashMap<&str, Vec<usize>> = HashMap::new();
    for (at, mail) in mails.iter().enumerate() {
        if !mail.name.is_empty() && !mail.parties.is_empty() {
            named.entry(&mail.name).or_default().push(at);
        }
    }
    for same in named.values() {
        for (n, &a) in same.iter().enumerate() {
            for &b in &same[n + 1..] {
                if by_subject(&mails[a], &mails[b]) {
                    sets.join(a, b);
                }
            }
        }
    }
    (0..mails.len()).map(|at| sets.root(at)).collect()
}

/// Whether two mails of the same subject, without its Re: and tags, are
/// one conversation: with the same party, near in time, and one answers or
/// the subjects as written differ. Two notices alike from a shop stay two.
fn by_subject(a: &Thread, b: &Thread) -> bool {
    a.parties.iter().any(|party| b.parties.contains(party))
        && (a.date - b.date).abs() <= TimeDelta::days(WITHIN_DAYS)
        && (a.subject != b.subject || !a.refs.is_empty() || !b.refs.is_empty())
}

/// Disjoint sets, by place, that grow.
struct Sets(Vec<usize>);

impl Sets {
    fn new(len: usize) -> Sets {
        Sets((0..len).collect())
    }

    fn add(&mut self) -> usize {
        self.0.push(self.0.len());
        self.0.len() - 1
    }

    fn root(&mut self, mut at: usize) -> usize {
        while self.0[at] != at {
            self.0[at] = self.0[self.0[at]];
            at = self.0[at];
        }
        at
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.0[b.max(a)] = a.min(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "michael@lazarski.test";
    const SHOP: &str = "kontakt@penstore.test";

    /// October `day`, at `hour`.
    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        format!("2026-10-{day:02}T{hour:02}:00:00Z")
            .parse()
            .unwrap()
    }

    fn mail(
        folder: Option<&str>,
        uid: i64,
        from: &str,
        subject: &str,
        when: DateTime<Utc>,
    ) -> MailSummary {
        MailSummary {
            account_id: Uuid::nil(),
            folder: folder.map(str::to_string),
            uid,
            from_name: from.into(),
            from_address: from.into(),
            subject: subject.into(),
            snippet: String::new(),
            date: when,
            seen: true,
            to_name: None,
        }
    }

    fn joins(id: &str, refs: &[&str], to: &[&str]) -> Joins {
        Joins {
            message_id: id.into(),
            refs: refs.iter().map(|r| r.to_string()).collect(),
            to_addresses: to.iter().map(|t| t.to_string()).collect(),
            reply_to: String::new(),
        }
    }

    fn own() -> HashSet<String> {
        HashSet::from([ME.to_string()])
    }

    /// Each conversation's mail, by folder and UID.
    fn shown(conversations: &[Conversation]) -> Vec<Vec<(Option<&str>, i64)>> {
        conversations
            .iter()
            .map(|c| {
                c.mails
                    .iter()
                    .map(|m| (m.folder.as_deref(), m.uid))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_subject_reads_without_its_answers_and_tags() {
        let name = |subject: &str| words(mail_parser::thread_name(subject));
        assert_eq!(name("[Pen Store] Re: Platinum Preppy"), "platinum preppy");
        assert_eq!(name("Re: [Pen Store] Platinum Preppy"), "platinum preppy");
        assert_eq!(name("Sv: AW: Fwd:  Platinum   Preppy"), "platinum preppy");
        assert_eq!(name("Platinum Preppy"), "platinum preppy");
    }

    #[test]
    fn a_helpdesk_that_breaks_the_chain_is_one_conversation() {
        let asked = "Platinum Preppy Black Edition i Extra Fine";
        let inbox = vec![
            (
                mail(
                    None,
                    7,
                    SHOP,
                    &format!("[Pen Store] Re: {asked}"),
                    at(26, 9),
                ),
                joins("a1@helpdesk.test", &[], &[ME]),
            ),
            (
                mail(None, 8, "news@penstore.test", "Autumn offers", at(26, 8)),
                joins("n1@penstore.test", &[], &[ME]),
            ),
            (
                mail(
                    None,
                    9,
                    SHOP,
                    &format!("Re: [Pen Store] {asked}"),
                    at(26, 15),
                ),
                joins("a2@helpdesk.test", &[], &[ME]),
            ),
        ];
        let filed = vec![
            (
                mail(Some("sent"), 3, ME, asked, at(25, 9)),
                joins("s1@lazarski.test", &[], &[SHOP]),
            ),
            // Sent shows it twice: one mail, by its Message-ID.
            (
                mail(Some("sent"), 4, ME, asked, at(25, 9)),
                joins("s1@lazarski.test", &[], &[SHOP]),
            ),
            (
                mail(
                    Some("sent"),
                    5,
                    ME,
                    &format!("Re: [Pen Store] {asked}"),
                    at(26, 12),
                ),
                joins("s2@lazarski.test", &["a1@helpdesk.test"], &[SHOP]),
            ),
        ];
        assert_eq!(
            shown(&conversations(inbox, filed, &own())),
            [
                vec![(Some("sent"), 3), (None, 7), (Some("sent"), 5), (None, 9)],
                vec![(None, 8)],
            ]
        );
    }

    #[test]
    fn an_answer_joins_by_its_header_whatever_its_subject() {
        let inbox = vec![
            (
                mail(None, 1, "anna@weber.test", "Lunch?", at(1, 9)),
                joins("q@weber.test", &[], &[ME]),
            ),
            (
                mail(
                    None,
                    2,
                    "ben@other.test",
                    "Something else entirely",
                    at(2, 9),
                ),
                joins("r@other.test", &["q@weber.test"], &[ME]),
            ),
        ];
        assert_eq!(
            shown(&conversations(inbox, vec![], &own())),
            [vec![(None, 1), (None, 2)]]
        );
    }

    #[test]
    fn the_subject_alone_joins_nothing() {
        let inbox = vec![
            // Alike notices from one shop stay two.
            (
                mail(None, 1, SHOP, "Your order has shipped", at(1, 9)),
                joins("o1@penstore.test", &[], &[ME]),
            ),
            (
                mail(None, 2, SHOP, "Your order has shipped", at(3, 9)),
                joins("o2@penstore.test", &[], &[ME]),
            ),
            // Two people at one mail provider aren't one party.
            (
                mail(None, 3, "anna@gmail.com", "Re: Hello", at(4, 9)),
                joins("h1@gmail.com", &[], &[ME]),
            ),
            // Weeks apart is another conversation.
            (
                mail(
                    None,
                    4,
                    SHOP,
                    "Re: Your order has shipped",
                    at(1, 9) + TimeDelta::days(45),
                ),
                joins("o3@penstore.test", &[], &[ME]),
            ),
        ];
        let filed = vec![
            (
                mail(Some("sent"), 5, ME, "Hello", at(4, 8)),
                joins("h0@lazarski.test", &[], &["ben@gmail.com"]),
            ),
            (
                mail(
                    Some("sent"),
                    6,
                    ME,
                    "Your order has shipped",
                    at(1, 9) - TimeDelta::days(60),
                ),
                joins("o0@lazarski.test", &[], &[SHOP]),
            ),
        ];
        let conversations = conversations(inbox, filed, &own());
        let shown = shown(&conversations);
        assert_eq!(shown.len(), 4, "{shown:?}");
        assert!(
            shown
                .iter()
                .all(|c| c.iter().all(|(folder, _)| folder.is_none())),
            "{shown:?}"
        );
    }
}
