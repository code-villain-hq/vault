//! The field at the foot of every view: you ask Postrider something, or tell
//! it what to do, and Claude Code on this Mac answers
//! (`studio/design/behaviour.md`, "Claude"), as you're signed in to it, in
//! an empty folder of its own, with Postrider's tools and nothing else:
//! finding senders and mail, reading a mail, and sorting, with rules on the
//! mail server (`rules.rs`) or by reading on this Mac (`reading.rs`). It
//! calls them at [`MCP_PATH`] with a token that lasts the one question.
//! What an answer changed, Undo puts back. Postrider runs only on this Mac,
//! so there's no key and no question handed over.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Local, Utc};
use code_villain_claude::mcp::{self, Called, Tools};
use code_villain_claude::{App, Failure, Question, bearer};
use mail_parser::MessageParser;
use postrider_domain::{Answer, Asked, FolderLink, MailAccount, MailSummary};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::mail::Mail;
use crate::message;
use crate::reading::ReadingRule;
use crate::rules::{self, Moved, Rule, When, same_folder};
use crate::state::AppState;

pub const POSTRIDER: App = App {
    key: "postrider",
    name: "Postrider",
    version: env!("CARGO_PKG_VERSION"),
};

/// Where Claude Code reaches a question's tools.
pub const MCP_PATH: &str = "/api/ai/mcp";

/// How long an answer may take: sorting moves mail on every account.
const TIMEOUT: Duration = Duration::from_secs(240);

const MAX_TURNS: u32 = 20;

/// How much of a mail read_mail gives.
const READ_AT_MOST: usize = 12_000;

/// The answers whose changes Undo can still put back, by their id.
pub type Answers = Arc<Mutex<HashMap<Uuid, Done>>>;

/// A question Claude Code is answering: whose, and what it changed so far.
pub struct Asking {
    mail: Arc<Mail>,
    workspace_id: Uuid,
    accounts: Vec<MailAccount>,
    done: tokio::sync::Mutex<Done>,
}

/// What an answer changed, to put back.
#[derive(Default)]
pub struct Done {
    workspace_id: Uuid,
    /// Each account's rules before the answer changed them.
    before: Vec<(MailAccount, Vec<Rule>)>,
    /// The mail it moved out of the Inbox.
    moved: Vec<(MailAccount, Moved)>,
    /// The folders it made, by their path.
    made: Vec<(MailAccount, String)>,
    /// The reading rules it made, and those it took away.
    reading_made: Vec<Uuid>,
    reading_removed: Vec<ReadingRule>,
    chips: Vec<String>,
    folder: Option<FolderLink>,
}

impl Done {
    fn changed(&self) -> bool {
        !self.before.is_empty()
            || !self.moved.is_empty()
            || !self.made.is_empty()
            || !self.reading_made.is_empty()
            || !self.reading_removed.is_empty()
    }
}

const TOOLS: &[&str] = &[
    "find_senders",
    "find_mail",
    "read_mail",
    "sort_into",
    "sort_by_reading",
    "stop_sorting",
];

