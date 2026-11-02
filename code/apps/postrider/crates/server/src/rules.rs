//! Sorting rules: mail from a sender, to an address, with words in its
//! subject or from a mailing list goes to a folder. They live on each
//! account's mail server, in Postrider's own Sieve script ([`SCRIPT`]), so
//! the server sorts while the Mac is shut and every mail app sees the same
//! folders. Each rule is written with a comment that says it again in JSON
//! (`# rule {…}`), which is how Postrider reads its rules back: nothing
//! about them is kept on the Mac.

use std::fmt::Write as _;

use postrider_domain::MailAccount;
use serde::{Deserialize, Serialize};

use crate::imap::{self, Failure, Place};
use crate::mail::Mail;
use crate::sieve::Sieve;

/// Postrider's script on each account's server.
pub const SCRIPT: &str = "postrider";

/// What the server runs when the account ran a script of its own before:
/// that one first, then Postrider's.
pub const BOTH: &str = "postrider-and-yours";

const HEADER: &str = "# The mail you told Postrider to sort, and where it goes. Postrider\n\
# writes this script again each time: tell it what to change.\n";

/// Mail that matches any of `when` goes to `folder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// The folder as Postrider shows it: "Code".
    pub folder: String,
    /// Its path on this account's server: "INBOX.Code".
    pub path: String,
    pub when: Vec<When>,
    /// The newsletters' rule's senders that write to many without saying
    /// so, as Claude Code found ([`crate::judging`]): their mail goes there
    /// too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub senders: Vec<String>,
}

impl Rule {
    /// Where its test goes in the script: 0 for yours, 1 to keep senders in
    /// the Inbox, 2 for newsletters.
    fn place(&self) -> u8 {
        if self.when.contains(&When::Unsubscribe) {
            2
        } else if self.path == "INBOX" {
            1
        } else {
            0
        }
    }

    /// The rule that puts newsletters in `newsletters`.
    pub fn newsletters(newsletters: &Place) -> Rule {
        Rule {
            folder: newsletters.name.clone(),
            path: newsletters.path.clone(),
            when: vec![When::Unsubscribe],
            senders: Vec::new(),
        }
    }

    /// Whether it's the rule for newsletters.
    pub fn is_newsletters(&self) -> bool {
        self.when.contains(&When::Unsubscribe)
    }

