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
# writes this script again each time: tell it what to change.\n\
require [\"fileinto\"];\n";

/// Mail that matches any of `when` goes to `folder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// The folder as Postrider shows it: "Code".
    pub folder: String,
    /// Its path on this account's server: "INBOX.Code".
    pub path: String,
    pub when: Vec<When>,
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
}

impl When {
    /// As a chip shows it: "From notifications@github.com".
    pub fn chip(&self) -> String {
        match self {
            When::From(who) => format!("From {who}"),
            When::To(who) => format!("To {who}"),
            When::Subject(words) => format!("Subject has “{words}”"),
            When::List(list) => format!("List {list}"),
        }
    }

    /// Its test in Sieve.
    fn test(&self) -> String {
        let address = |headers: &str, who: &str| {
            let part = if who.contains('@') { "" } else { ":domain " };
            format!("address {part}:is {headers} {}", sieve_string(who))
        };
        match self {
            When::From(who) => address("\"from\"", who),
            When::To(who) => address("[\"to\", \"cc\"]", who),
            When::Subject(words) => format!("header :contains \"subject\" {}", sieve_string(words)),
            When::List(list) => format!("header :contains \"list-id\" {}", sieve_string(list)),
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
        }
    }

    /// Whether it looks for the same as `other`, in any case.
    pub fn same(&self, other: &When) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.text().eq_ignore_ascii_case(other.text())
    }

    pub fn text(&self) -> &str {
        let (When::From(text) | When::To(text) | When::Subject(text) | When::List(text)) = self;
        text
    }
}

/// Postrider's script for these rules.
pub fn script(rules: &[Rule]) -> String {
    let mut script = String::from(HEADER);
    for rule in rules.iter().filter(|rule| !rule.when.is_empty()) {
        let said = serde_json::to_string(rule).expect("a rule is JSON");
        let tests = rule
            .when
            .iter()
            .map(When::test)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            script,
            "\n# rule {said}\nif anyof ({tests}) {{\n  fileinto {};\n  stop;\n}}\n",
            sieve_string(&rule.path)
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
fn sieve_string(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// An IMAP SEARCH for mail that matches any of `when`, with the CHARSET it
/// needs in front.
fn search(when: &[When], first: &str) -> String {
    fn any(keys: &[String]) -> String {
        match keys {
            [] => "ALL".into(),
            [key] => key.clone(),
            [key, rest @ ..] => format!("OR {key} ({})", any(rest)),
        }
    }
    let keys: Vec<String> = when.iter().map(When::search).collect();
    let query = format!("{first}{}", any(&keys));
    if when.iter().all(|w| w.text().is_ascii()) {
        query
    } else {
        format!("CHARSET UTF-8 {query}")
    }
}

/// UIDs as a set IMAP reads, runs joined: `3:7,9`.
fn uid_set(uids: &[u32]) -> String {
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

/// Mail a rule moved from the Inbox, to move back on Undo: what in `path`
/// matches `when`, from UID `since` on.
#[derive(Debug, Clone)]
pub struct Moved {
    pub path: String,
    pub since: u32,
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
            sieve.put(SCRIPT, &script(rules)).await?;
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
            since,
            when: when.to_vec(),
            count,
        })
    }

    /// Moves mail a rule moved back to the Inbox: what came to its folder
    /// since and matches it.
    pub(crate) async fn move_back(
        &self,
        account: &MailAccount,
        moved: &Moved,
    ) -> Result<(), Failure> {
        if moved.count == 0 {
            return Ok(());
        }
        let query = search(&moved.when, &format!("UID {}:* ", moved.since));
        let since = moved.since;
        self.on_server(account, |session| {
            let (query, path) = (query.clone(), moved.path.clone());
            Box::pin(async move {
                session.select(&path).await?;
                // `n:*` takes in the newest mail even when its UID is below n.
                let uids: Vec<u32> = session
                    .uid_search(&query)
                    .await?
                    .into_iter()
                    .filter(|uid| *uid >= since)
                    .collect();
                if !uids.is_empty() {
                    session.uid_mv(uid_set(&uids), "INBOX").await?;
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
        }
    }

    #[test]
    fn a_rule_is_written_and_read_back() {
        let script = script(&[code()]);
        assert!(script.contains(
            "if anyof (address :is \"from\" \"notifications@github.com\", \
             address :domain :is \"from\" \"codevillain.eu\", \
             header :contains \"subject\" \"\\\"PR\\\"\") {\n  fileinto \"INBOX.Code\";\n  stop;\n}"
        ));
        assert_eq!(rules_in(&script), vec![code()]);
        assert_eq!(rules_in(HEADER), vec![]);
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
    fn uids_are_joined_in_runs() {
        assert_eq!(uid_set(&[9, 3, 4, 5, 7, 6, 12]), "3:7,9,12");
        assert_eq!(uid_set(&[1]), "1");
    }
}