fn definitions() -> Vec<Value> {
    let addresses = json!({ "type": "array", "items": { "type": "string" } });
    vec![
        json!({
            "name": "find_senders",
            "description": "The senders in the Inbox whose name or address, or whose mail, has \
                these words, with how many mails each sent, the subject of their newest and \
                the accounts they wrote to. A rule needs addresses: \"github\" finds \
                notifications@github.com.",
            "input_schema": {
                "type": "object",
                "properties": { "words": { "type": "string" } },
                "required": ["words"],
            },
        }),
        json!({
            "name": "find_mail",
            "description": "Mail with every one of these words in it, in the Inbox and every \
                folder but Drafts, Trash and Junk, newest first: who sent it and when, its \
                subject and start, and the account, folder and uid read_mail reads it by.",
            "input_schema": {
                "type": "object",
                "properties": { "words": { "type": "string" } },
                "required": ["words"],
            },
        }),
        json!({
            "name": "read_mail",
            "description": "One mail's text, by what find_mail said: its account's address, \
                its folder (none for the Inbox) and its uid. Reading it doesn't mark it read.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "account": { "type": "string" },
                    "folder": { "type": "string" },
                    "uid": { "type": "integer" },
                },
                "required": ["account", "uid"],
            },
        }),
        json!({
            "name": "sort_into",
            "description": "Sorts mail into a folder with a rule on the account's mail server, \
                so it sorts while this Mac is shut and every mail app sees it, and moves what's \
                in the Inbox already. The folder is made where there's none. A rule the folder \
                has already gets these added. from and to take addresses, or a domain for \
                anyone there; subject takes words in the subject; list takes a mailing list's \
                List-Id. accounts (addresses) says where; without it, the accounts whose Inbox \
                has mail from these senders, else every account. It answers with what it did.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string" },
                    "from": addresses,
                    "to": addresses,
                    "subject": addresses,
                    "list": addresses,
                    "accounts": addresses,
                },
                "required": ["folder"],
            },
        }),
        json!({
            "name": "sort_by_reading",
            "description": "Sorts mail into a folder by reading it, for what no rule on the \
                mail server can tell, like \"anything that looks like an invoice\": Postrider \
                reads the mail that comes to the Inbox from now on and moves what fits, on this \
                Mac, while it's open. looks_like finishes \"mail that looks like …\", in \
                English: \"an invoice\". mails are what's in the Inbox already that fits, by \
                what find_mail said (folder none): they move now. accounts (addresses) says \
                where; without it, every account. The folder is made where there's none. It \
                answers with what it did.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string" },
                    "looks_like": { "type": "string" },
                    "mails": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "account": { "type": "string" },
                                "uid": { "type": "integer" },
                            },
                            "required": ["account", "uid"],
                        },
                    },
                    "accounts": addresses,
                },
                "required": ["folder", "looks_like"],
            },
        }),
        json!({
            "name": "stop_sorting",
            "description": "Stops sorting into a folder: takes these from its rule, or, with \
                reading true, only its sorting by reading, or else the whole rule and its \
                sorting by reading, in every account. What's in the folder stays.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string" },
                    "from": addresses,
                    "to": addresses,
                    "subject": addresses,
                    "list": addresses,
                    "reading": { "type": "boolean" },
                },
                "required": ["folder"],
            },
        }),
    ]
}

impl Tools for Asking {
    fn list(&self) -> Vec<Value> {
        definitions().iter().map(mcp::listed).collect()
    }

    async fn call(&self, name: &str, arguments: Value) -> Called {
        let done = match name {
            "find_senders" => self.find_senders(&arguments).await,
            "find_mail" => self.find_mail(&arguments).await,
            "read_mail" => self.read_mail(&arguments).await,
            "sort_into" => self.sort_into(&arguments).await,
            "sort_by_reading" => self.sort_by_reading(&arguments).await,
            "stop_sorting" => self.stop_sorting(&arguments).await,
            _ => Err(format!("Postrider has no tool called {name}.")),
        };
        match done {
            Ok(text) => Called::ok(text),
            Err(text) => Called::failed(text),
        }
    }
}