    /// Its tests in Sieve, any of which files a mail there.
    fn tests(&self, bodies: bool) -> String {
        self.when
            .iter()
            .map(|when| match when {
                When::Unsubscribe => crate::newsletters::test(bodies, &self.senders),
                when => when.test(bodies),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What a rule looks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// Sent from this address, or from anyone at this domain.
    From(String),
    /// Sent to (or copied to) this address, or anyone at this domain.
    To(String),
    /// These words in the subject.
    Subject(String),
    /// From this mailing list, by its List-Id.
    List(String),
    /// With a way to unsubscribe, sent to many as its header says, and
    /// not by a machine by itself: a newsletter ([`crate::newsletters`]).
    Unsubscribe,
}

impl When {
    /// As a chip shows it: "From notifications@github.com".
    pub fn chip(&self) -> String {
        match self {
            When::From(who) => format!("From {who}"),
            When::To(who) => format!("To {who}"),
            When::Subject(words) => format!("Subject has “{words}”"),
            When::List(list) => format!("List {list}"),
            When::Unsubscribe => "Is a newsletter".into(),
        }
    }

    /// Its test in Sieve; `bodies` when the server reads a mail's text
    /// (`body`, RFC 5173).
    pub(crate) fn test(&self, bodies: bool) -> String {
        let address = |headers: &str, who: &str| {
            let part = if who.contains('@') { "" } else { ":domain " };
            format!("address {part}:is {headers} {}", sieve_string(who))
        };
        match self {
            When::From(who) => address("\"from\"", who),
            When::To(who) => address("[\"to\", \"cc\"]", who),
            When::Subject(words) => format!("header :contains \"subject\" {}", sieve_string(words)),
            When::List(list) => format!("header :contains \"list-id\" {}", sieve_string(list)),
            When::Unsubscribe => crate::newsletters::test(bodies, &[]),
        }
    }

    /// Its key in an IMAP SEARCH.
    fn search(&self) -> String {
        let quoted =
            |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
        match self {
            When::From(who) => format!("FROM {}", quoted(who)),
            When::To(who) => format!("OR TO {0} CC {0}", quoted(who)),
            When::Subject(words) => format!("SUBJECT {}", quoted(words)),
            When::List(list) => format!("HEADER List-Id {}", quoted(list)),
            // An empty string is in every mail that has the header. It finds
            // what offers to unsubscribe; whether that went to many is read
            // in its header after.
            When::Unsubscribe => format!(
                "OR HEADER List-Unsubscribe \"\" ({})",
                crate::newsletters::from_a_service_search()
            ),
        }
    }

    /// Whether it looks for the same as `other`, in any case.
    pub fn same(&self, other: &When) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.text().eq_ignore_ascii_case(other.text())
    }

    pub fn text(&self) -> &str {
        match self {
            When::From(text) | When::To(text) | When::Subject(text) | When::List(text) => text,
            When::Unsubscribe => "",
        }
    }
}

/// Postrider's script for these rules. Sieve stops at the first that
/// matches, so yours come first, then the senders kept in the Inbox, then
/// newsletters: a newsletter you sort into a folder goes there. A server
/// that reads mail's text (`can_read_bodies`) finds newsletters by it too.
pub fn script(rules: &[Rule], can_read_bodies: bool) -> String {
    let bodies = can_read_bodies && rules.iter().any(Rule::is_newsletters);
    let mut script = String::from(HEADER);
    script.push_str(if bodies {
        "require [\"fileinto\", \"body\"];\n"
    } else {
        "require [\"fileinto\"];\n"
    });
    let mut ordered: Vec<&Rule> = rules.iter().filter(|rule| !rule.when.is_empty()).collect();
    ordered.sort_by_key(|rule| rule.place());
    for rule in ordered {
        let said = serde_json::to_string(rule).expect("a rule is JSON");
        let tests = rule.tests(bodies);
        // Sieve names a folder in UTF-8, where IMAP has modified UTF-7.
        let _ = write!(
            script,
            "\n# rule {said}\nif anyof ({tests}) {{\n  fileinto {};\n  stop;\n}}\n",
            sieve_string(&crate::imap::folder_name(&rule.path))
        );
    }
    script
}

/// The rules in Postrider's script, by the comments it wrote them with.
pub fn rules_in(script: &str) -> Vec<Rule> {
    script
        .lines()
        .filter_map(|line| line.strip_prefix("# rule "))
        .filter_map(|said| serde_json::from_str(said).ok())
        .collect()
}

/// The script that runs theirs, then Postrider's.
fn running_both(theirs: &str) -> String {
    format!(
        "# What the server runs: your own rules first, then the mail you told\n\
         # Postrider to sort.\n\
         require [\"include\"];\n\
         include :personal {};\n\
         include :personal {};\n",
        sieve_string(theirs),
        sieve_string(SCRIPT)
    )
}

/// A string as Sieve quotes it, on one line.
pub(crate) fn sieve_string(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// An IMAP SEARCH for mail that matches any of `when`, with the CHARSET it
/// needs in front.
pub(crate) fn search(when: &[When], first: &str) -> String {
    let keys: Vec<String> = when.iter().map(When::search).collect();
    let query = format!("{first}{}", any(&keys));
    if when.iter().all(|w| w.text().is_ascii()) {
        query
    } else {
        format!("CHARSET UTF-8 {query}")
    }
}

/// An IMAP SEARCH key that any of `keys` matches; every mail for none.
pub(crate) fn any(keys: &[String]) -> String {
    match keys {
        [] => "ALL".into(),
        [key] => key.clone(),
        [key, rest @ ..] => format!("OR {key} ({})", any(rest)),
    }
}

/// UIDs as a set IMAP reads, runs joined: `3:7,9`.
pub(crate) fn uid_set(uids: &[u32]) -> String {
    let mut uids = uids.to_vec();
    uids.sort_unstable();
    let mut set = String::new();
    let mut i = 0;
    while i < uids.len() {
        let start = uids[i];
        while i + 1 < uids.len() && uids[i + 1] == uids[i] + 1 {
            i += 1;
        }
        if !set.is_empty() {
            set.push(',');
        }
        if uids[i] == start {
            let _ = write!(set, "{start}");
        } else {
            let _ = write!(set, "{start}:{}", uids[i]);
        }
        i += 1;
    }
    set
}

/// A folder an account renamed: where it was, where it is, and the
/// account's rules before.
#[derive(Debug, Clone)]
pub struct Renamed {
    pub account: MailAccount,
    pub from: Place,
    pub to: Place,
    pub rules: Vec<Rule>,
}

/// Mail an answer moved, to move back to `back` on Undo: what in `path`
/// matches `when`, from UID `since` on, and below `until` when it moved
/// only what it named.
#[derive(Debug, Clone)]
pub struct Moved {
    pub path: String,
    pub back: String,
    pub since: u32,
    pub until: Option<u32>,
    pub when: Vec<When>,
    pub count: usize,
}

impl Mail {
    /// Signs in to the account's sorting rules, with its password.
    async fn sieve(&self, account: &MailAccount) -> Result<Sieve, Failure> {
        let password = self.password(account).await?;
        Sieve::sign_in(&account.host, &account.username, &password).await
    }

    /// The rules Postrider keeps on the account's server.
    pub(crate) async fn rules(&self, account: &MailAccount) -> Result<Vec<Rule>, Failure> {
        let mut sieve = self.sieve(account).await?;
        let script = sieve.get(SCRIPT).await;
        sieve.log_out().await;
        Ok(script?.as_deref().map(rules_in).unwrap_or_default())
    }

    /// Writes the account's rules, and makes sure the server runs them:
    /// alone, or after the script it ran before.
    pub(crate) async fn put_rules(
        &self,
        account: &MailAccount,
        rules: &[Rule],
    ) -> Result<(), Failure> {
        let mut sieve = self.sieve(account).await?;
        let put = async {
            sieve.put(SCRIPT, &script(rules, sieve.can("body"))).await?;
            let scripts = sieve.scripts().await?;
            match scripts.iter().find(|(_, active)| *active) {
                None => sieve.activate(SCRIPT).await,
                Some((name, _)) if name == SCRIPT || name == BOTH => Ok(()),
                Some((theirs, _)) => {
                    sieve.put(BOTH, &running_both(theirs)).await?;
                    sieve.activate(BOTH).await
                }
            }
        }
        .await;
        sieve.log_out().await;
        put
    }

    /// The account's folder called `name`, made when it has none; and
    /// whether it was.
    pub(crate) async fn folder_named(
        &self,
        account: &MailAccount,
        name: &str,
    ) -> Result<(Place, bool), Failure> {
        if let Some(place) = self.place(account, name).await? {
            return Ok((place, false));
        }
        let made = self
            .on_server(account, |session| {
                let name = name.to_string();
                Box::pin(async move { imap::make_folder(session, &name, None).await })
            })
            .await?;
        self.lock_places()
            .entry(account.id)
            .or_default()
            .push(made.clone());
        Ok((made, true))
    }

    /// Renames the account's folder, and with it the rules that sort into
    /// it or a folder inside it: Postrider's on the server, and those it
    /// sorts by reading. Its rules as they were come back with it.
    pub(crate) async fn rename_folder(
        &self,
        account: &MailAccount,
        from: &Place,
        name: &str,
    ) -> Result<Renamed, Failure> {
        let rules = self.rules(account).await?;
        let rename = |from: Place, name: String| {
            self.on_server(account, move |session| {
                let (from, name) = (from.clone(), name.clone());
                Box::pin(async move { imap::rename_folder(session, &from, &name).await })
            })
        };
        let to = rename(from.clone(), name.to_string()).await?;
        // Folders inside it moved with it: they're listed again.
        self.lock_places().remove(&account.id);
        self.lock_lines()
            .retain(|(id, path), _| *id != account.id || moved_to(path, from, &to).is_none());
        let mut changed = rules.clone();
        for rule in &mut changed {
            if let Some(path) = moved_to(&rule.path, from, &to) {
                rule.path = path;
                rule.folder = renamed(&rule.folder, from, &to);
            }
        }
        if changed != rules
            && let Err(failure) = self.put_rules(account, &changed).await
        {
            // Back where it was, where its rules look for it.
            let _ = rename(to.clone(), leaf(&from.name).to_string()).await;
            return Err(failure);
        }
        let reading = self.reading_rules(account.id).await?;
        for rule in reading {
            if let Some(path) = moved_to(&rule.path, from, &to) {
                sqlx::query("UPDATE reading_rules SET folder = $2, path = $3 WHERE id = $1")
                    .bind(rule.id)
                    .bind(renamed(&rule.folder, from, &to))
                    .bind(path)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(Renamed {
            account: account.clone(),
            from: from.clone(),
            to,
            rules,
        })
    }

    /// Moves what's in the account's Inbox that matches `when` to `path`.
    pub(crate) async fn move_matching(
        &self,
        account: &MailAccount,
        when: &[When],
        path: &str,
    ) -> Result<Moved, Failure> {
        let query = search(when, "");
        let (since, count) = self
            .on_server(account, |session| {
                let (query, path) = (query.clone(), path.to_string());
                Box::pin(async move {
                    let since = session
                        .status(&path, "(UIDNEXT)")
                        .await?
                        .uid_next
                        .unwrap_or(1);
                    session.select("INBOX").await?;
                    let uids: Vec<u32> = session.uid_search(&query).await?.into_iter().collect();
                    if !uids.is_empty() {
                        session.uid_mv(uid_set(&uids), &path).await?;
                    }
                    Ok((since, uids.len()))
                })
            })
            .await?;
        Ok(Moved {
            path: path.to_string(),
            back: "INBOX".into(),
            since,
            until: None,
            when: when.to_vec(),
            count,
        })
    }

    /// Moves these mails from the account's Inbox to `path`: those of them
    /// still there. Undo moves back whatever came to `path` since.
    pub(crate) async fn move_uids(
        &self,
        account: &MailAccount,
        uids: &[u32],
        path: &str,
    ) -> Result<Moved, Failure> {
        if uids.is_empty() {
            return Ok(Moved {
                path: path.to_string(),
                back: "INBOX".into(),
                since: 0,
                until: None,
                when: Vec::new(),
                count: 0,
            });
        }
        let set = uid_set(uids);
        let (since, count) = self
            .on_server(account, |session| {
                let (set, path) = (set.clone(), path.to_string());
                Box::pin(async move {
                    let since = session
                        .status(&path, "(UIDNEXT)")
                        .await?
                        .uid_next
                        .unwrap_or(1);
                    session.select("INBOX").await?;
                    let there: Vec<u32> = session
                        .uid_search(format!("UID {set}"))
                        .await?
                        .into_iter()
                        .collect();
                    if !there.is_empty() {
                        session.uid_mv(uid_set(&there), &path).await?;
                    }
                    Ok((since, there.len()))
                })
            })
            .await?;
        Ok(Moved {
            path: path.to_string(),
            back: "INBOX".into(),
            since,
            until: None,
            when: Vec::new(),
            count,
        })
    }

    /// Moves mail from the folder `from` to `to`, as the keys do: these
    /// UIDs, those of them still there, or with none, what matches `when`.
    /// Undo moves back only what it moved, to where it was.
    pub(crate) async fn move_out(
        &self,
        account: &MailAccount,
        from: &str,
        uids: &[u32],
        when: &[When],
        to: &str,
    ) -> Result<Moved, Failure> {
        let mut moved = Moved {
            path: to.to_string(),
            back: from.to_string(),
            since: 0,
            until: Some(0),
            when: Vec::new(),
            count: 0,
        };
        // Neither would be every mail there.
        if uids.is_empty() && when.is_empty() {
            return Ok(moved);
        }
        let query = if uids.is_empty() {
            search(when, "")
        } else {
            format!("UID {}", uid_set(uids))
        };
        let (since, until, gone) = self
            .on_server(account, |session| {
                let (query, from, to) = (query.clone(), from.to_string(), to.to_string());
                Box::pin(async move {
                    let since = session
                        .status(&to, "(UIDNEXT)")
                        .await?
                        .uid_next
                        .unwrap_or(1);
                    session.select(&from).await?;
                    let there: Vec<u32> = session.uid_search(&query).await?.into_iter().collect();
                    if there.is_empty() {
                        return Ok((since, since, there));
                    }
                    session.uid_mv(uid_set(&there), &to).await?;
                    // What came to it meanwhile is what moved.
                    let until = session
                        .status(&to, "(UIDNEXT)")
                        .await?
                        .uid_next
                        .unwrap_or(u32::MAX);
                    Ok((since, until, there))
                })
            })
            .await?;
        if from == "INBOX" {
            for uid in &gone {
                self.forget_inbox_line(account.id, i64::from(*uid)).await?;
            }
        }
        if let Some((_, lines)) = self.lock_lines().get_mut(&(account.id, from.to_string())) {
            for uid in &gone {
                lines.remove(&i64::from(*uid));
            }
        }
        moved.since = since;
        moved.until = Some(until);
        moved.count = gone.len();
        Ok(moved)
    }

    /// Moves mail an answer moved back where it was: what came to its
    /// folder since and matches what it looked for; what it moved alone,
    /// when it knows.
    pub(crate) async fn move_back(
        &self,
        account: &MailAccount,
        moved: &Moved,
    ) -> Result<(), Failure> {
        if moved.count == 0 {
            return Ok(());
        }
        let (since, until) = (moved.since, moved.until);
        let range = match until {
            Some(until) => format!("UID {since}:{} ", until.saturating_sub(1).max(since)),
            None => format!("UID {since}:* "),
        };
        let query = search(&moved.when, &range);
        self.on_server(account, |session| {
            let (query, path, back) = (query.clone(), moved.path.clone(), moved.back.clone());
            Box::pin(async move {
                session.select(&path).await?;
                // `n:*` takes in the newest mail even when its UID is below n.
                let uids: Vec<u32> = session
                    .uid_search(&query)
                    .await?
                    .into_iter()
                    .filter(|uid| *uid >= since && until.is_none_or(|until| *uid < until))
                    .collect();
                if !uids.is_empty() {
                    session.uid_mv(uid_set(&uids), &back).await?;
                }
                Ok(())
            })
        })
        .await?;
        self.lock_lines().remove(&(account.id, moved.path.clone()));
        Ok(())
    }

    /// Takes away a folder Postrider made, if nothing's in it.
    pub(crate) async fn remove_if_empty(
        &self,
        account: &MailAccount,
        path: &str,
    ) -> Result<(), Failure> {
        let removed = self
            .on_server(account, |session| {
                let path = path.to_string();
                Box::pin(async move {
                    // Dovecot hangs up on a connection whose folder is
                    // deleted under it, as Undo's moving back left it.
                    session.select("INBOX").await?;
                    let held = session.status(&path, "(MESSAGES)").await?.exists;
                    if held > 0 {
                        return Ok(false);
                    }
                    let _ = session.unsubscribe(&path).await;
                    session.delete(&path).await?;
                    Ok(true)
                })
            })
            .await?;
        if removed && let Some(places) = self.lock_places().get_mut(&account.id) {
            places.retain(|place| place.path != path);
        }
        Ok(())
    }

    /// The senders in the Inbox whose name, address or mail has these words,
    /// with how many mails each sent: what a rule's addresses are found by.
    pub(crate) async fn senders(
        &self,
        accounts: &[MailAccount],
        words: &str,
    ) -> Result<Vec<Sender>, sqlx::Error> {
        let ids: Vec<_> = accounts.iter().map(|a| a.id).collect();
        let pattern = format!("%{}%", words.trim().replace(['%', '_'], ""));
        sqlx::query_as::<_, Sender>(SENDERS)
            .bind(&ids)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
    }
}

const SENDERS: &str = "\
SELECT lower(from_address) AS address, max(from_name) AS name, count(*) AS mails,
       (array_agg(subject ORDER BY date DESC))[1] AS latest,
       array_agg(DISTINCT account_id) AS accounts
FROM inbox_mails
WHERE account_id = ANY($1)
  AND (from_name ILIKE $2 OR from_address ILIKE $2 OR subject ILIKE $2 OR snippet ILIKE $2)
GROUP BY lower(from_address)
ORDER BY count(*) DESC
LIMIT 20";

/// A sender in the Inbox.
#[derive(Debug, sqlx::FromRow)]
pub struct Sender {
    pub address: String,
    pub name: String,
    pub mails: i64,
    /// The subject of their newest mail.
    pub latest: String,
    /// The accounts they wrote to.
    pub accounts: Vec<uuid::Uuid>,
}

/// The names the Inbox knows these addresses by.
pub(crate) async fn names_of(
    mail: &Mail,
    accounts: &[MailAccount],
    addresses: &[String],
) -> Vec<(String, String)> {
    let ids: Vec<_> = accounts.iter().map(|a| a.id).collect();
    let lower: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
    sqlx::query_as::<_, (String, String)>(NAMES)
        .bind(&ids)
        .bind(&lower)
        .fetch_all(&mail.pool)
        .await
        .unwrap_or_default()
}

const NAMES: &str = "\
SELECT DISTINCT ON (lower(from_address)) lower(from_address), from_name
FROM inbox_mails
WHERE account_id = ANY($1) AND lower(from_address) = ANY($2)
  AND from_name <> '' AND lower(from_name) <> lower(from_address)
ORDER BY lower(from_address), date DESC";

/// The accounts whose Inbox has mail from these addresses, or from anyone
/// at these domains.
pub(crate) async fn written_to(
    mail: &Mail,
    accounts: &[MailAccount],
    senders: &[String],
) -> Vec<uuid::Uuid> {
    let ids: Vec<_> = accounts.iter().map(|a| a.id).collect();
    let lower: Vec<String> = senders.iter().map(|a| a.to_lowercase()).collect();
    sqlx::query_scalar(WRITTEN_TO)
        .bind(&ids)
        .bind(&lower)
        .fetch_all(&mail.pool)
        .await
        .unwrap_or_default()
}

const WRITTEN_TO: &str = "\
SELECT DISTINCT account_id FROM inbox_mails
WHERE account_id = ANY($1)
  AND (lower(from_address) = ANY($2) OR split_part(lower(from_address), '@', 2) = ANY($2))";

/// Every rule's folders and what they look for, for the prompt.
pub fn described(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return "no rules".into();
    }
    rules
        .iter()
        .map(|rule| {
            let when: Vec<String> = rule.when.iter().map(When::chip).collect();
            format!("{} ← {}", rule.folder, when.join(", "))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Whether two folder names are the same folder.
/// The last part of a folder's name: "Layer" of "Projects / Layer".
pub fn leaf(name: &str) -> &str {
    name.rsplit(" / ").next().unwrap_or(name)
}

/// Where `path` is once `from` is renamed to `to`: it, or a folder inside it.
fn moved_to(path: &str, from: &Place, to: &Place) -> Option<String> {
    let rest = path.strip_prefix(from.path.as_str())?;
    (rest.is_empty() || rest.starts_with(['.', '/'])).then(|| format!("{}{rest}", to.path))
}

/// A folder's name once `from` is renamed to `to`.
fn renamed(name: &str, from: &Place, to: &Place) -> String {
    match name.strip_prefix(from.name.as_str()) {
        Some(rest) => format!("{}{rest}", to.name),
        None => to.name.clone(),
    }
}

pub fn same_folder(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> Rule {
        Rule {
            folder: "Code".into(),
            path: "INBOX.Code".into(),
            when: vec![
                When::From("notifications@github.com".into()),
                When::From("codevillain.eu".into()),
                When::Subject("\"PR\"".into()),
            ],
            senders: Vec::new(),
        }
    }

    #[test]
    fn a_rule_is_written_and_read_back() {
        let script = script(&[code()], true);
        assert!(script.contains("require [\"fileinto\"];\n"));
        assert!(script.contains(
            "if anyof (address :is \"from\" \"notifications@github.com\", \
             address :domain :is \"from\" \"codevillain.eu\", \
             header :contains \"subject\" \"\\\"PR\\\"\") {\n  fileinto \"INBOX.Code\";\n  stop;\n}"
        ));
        assert_eq!(rules_in(&script), vec![code()]);
        assert_eq!(rules_in(HEADER), vec![]);
    }

    #[test]
    fn a_rule_files_into_the_folder_by_its_utf8_name() {
        let fees = Rule {
            folder: "Gebühren & Rechnungen".into(),
            path: "Geb&APw-hren &- Rechnungen".into(),
            when: vec![When::From("netcup.test".into())],
            senders: Vec::new(),
        };
        let script = script(std::slice::from_ref(&fees), true);
        assert!(script.contains("fileinto \"Gebühren & Rechnungen\";"));
        assert_eq!(rules_in(&script), vec![fees]);
    }

    #[test]
    fn a_search_finds_any_of_them() {
        assert_eq!(
            search(&code().when, ""),
            "OR FROM \"notifications@github.com\" (OR FROM \"codevillain.eu\" (SUBJECT \"\\\"PR\\\"\"))"
        );
        assert_eq!(
            search(&[When::Subject("Grüße".into())], "UID 5:* "),
            "CHARSET UTF-8 UID 5:* SUBJECT \"Grüße\""
        );
    }

    #[test]
    fn newsletters_come_after_yours_and_the_senders_kept() {
        let newsletters = Rule::newsletters(&Place {
            path: "INBOX.Newsletters".into(),
            name: "Newsletters".into(),
            role: None,
        });
        let kept = Rule {
            folder: "Inbox".into(),
            path: "INBOX".into(),
            when: vec![When::From("news@bahn.test".into())],
            senders: Vec::new(),
        };
        let script = script(&[newsletters.clone(), kept.clone(), code()], true);
        assert_eq!(rules_in(&script), vec![code(), kept, newsletters.clone()]);
        assert!(script.contains("require [\"fileinto\", \"body\"];\n"));
        assert!(script.contains(
            "if anyof (allof (anyof (exists \"list-unsubscribe\", \
             allof (anyof (exists \"x-msfbl\", exists \"x-sg-eid\", "
        ));
        assert!(script.contains(
            "exists \"x-csa-complaints\"), body :text :contains [\"unsubscribe\", \"abmelden\", "
        ));
        assert!(script.contains(
            "\"avsluta prenumeration\"])), anyof (exists \"list-id\", \
             header :is \"precedence\" [\"bulk\", \"list\"], \
             header :contains \"x-pm-message-stream\" \"broadcast\", \
             allof (exists \"x-mc-user\", not exists \"x-mandrill-user\")), \
             not allof (exists \"auto-submitted\", not header :is \"auto-submitted\" \"no\", \
             not exists \"list-unsubscribe-post\", \
             not header :is \"precedence\" [\"bulk\", \"list\"]))) {\n  \
             fileinto \"INBOX.Newsletters\";\n  stop;\n}"
        ));
        // A server that doesn't read mail's text goes by the header alone.
        let header_only = super::script(std::slice::from_ref(&newsletters), false);
        assert!(header_only.contains("require [\"fileinto\"];\n"));
        assert!(
            header_only.contains(
                "if anyof (allof (exists \"list-unsubscribe\", anyof (exists \"list-id\", "
            )
        );
        // The senders Claude Code found writing to many go in its test, and
        // come back with it.
        let named = Rule {
            senders: vec!["no-reply@m.coursera.test".into()],
            ..newsletters
        };
        let script = super::script(std::slice::from_ref(&named), false);
        assert!(script.contains(
            "not exists \"x-mandrill-user\"), address :is \"from\" [\"no-reply@m.coursera.test\"])"
        ));
        assert_eq!(rules_in(&script), vec![named]);
        let found = search(&[When::Unsubscribe], "");
        assert!(found.starts_with(
            "OR HEADER List-Unsubscribe \"\" (OR HEADER X-MSFBL \"\" (OR HEADER X-SG-EID \"\" ("
        ));
        assert!(found.ends_with(
            "(HEADER X-CSA-Complaints \"\"))))))) OR BODY \"unsubscribe\" (OR BODY \"abmelden\" (\
             OR BODY \"abbestellen\" (OR BODY \"avregistrera\" (OR BODY \"avprenumerera\" (\
             BODY \"avsluta prenumeration\"))))))"
        ));
    }

    #[test]
    fn uids_are_joined_in_runs() {
        assert_eq!(uid_set(&[9, 3, 4, 5, 7, 6, 12]), "3:7,9,12");
        assert_eq!(uid_set(&[1]), "1");
    }
}