impl Asking {
    async fn find_senders(&self, arguments: &Value) -> Result<String, String> {
        let words = text(arguments, "words")?;
        let senders = self
            .mail
            .senders(&self.accounts, words)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "couldn't read the senders");
                "Couldn't read the Inbox.".to_string()
            })?;
        if senders.is_empty() {
            return Ok(format!("No sender in the Inbox matches “{words}”."));
        }
        let mut found = String::new();
        for sender in senders {
            let to: Vec<&str> = self
                .accounts
                .iter()
                .filter(|a| sender.accounts.contains(&a.id))
                .map(|a| a.address.as_str())
                .collect();
            let _ = writeln!(
                found,
                "{} <{}>: {} mails to {}, newest “{}”",
                sender.name,
                sender.address,
                sender.mails,
                to.join(" and "),
                sender.latest
            );
        }
        Ok(found)
    }

    async fn find_mail(&self, arguments: &Value) -> Result<String, String> {
        let words = text(arguments, "words")?;
        let listing = self
            .mail
            .search(self.workspace_id, words)
            .await
            .map_err(|e| e.message)?;
        let mut found = String::new();
        for mail in listing.mails.iter().take(20) {
            let account = self.account_by_id(mail.account_id);
            let _ = writeln!(
                found,
                "{} · {} <{}> · {} — {} [account {}, folder {}, uid {}]",
                mail.date.with_timezone(&Local).format("%a %e %b %Y %H:%M"),
                mail.from_name,
                mail.from_address,
                mail.subject,
                mail.snippet,
                account.map_or("?", |a| a.address.as_str()),
                mail.folder.as_deref().unwrap_or("none"),
                mail.uid
            );
        }
        for problem in &listing.problems {
            if let Some(said) = &problem.message {
                let _ = writeln!(found, "Couldn't search one account: {said}");
            }
        }
        if found.is_empty() {
            return Ok(format!("No mail has “{words}” in it."));
        }
        Ok(found)
    }

    async fn read_mail(&self, arguments: &Value) -> Result<String, String> {
        let address = text(arguments, "account")?;
        let account = self.account_by_address(address)?;
        let uid = arguments["uid"]
            .as_i64()
            .ok_or("Say which mail, by its uid.")?;
        let folder = arguments["folder"]
            .as_str()
            .map(str::trim)
            .filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case("none"));
        let (_, raw) = self
            .mail
            .source_of(account, folder, uid)
            .await
            .map_err(|e| e.message)?;
        Ok(as_read(&raw, account.id, READ_AT_MOST))
    }

    async fn sort_into(&self, arguments: &Value) -> Result<String, String> {
        let folder = text(arguments, "folder")?;
        let when = whens(arguments);
        if when.is_empty() {
            return Err("Say what goes there: from, to, subject or list.".into());
        }
        let accounts = self.where_to_sort(arguments, &when).await?;
        let mut done = self.done.lock().await;
        let mut made_rule = Vec::new();
        let mut added_to = Vec::new();
        let mut moved = 0;
        let mut shown = folder.to_string();
        for account in &accounts {
            let failed =
                |failure: crate::imap::Failure| format!("{}: {}", account.name, failure.message());
            let rules = self.mail.rules(account).await.map_err(failed)?;
            let (place, made) = self
                .mail
                .folder_named(account, folder)
                .await
                .map_err(failed)?;
            if made {
                done.made.push((account.clone(), place.path.clone()));
            }
            shown.clone_from(&place.name);
            let mut changed = rules.clone();
            let added: Vec<When> = match changed
                .iter_mut()
                .find(|rule| rule.path == place.path || same_folder(&rule.folder, folder))
            {
                Some(rule) => {
                    let added: Vec<When> = when
                        .iter()
                        .filter(|w| !rule.when.iter().any(|had| had.same(w)))
                        .cloned()
                        .collect();
                    rule.when.extend(added.iter().cloned());
                    added_to.push(account.name.clone());
                    added
                }
                None => {
                    changed.push(Rule {
                        folder: place.name.clone(),
                        path: place.path.clone(),
                        when: when.clone(),
                    });
                    made_rule.push(account.name.clone());
                    when.clone()
                }
            };
            if changed == rules {
                continue;
            }
            if !done.before.iter().any(|(a, _)| a.id == account.id) {
                done.before.push((account.clone(), rules));
            }
            self.mail
                .put_rules(account, &changed)
                .await
                .map_err(failed)?;
            let out = self
                .mail
                .move_matching(account, &added, &place.path)
                .await
                .map_err(failed)?;
            moved += out.count;
            done.moved.push((account.clone(), out));
            done.folder = Some(FolderLink {
                key: place.key(),
                name: place.name.clone(),
            });
        }
        for chip in when.iter().map(When::chip) {
            if !done.chips.contains(&chip) {
                done.chips.push(chip);
            }
        }
        let who = self.who(&when).await;
        let mut said = String::new();
        if !made_rule.is_empty() {
            let _ = write!(
                said,
                "Made a rule {}: mail {who} goes to **{shown}**. ",
                self.in_accounts(&made_rule)
            );
        }
        if !added_to.is_empty() {
            let _ = write!(
                said,
                "Added to the rule for **{shown}** {}: mail {who}. ",
                self.in_accounts(&added_to)
            );
        }
        said.push_str(&match moved {
            0 => "Nothing in the Inbox matched yet.".to_string(),
            1 => "Moved 1 mail there.".to_string(),
            n => format!("Moved {n} mails there."),
        });
        Ok(format!("Done. Tell them so, in their language: {said}"))
    }

    async fn sort_by_reading(&self, arguments: &Value) -> Result<String, String> {
        let folder = text(arguments, "folder")?;
        let about = text(arguments, "looks_like")?;
        let about = about
            .strip_prefix("mail that looks like ")
            .unwrap_or(about)
            .trim_end_matches('.')
            .to_string();
        let named = strings(arguments, "accounts");
        let accounts: Vec<MailAccount> = if named.is_empty() {
            self.accounts.clone()
        } else {
            named
                .iter()
                .map(|n| self.account_by_address(n).cloned())
                .collect::<Result<_, _>>()?
        };
        let mut mails: Vec<(Uuid, u32)> = Vec::new();
        for mail in arguments["mails"].as_array().into_iter().flatten() {
            let account = self.account_by_address(mail["account"].as_str().unwrap_or(""))?;
            let uid = mail["uid"]
                .as_u64()
                .and_then(|uid| u32::try_from(uid).ok())
                .ok_or("Say each mail by its account and uid.")?;
            mails.push((account.id, uid));
        }
        let mut done = self.done.lock().await;
        let mut made_rule = Vec::new();
        let mut moved = 0;
        let mut shown = folder.to_string();
        for account in &accounts {
            let failed =
                |failure: crate::imap::Failure| format!("{}: {}", account.name, failure.message());
            let kept = |e: sqlx::Error| {
                tracing::error!(error = %e, "couldn't keep a reading rule");
                format!("{}: Postrider couldn't keep the rule.", account.name)
            };
            let (place, made) = self
                .mail
                .folder_named(account, folder)
                .await
                .map_err(failed)?;
            if made {
                done.made.push((account.clone(), place.path.clone()));
            }
            shown.clone_from(&place.name);
            let rules = self.mail.reading_rules(account.id).await.map_err(kept)?;
            let had = rules
                .iter()
                .any(|r| r.path == place.path && r.about.eq_ignore_ascii_case(&about));
            if !had {
                let rule = ReadingRule {
                    id: Uuid::now_v7(),
                    account_id: account.id,
                    folder: place.name.clone(),
                    path: place.path.clone(),
                    about: about.clone(),
                    created_at: Utc::now(),
                };
                self.mail.add_reading_rule(&rule).await.map_err(kept)?;
                done.reading_made.push(rule.id);
                made_rule.push(account.name.clone());
            }
            let uids: Vec<u32> = mails
                .iter()
                .filter(|(id, _)| *id == account.id)
                .map(|(_, uid)| *uid)
                .collect();
            if !uids.is_empty() {
                let out = self
                    .mail
                    .move_uids(account, &uids, &place.path)
                    .await
                    .map_err(failed)?;
                for uid in &uids {
                    let _ = self
                        .mail
                        .forget_inbox_line(account.id, i64::from(*uid))
                        .await;
                }
                moved += out.count;
                done.moved.push((account.clone(), out));
            }
            done.folder = Some(FolderLink {
                key: place.key(),
                name: place.name.clone(),
            });
        }
        let chip = format!("Looks like {about}");
        if !done.chips.contains(&chip) {
            done.chips.push(chip);
        }
        let mut said = String::new();
        if made_rule.is_empty() {
            let _ = write!(
                said,
                "Postrider sorts mail that looks like **{about}** into **{shown}** already. "
            );
        } else {
            let _ = write!(
                said,
                "Made a rule {}: mail that looks like **{about}** goes to **{shown}**. \
                 Postrider sorts it by reading new mail, so only while it's open on this Mac. ",
                self.in_accounts(&made_rule)
            );
        }
        said.push_str(&match moved {
            0 => "Nothing in the Inbox moved yet.".to_string(),
            1 => "Moved 1 mail there.".to_string(),
            n => format!("Moved {n} mails there."),
        });
        Ok(format!("Done. Tell them so, in their language: {said}"))
    }

    async fn stop_sorting(&self, arguments: &Value) -> Result<String, String> {
        let folder = text(arguments, "folder")?;
        let when = whens(arguments);
        let reading = arguments["reading"].as_bool().unwrap_or(false);
        let mut done = self.done.lock().await;
        let mut stopped = Vec::new();
        let mut problems = Vec::new();
        for account in self.accounts.iter().filter(|_| !reading) {
            let rules = match self.mail.rules(account).await {
                Ok(rules) => rules,
                Err(failure) => {
                    problems.push(format!("{}: {}", account.name, failure.message()));
                    continue;
                }
            };
            let mut changed = rules.clone();
            for rule in changed
                .iter_mut()
                .filter(|rule| same_folder(&rule.folder, folder))
            {
                if when.is_empty() {
                    rule.when.clear();
                } else {
                    rule.when.retain(|had| !when.iter().any(|w| w.same(had)));
                }
            }
            changed.retain(|rule| !rule.when.is_empty());
            if changed == rules {
                continue;
            }
            if let Err(failure) = self.mail.put_rules(account, &changed).await {
                problems.push(format!("{}: {}", account.name, failure.message()));
                continue;
            }
            if !done.before.iter().any(|(a, _)| a.id == account.id) {
                done.before.push((account.clone(), rules));
            }
            stopped.push(account.name.clone());
        }
        // Sorting by reading stops with the whole rule, or on its own.
        let mut stopped_reading = Vec::new();
        let mut abouts: Vec<String> = Vec::new();
        for account in self.accounts.iter().filter(|_| when.is_empty()) {
            let rules = match self.mail.reading_rules(account.id).await {
                Ok(rules) => rules,
                Err(e) => {
                    tracing::error!(error = %e, "couldn't read the reading rules");
                    problems.push(format!(
                        "{}: Postrider couldn't read its rules.",
                        account.name
                    ));
                    continue;
                }
            };
            for rule in rules.into_iter().filter(|r| same_folder(&r.folder, folder)) {
                if let Err(e) = self.mail.remove_reading_rule(rule.id).await {
                    tracing::error!(error = %e, "couldn't take away a reading rule");
                    problems.push(format!(
                        "{}: Postrider couldn't change its rules.",
                        account.name
                    ));
                    continue;
                }
                if !stopped_reading.contains(&account.name) {
                    stopped_reading.push(account.name.clone());
                }
                if !abouts.contains(&rule.about) {
                    abouts.push(rule.about.clone());
                }
                done.reading_removed.push(rule);
            }
        }
        if stopped.is_empty() && stopped_reading.is_empty() {
            return match problems.is_empty() {
                true => Ok(format!("No rule sorts that into {folder}.")),
                false => Err(problems.join("\n")),
            };
        }
        for chip in when.iter().map(When::chip) {
            if !done.chips.contains(&chip) {
                done.chips.push(chip);
            }
        }
        for about in &abouts {
            let chip = format!("Looks like {about}");
            if !done.chips.contains(&chip) {
                done.chips.push(chip);
            }
        }
        let mut said = String::new();
        if !stopped.is_empty() {
            let what = if when.is_empty() {
                String::new()
            } else {
                format!(" mail {}", self.who(&when).await)
            };
            let _ = write!(
                said,
                "Stopped sorting{what} into **{folder}** {}. ",
                self.in_accounts(&stopped)
            );
        }
        if !stopped_reading.is_empty() {
            let looks: Vec<String> = abouts.iter().map(|a| format!("**{a}**")).collect();
            let _ = write!(
                said,
                "Stopped sorting mail that looks like {} into **{folder}** {}. ",
                or(&looks),
                self.in_accounts(&stopped_reading)
            );
        }
        Ok(format!(
            "Done. Tell them so, in their language: {said}What's there stays."
        ))
    }

    /// Where a rule goes: the accounts named, else those its senders wrote
    /// to, else every account.
    async fn where_to_sort(
        &self,
        arguments: &Value,
        when: &[When],
    ) -> Result<Vec<MailAccount>, String> {
        let named = strings(arguments, "accounts");
        if !named.is_empty() {
            return named
                .iter()
                .map(|n| self.account_by_address(n).cloned())
                .collect();
        }
        let senders: Vec<String> = when
            .iter()
            .filter_map(|w| match w {
                When::From(who) => Some(who.clone()),
                _ => None,
            })
            .collect();
        let written = rules::written_to(&self.mail, &self.accounts, &senders).await;
        let found: Vec<MailAccount> = self
            .accounts
            .iter()
            .filter(|a| written.contains(&a.id))
            .cloned()
            .collect();
        Ok(if found.is_empty() {
            self.accounts.clone()
        } else {
            found
        })
    }

    /// What a rule looks for, in words: "from **GitHub** and **Code
    /// Villain Git**".
    async fn who(&self, when: &[When]) -> String {
        let addresses: Vec<String> = when
            .iter()
            .filter_map(|w| match w {
                When::From(who) if who.contains('@') => Some(who.clone()),
                _ => None,
            })
            .collect();
        let names: HashMap<String, String> =
            rules::names_of(&self.mail, &self.accounts, &addresses)
                .await
                .into_iter()
                .filter(|(_, name)| !name.trim().is_empty())
                .collect();
        let named = |who: &str| -> String {
            if who.contains('@') {
                let name = names.get(&who.to_lowercase()).map_or(who, String::as_str);
                format!("**{name}**")
            } else {
                format!("anyone at **{who}**")
            }
        };
        let mut parts = Vec::new();
        let of = |pick: fn(&When) -> Option<&str>| -> Vec<&str> {
            when.iter().filter_map(pick).collect()
        };
        let from = of(|w| matches!(w, When::From(_)).then(|| w.text()));
        if !from.is_empty() {
            let names: Vec<String> = from.iter().map(|w| named(w)).collect();
            parts.push(format!("from {}", and(&names)));
        }
        let to = of(|w| matches!(w, When::To(_)).then(|| w.text()));
        if !to.is_empty() {
            let names: Vec<String> = to.iter().map(|w| named(w)).collect();
            parts.push(format!("to {}", and(&names)));
        }
        let list = of(|w| matches!(w, When::List(_)).then(|| w.text()));
        if !list.is_empty() {
            let names: Vec<String> = list.iter().map(|w| format!("**{w}**")).collect();
            parts.push(format!("from the list {}", and(&names)));
        }
        let subject = of(|w| matches!(w, When::Subject(_)).then(|| w.text()));
        if !subject.is_empty() {
            let words: Vec<String> = subject.iter().map(|w| format!("“{w}”")).collect();
            parts.push(format!("with {} in the subject", or(&words)));
        }
        parts.join(", or ")
    }

    /// "in both accounts", "in Lazarski".
    fn in_accounts(&self, names: &[String]) -> String {
        match (names.len(), self.accounts.len()) {
            (2, 2) => "in both accounts".into(),
            (n, all) if n == all && n > 2 => "in every account".into(),
            _ => format!("in {}", and(names)),
        }
    }

    fn account_by_id(&self, id: Uuid) -> Option<&MailAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }

    fn account_by_address(&self, said: &str) -> Result<&MailAccount, String> {
        let said = said.trim();
        self.accounts
            .iter()
            .find(|a| a.address.eq_ignore_ascii_case(said) || a.name.eq_ignore_ascii_case(said))
            .ok_or_else(|| {
                let known: Vec<&str> = self.accounts.iter().map(|a| a.address.as_str()).collect();
                format!(
                    "There's no account {said}. The accounts are {}.",
                    known.join(", ")
                )
            })
    }
}

fn text<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments[name]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("Say the {name}."))
}

fn strings(arguments: &Value, name: &str) -> Vec<String> {
    match &arguments[name] {
        Value::String(one) => vec![one.trim().to_string()],
        Value::Array(many) => many
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.trim().to_string())
            .collect(),
        _ => Vec::new(),
    }
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect()
}

/// A mail as Claude Code reads it: who it's from and to, when, its subject,
/// its files and up to `at_most` characters of its text.
pub(crate) fn as_read(raw: &[u8], account_id: Uuid, at_most: usize) -> String {
    let parsed = MessageParser::new().parse(raw);
    let from = parsed
        .as_ref()
        .and_then(|m| m.from())
        .and_then(|list| list.first())
        .map(|mailbox| match (mailbox.name(), mailbox.address()) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (name, address) => name.or(address).unwrap_or_default().to_string(),
        })
        .unwrap_or_default();
    let subject = parsed
        .as_ref()
        .and_then(|m| m.subject())
        .unwrap_or_default();
    let date = parsed
        .as_ref()
        .and_then(|m| m.date())
        .map(|d| d.to_rfc3339())
        .unwrap_or_default();
    let summary = MailSummary {
        account_id,
        folder: None,
        uid: 0,
        from_name: String::new(),
        from_address: String::new(),
        subject: String::new(),
        snippet: String::new(),
        date: Utc::now(),
        seen: true,
        to_name: None,
    };
    let opened = message::read(raw, summary);
    let mut body: String = opened.text.chars().take(at_most).collect();
    if body.len() < opened.text.len() {
        body.push_str("\n[…]");
    }
    let files: Vec<&str> = opened.attachments.iter().map(|a| a.name.as_str()).collect();
    let mut read = format!(
        "From: {from}\nTo: {}\nDate: {date}\nSubject: {subject}\n",
        opened.to.join(", ")
    );
    if !files.is_empty() {
        let _ = writeln!(read, "Files: {}", files.join(", "));
    }
    let _ = write!(read, "\n{body}");
    read
}

/// What a sort_into or stop_sorting looks for.
fn whens(arguments: &Value) -> Vec<When> {
    let mut when = Vec::new();
    let senders = |who: String| who.trim_start_matches('@').to_lowercase();
    when.extend(
        strings(arguments, "from")
            .into_iter()
            .map(senders)
            .map(When::From),
    );
    when.extend(
        strings(arguments, "to")
            .into_iter()
            .map(senders)
            .map(When::To),
    );
    when.extend(strings(arguments, "list").into_iter().map(When::List));
    when.extend(strings(arguments, "subject").into_iter().map(When::Subject));
    when
}

fn and(names: &[String]) -> String {
    joined(names, "and")
}

fn or(names: &[String]) -> String {
    joined(names, "or")
}

fn joined(names: &[String], last: &str) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., end] => format!("{} {last} {end}", rest.join(", ")),
    }
}

/// How Postrider answers, and what it knows before it looks.
async fn prompt(mail: &Arc<Mail>, workspace_id: Uuid, accounts: &[MailAccount]) -> String {
    let listed: Vec<String> = accounts
        .iter()
        .map(|a| format!("{} <{}>", a.name, a.address))
        .collect();
    let folders = match mail.folders(workspace_id).await {
        Ok(folders) => folders
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
        Err(_) => "couldn't be read".into(),
    };
    let mut sorting = String::new();
    for account in accounts {
        let mut rules = match mail.rules(account).await {
            Ok(rules) => rules::described(&rules),
            Err(failure) => format!("can't sort there: {}", failure.message()),
        };
        let reading: Vec<String> = mail
            .reading_rules(account.id)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| format!("{} ← looks like {}", r.folder, r.about))
            .collect();
        if !reading.is_empty() {
            let _ = write!(rules, "; by reading: {}", reading.join("; "));
        }
        let _ = writeln!(sorting, "- {}: {rules}", account.name);
    }
    let now = Local::now().format("%A, %e %B %Y, %H:%M");
    format!(
        "You are Postrider, the mail and calendar app on this Mac. The person asks you \
         something about their mail, or tells you what to do, in the field at the foot of \
         the app. Answer in the language they wrote in, in one to three short sentences, \
         with no lists, headings or tables. Put names, senders and folders in **bold**.\n\
         \n\
         Their accounts: {accounts}.\n\
         Their folders: {folders}. A folder with the same name in more than one account is \
         one folder to them.\n\
         Sorting now, by account:\n{sorting}\
         It is {now}.\n\
         \n\
         To sort mail into a folder, find its senders first with find_senders: a name like \
         GitHub isn't an address. Then sort_into the folder they named, with the addresses \
         it found, or a domain for anyone there. To change a rule (\"Dependabot too\"), \
         sort_into the same folder again with what's new; to take something out, \
         stop_sorting. When a tool says what it did, say that, in its words; add a sentence \
         only for what they should know, such as a sender you didn't find. Sort only what \
         they asked for.\n\
         \n\
         Sorting that needs reading a mail, like \"anything that looks like an invoice\", \
         can't be a rule on the mail server: sort_by_reading does it, on this Mac while \
         Postrider is open. Before it, find what's in the Inbox already that fits with \
         find_mail, reading what you're unsure of, and give it those mails. When a sender \
         says it all, a rule with sort_into is better: it sorts while the Mac is shut.\n\
         \n\
         To answer about their mail, find it with find_mail and read it with read_mail. \
         Never make up mail, senders, numbers or what a mail says.",
        accounts = listed.join(", "),
    )
}

/// `POST /api/w/{workspace}/ask`: what you ask, answered by Claude Code on
/// this Mac. What it changed waits for Undo, by the answer's id.
pub async fn ask(
    State(state): State<AppState>,
    member: Member,
    Json(asked): Json<Asked>,
) -> Result<Json<Answer>, AppError> {
    let said = asked.text.trim().to_string();
    if said.is_empty() {
        return Err(AppError::invalid(
            "Ask something, or tell Postrider what to do.",
        ));
    }
    let Some(adapter) = state.claude_code.clone() else {
        return Err(AppError::refused(
            "Postrider answers with Claude Code on this Mac, and this server can't start it.",
        ));
    };
    let workspace_id = member.actor.workspace_id;
    let accounts = state.mail.accounts(workspace_id).await?;
    if accounts.is_empty() {
        return Err(AppError::invalid(
            "Add an account in Settings first: there's no mail to ask about.",
        ));
    }
    let system = prompt(&state.mail, workspace_id, &accounts).await;
    let open = state.asking.open(Asking {
        mail: state.mail.clone(),
        workspace_id,
        accounts,
        done: tokio::sync::Mutex::new(Done {
            workspace_id,
            ..Done::default()
        }),
    });
    // Its own task, so a page that goes away doesn't stop it halfway
    // through a change Undo should know of.
    let answers = state.answers.clone();
    let answered = tokio::spawn(async move {
        let question = Question {
            system: &system,
            prompt: &said,
            images: &[],
            web: false,
            tools: TOOLS,
            token: &open.token,
            timeout: Some(TIMEOUT),
            max_turns: Some(MAX_TURNS),
        };
        let answer = adapter.ask(&question, |_| {}).await;
        let done = std::mem::take(&mut *open.what().done.lock().await);
        drop(open);
        let chips = done.chips.clone();
        let folder = done.folder.clone();
        let id = done.changed().then(|| {
            let id = Uuid::now_v7();
            answers.lock().unwrap().insert(id, done);
            id
        });
        (answer, id, chips, folder)
    })
    .await
    .map_err(AppError::internal)?;
    let (answer, id, chips, folder) = answered;
    let text = match answer {
        Ok(answer) => answer.text.trim().to_string(),
        // It changed something before it stopped: show that, with Undo.
        Err(failure) if id.is_some() => problem(failure),
        Err(failure) => return Err(AppError::refused(problem(failure))),
    };
    Ok(Json(Answer {
        id,
        text,
        chips,
        folder,
    }))
}

/// `POST /api/w/{workspace}/answers/{answer}/undo`: puts back what an
/// answer changed: the rules as they were, the mail it moved in the
/// Inbox, and no folder it made, if nothing's come to it since.
pub async fn undo(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let done = {
        let mut answers = state.answers.lock().unwrap();
        match answers.get(&id) {
            Some(done) if done.workspace_id == member.actor.workspace_id => answers.remove(&id),
            _ => None,
        }
    }
    .ok_or_else(|| AppError::not_found("There's nothing to undo here any more."))?;
    match put_back(&state.mail, &done).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(failure) => {
            state.answers.lock().unwrap().insert(id, done);
            Err(AppError::refused(failure.message()))
        }
    }
}

async fn put_back(mail: &Mail, done: &Done) -> Result<(), crate::imap::Failure> {
    for (account, rules) in &done.before {
        mail.put_rules(account, rules).await?;
    }
    let kept = |e: sqlx::Error| {
        tracing::error!(error = %e, "couldn't put back a reading rule");
        crate::imap::Failure::Offline("Postrider couldn't change its rules.".into())
    };
    for id in &done.reading_made {
        mail.remove_reading_rule(*id).await.map_err(kept)?;
    }
    for rule in &done.reading_removed {
        mail.add_reading_rule(rule).await.map_err(kept)?;
    }
    for (account, moved) in &done.moved {
        mail.move_back(account, moved).await?;
    }
    for (account, path) in &done.made {
        mail.remove_if_empty(account, path).await?;
    }
    Ok(())
}

/// Why Claude Code gave no answer, in words for the page.
pub(crate) fn problem(failure: Failure) -> String {
    match failure {
        Failure::NotInstalled => "Postrider answers with Claude Code on this Mac, and its ACP \
             adapter isn't installed. Run npm install -g @agentclientprotocol/claude-agent-acp \
             in Terminal, then ask again."
            .into(),
        Failure::SignedOut => "Claude Code isn't signed in on this Mac. Run claude in Terminal \
             and sign in there, then ask again."
            .into(),
        Failure::Trouble(said) => said,
    }
}

/// `POST /api/ai/mcp`: a question's tools, for Claude Code, behind the
/// question's token. One JSON-RPC message per request; a notification gets
/// 202.
pub async fn mcp_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Result<Response, AppError> {
    let token = bearer(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
    );
    let asking = token
        .and_then(|token| state.asking.get(token))
        .ok_or_else(|| AppError::unauthorized("This question is over."))?;
    Ok(match mcp::reply(POSTRIDER, &message, &asking.what).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_to_sort_is_read_from_the_call() {
        let when = whens(&json!({
            "folder": "Code",
            "from": ["Notifications@GitHub.com", "@codevillain.eu", " "],
            "subject": "Invoice",
        }));
        assert_eq!(
            when,
            vec![
                When::From("notifications@github.com".into()),
                When::From("codevillain.eu".into()),
                When::Subject("Invoice".into()),
            ]
        );
    }

    #[test]
    fn names_are_joined_in_words() {
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(and(&names(&["GitHub"])), "GitHub");
        assert_eq!(and(&names(&["A", "B", "C"])), "A, B and C");
        assert_eq!(or(&names(&["A", "B"])), "A or B");
    }
}
