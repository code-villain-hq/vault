//! Parcels, at `GET /api/w/{workspace}/parcels`: the tracking numbers in
//! the mail this Mac keeps (the Inbox's and the folders a search asks, but
//! Sent), each with where it is as its newest mail says and a link to
//! follow it. Nothing is asked of a carrier: the link is the mail's own,
//! else the carrier's page the number or the mail names, else Parcels'
//! page, which finds the carrier itself.
//!
//! A number is found three ways: a `ParcelDelivery` a shop or carrier
//! wrote into the mail (schema.org, as JSON-LD), a link to a carrier's
//! tracking page, or the words around it ("Sendungsnummer: …", and numbers
//! only one carrier writes, `1Z…`, `JJD…`, in a mail about a parcel).
//! What each mail says is read once and kept in memory.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use axum::Json;
use axum::extract::State;
use chrono::{Duration, Utc};
use mail_parser::MessageParser;
use postrider_domain::{FolderRole, MailSummary, Parcel, ParcelState, Parcels};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::folders::{searched, summary};
use crate::mail::Mail;
use crate::state::AppState;

/// A delivered parcel stays on the page this long after its last mail.
const DELIVERED_DAYS: i64 = 7;
/// Any other this long: a mail that stopped coming says nothing more.
const STALE_DAYS: i64 = 60;

/// Where a number goes when no carrier is known: Parcels finds it.
const ANY_CARRIER: &str = "https://parcelsapp.com/en/tracking/";

struct Carrier {
    name: &'static str,
    /// Its tracking page, the number in place of `{}`.
    page: Option<&'static str>,
    /// Its web and mail domains, and what's under them.
    domains: &'static [&'static str],
    /// How a mail names it, as written.
    words: &'static [&'static str],
}

const CARRIERS: &[Carrier] = &[
    Carrier {
        name: "DHL",
        page: Some(
            "https://www.dhl.de/de/privatkunden/pakete-empfangen/verfolgen.html?piececode={}",
        ),
        domains: &["dhl.de", "dhl.com", "deutschepost.de"],
        words: &["DHL", "Deutsche Post"],
    },
    Carrier {
        name: "DPD",
        page: Some("https://tracking.dpd.de/status/de_DE/parcel/{}"),
        domains: &["dpd.de", "dpd.com"],
        words: &["DPD"],
    },
    Carrier {
        name: "Hermes",
        page: Some("https://www.myhermes.de/empfangen/sendungsverfolgung/sendungsinformation#{}"),
        domains: &["myhermes.de", "hermesworld.com", "hermes-europe.co.uk"],
        words: &["Hermes"],
    },
    Carrier {
        name: "GLS",
        page: Some("https://gls-group.com/DE/de/paketverfolgung?match={}"),
        domains: &["gls-group.com", "gls-group.eu", "gls-pakete.de"],
        words: &["GLS"],
    },
    Carrier {
        name: "UPS",
        page: Some("https://www.ups.com/track?tracknum={}"),
        domains: &["ups.com"],
        words: &["UPS"],
    },
    Carrier {
        name: "FedEx",
        page: Some("https://www.fedex.com/fedextrack/?trknbr={}"),
        domains: &["fedex.com"],
        words: &["FedEx"],
    },
    Carrier {
        name: "Amazon",
        page: None,
        domains: &[],
        words: &["Amazon Logistics"],
    },
];

/// What one mail says of one parcel.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sighting {
    pub number: String,
    pub carrier: Option<String>,
    /// The mail's own link to follow it.
    pub url: Option<String>,
    pub state: ParcelState,
}

/// A kept mail: its account, folder path ("INBOX" for the Inbox's copy) and UID.
type Key = (Uuid, String, i64);

fn read() -> &'static Mutex<HashMap<Key, Vec<Sighting>>> {
    static READ: OnceLock<Mutex<HashMap<Key, Vec<Sighting>>>> = OnceLock::new();
    READ.get_or_init(Mutex::default)
}

impl Mail {
    /// The parcels the kept mail of the workspace's accounts names: still
    /// coming first, then delivered, newest mail first.
    pub async fn parcels(&self, workspace_id: Uuid) -> Result<Parcels, AppError> {
        let accounts = self.accounts(workspace_id).await?;
        let ids: Vec<Uuid> = accounts.iter().map(|account| account.id).collect();
        let mut mails: Vec<(Key, MailSummary)> = Vec::new();

        type Row = (
            Uuid,
            i64,
            String,
            String,
            String,
            String,
            chrono::DateTime<Utc>,
            bool,
            bool,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.account_id, m.uid, m.from_name, m.from_address, m.subject, m.snippet, \
             m.date, m.seen, m.flagged FROM inbox_mails m \
             WHERE m.account_id = ANY($1) AND EXISTS (SELECT 1 FROM mail_sources s \
             WHERE s.account_id = m.account_id AND s.uid = m.uid)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        for (account_id, uid, from_name, from_address, subject, snippet, date, seen, flagged) in
            rows
        {
            mails.push((
                (account_id, "INBOX".into(), uid),
                MailSummary {
                    account_id,
                    folder: None,
                    uid,
                    from_name,
                    from_address,
                    subject,
                    snippet,
                    date,
                    seen,
                    flagged,
                    to_name: None,
                    unsent: false,
                },
            ));
        }

        let sourced: HashSet<Key> = sqlx::query_as::<_, Key>(
            "SELECT account_id, path, uid FROM folder_sources WHERE account_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect();
        for (account_id, place, line) in self.kept_lines(&ids, false).await? {
            if !searched(&place) || place.role == Some(FolderRole::Sent) {
                continue;
            }
            let key = (account_id, place.path.clone(), line.uid);
            if sourced.contains(&key) {
                mails.push((key, summary(account_id, &place, line)));
            }
        }

        // Reads what wasn't read yet, a folder at a time.
        let unread: Vec<&(Key, MailSummary)> = {
            let known = read().lock().expect("parcels read");
            mails
                .iter()
                .filter(|(key, _)| !known.contains_key(key))
                .collect()
        };
        let mut wanted: HashMap<(Uuid, String), Vec<i64>> = HashMap::new();
        for ((account_id, path, uid), _) in &unread {
            wanted
                .entry((*account_id, path.clone()))
                .or_default()
                .push(*uid);
        }
        let mut sources: Vec<(Key, Vec<u8>, String, String)> = Vec::new();
        for ((account_id, path), uids) in wanted {
            let rows: Vec<(i64, Vec<u8>)> = if path == "INBOX" {
                sqlx::query_as(
                    "SELECT uid, raw FROM mail_sources WHERE account_id = $1 AND uid = ANY($2)",
                )
                .bind(account_id)
                .bind(&uids)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_as(
                    "SELECT uid, raw FROM folder_sources \
                     WHERE account_id = $1 AND path = $2 AND uid = ANY($3)",
                )
                .bind(account_id)
                .bind(&path)
                .bind(&uids)
                .fetch_all(&self.pool)
                .await?
            };
            for (uid, raw) in rows {
                let key = (account_id, path.clone(), uid);
                if let Some((_, mail)) = unread.iter().find(|(k, _)| *k == key) {
                    sources.push((key, raw, mail.from_address.clone(), mail.subject.clone()));
                }
            }
        }
        let found = tokio::task::spawn_blocking(move || {
            sources
                .into_iter()
                .map(|(key, raw, from, subject)| (key, sightings(&raw, &from, &subject)))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|error| AppError::internal(format!("reading parcels: {error}")))?;

        let mut known = read().lock().expect("parcels read");
        known.extend(found);
        let present: HashSet<&Key> = mails.iter().map(|(key, _)| key).collect();
        known.retain(|key, _| present.contains(key) || !ids.contains(&key.0));

        let mut ordered: Vec<(&MailSummary, &Sighting)> = mails
            .iter()
            .filter_map(|(key, mail)| known.get(key).map(|seen| (mail, seen)))
            .flat_map(|(mail, seen)| seen.iter().map(move |sighting| (mail, sighting)))
            .collect();
        ordered.sort_by_key(|(mail, _)| mail.date);
        let parcels = gather(ordered, Utc::now());
        Ok(Parcels { parcels })
    }
}

/// One parcel per number, oldest mail first in: its newest mail, the
/// furthest state any mail gave it, and the link to follow it.
fn gather(ordered: Vec<(&MailSummary, &Sighting)>, now: chrono::DateTime<Utc>) -> Vec<Parcel> {
    let mut parcels: Vec<Parcel> = Vec::new();
    for (mail, sighting) in ordered {
        match parcels
            .iter_mut()
            .find(|parcel| parcel.number == sighting.number)
        {
            Some(parcel) => {
                parcel.mail = mail.clone();
                parcel.state = parcel.state.max(sighting.state);
                if sighting.carrier.is_some() {
                    parcel.carrier.clone_from(&sighting.carrier);
                }
                if let Some(url) = &sighting.url {
                    parcel.track_url.clone_from(url);
                }
            }
            None => parcels.push(Parcel {
                number: sighting.number.clone(),
                carrier: sighting.carrier.clone(),
                track_url: sighting.url.clone().unwrap_or_default(),
                state: sighting.state,
                mail: mail.clone(),
            }),
        }
    }
    for parcel in &mut parcels {
        if parcel.track_url.is_empty() {
            parcel.track_url = page(parcel.carrier.as_deref(), &parcel.number);
        }
    }
    parcels.retain(|parcel| {
        let age = now - parcel.mail.date;
        match parcel.state {
            ParcelState::Delivered => age < Duration::days(DELIVERED_DAYS),
            _ => age < Duration::days(STALE_DAYS),
        }
    });
    parcels.sort_by(|a, b| {
        (a.state == ParcelState::Delivered)
            .cmp(&(b.state == ParcelState::Delivered))
            .then(b.mail.date.cmp(&a.mail.date))
    });
    parcels
}

/// The carrier's tracking page for a number, else Parcels'.
fn page(carrier: Option<&str>, number: &str) -> String {
    CARRIERS
        .iter()
        .find(|known| Some(known.name) == carrier)
        .and_then(|known| known.page)
        .map_or_else(
            || format!("{ANY_CARRIER}{number}"),
            |page| page.replace("{}", number),
        )
}

/// What one mail says of the parcels in it.
pub(crate) fn sightings(raw: &[u8], from_address: &str, subject: &str) -> Vec<Sighting> {
    let Some(message) = MessageParser::new().parse(raw) else {
        return Vec::new();
    };
    let text = message.text_bodies().collect::<Vec<_>>().join("\n");
    let html = message.html_bodies().collect::<Vec<_>>().join("\n");
    let links = links(&html, &text);
    let state = state_of(subject, &text);
    let named = carrier_named(from_address, &text);
    let mut found: Vec<Sighting> = declared(&html);

    // Links to a carrier's own page, with the number in them.
    for link in &links {
        let Some(carrier) = host(link).and_then(carrier_at) else {
            continue;
        };
        if let Some(number) = number_in_link(link) {
            found.push(Sighting {
                number,
                carrier: Some(carrier.name.into()),
                url: Some(link.clone()),
                state,
            });
        }
    }

    // Numbers in the words.
    for number in written(subject, &text) {
        let carrier = carrier_of(&number)
            .map(String::from)
            .or_else(|| named.map(String::from));
        let url = links
            .iter()
            .find(|link| link.to_ascii_uppercase().contains(&number))
            .cloned();
        found.push(Sighting {
            number,
            carrier,
            url,
            state,
        });
    }

    // One per number: what says most of it first.
    let mut kept: Vec<Sighting> = Vec::new();
    for sighting in found {
        match kept.iter_mut().find(|k| k.number == sighting.number) {
            Some(k) => {
                if k.carrier.is_none() {
                    k.carrier = sighting.carrier;
                }
                if k.url.is_none() {
                    k.url = sighting.url;
                }
            }
            None => kept.push(sighting),
        }
    }
    kept
}

/// The `ParcelDelivery`s a mail declares (schema.org, in JSON-LD).
fn declared(html: &str) -> Vec<Sighting> {
    let lower = html.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("application/ld+json") {
        let at = from + at;
        let Some(open) = lower[at..].find('>').map(|i| at + i + 1) else {
            break;
        };
        let Some(close) = lower[open..].find("</script").map(|i| open + i) else {
            break;
        };
        from = close;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&html[open..close]) {
            deliveries(&value, &mut found);
        }
    }
    found
}

fn deliveries(value: &serde_json::Value, found: &mut Vec<Sighting>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                deliveries(item, found);
            }
        }
        serde_json::Value::Object(fields) => {
            let kind = fields
                .get("@type")
                .map(ToString::to_string)
                .unwrap_or_default();
            if kind.contains("ParcelDelivery") {
                let number = fields
                    .get("trackingNumber")
                    .and_then(serde_json::Value::as_str)
                    .map(|n| {
                        n.split_whitespace()
                            .collect::<String>()
                            .to_ascii_uppercase()
                    })
                    .filter(|n| plausible(n));
                if let Some(number) = number {
                    let carrier = ["carrier", "provider"]
                        .iter()
                        .filter_map(|field| fields.get(*field))
                        .find_map(|who| {
                            who.as_str()
                                .or_else(|| who.get("name").and_then(serde_json::Value::as_str))
                        })
                        .map(|name| {
                            CARRIERS
                                .iter()
                                .find(|known| known.words.iter().any(|word| name.contains(word)))
                                .map_or_else(|| name.trim().to_string(), |known| known.name.into())
                        });
                    let status = [fields.get("deliveryStatus"), fields.get("partOfOrder")]
                        .into_iter()
                        .flatten()
                        .map(|status| status.to_string().to_ascii_lowercase())
                        .collect::<String>();
                    let state = if status.contains("delivered") {
                        ParcelState::Delivered
                    } else if status.contains("pickup") {
                        ParcelState::PickUp
                    } else {
                        ParcelState::Shipped
                    };
                    found.push(Sighting {
                        number,
                        carrier,
                        url: fields
                            .get("trackingUrl")
                            .and_then(serde_json::Value::as_str)
                            .filter(|url| url.starts_with("https://"))
                            .map(String::from),
                        state,
                    });
                }
            }
            for value in fields.values() {
                deliveries(value, found);
            }
        }
        _ => {}
    }
}

/// Every web link in the mail: its HTML's `href`s, then its text's.
fn links(html: &str, text: &str) -> Vec<String> {
    let mut links = Vec::new();
    for part in html.split("href=").skip(1) {
        let Some(quote) = part.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let Some(end) = part[1..].find(quote) else {
            continue;
        };
        links.push(part[1..=end].replace("&amp;", "&"));
    }
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '(' | ')'))
    {
        if word.starts_with("https://") || word.starts_with("http://") {
            links.push(word.to_string());
        }
    }
    links.retain(|link| link.starts_with("https://") || link.starts_with("http://"));
    links
}

fn host(link: &str) -> Option<String> {
    let rest = link.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase())
}

fn under(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn carrier_at(host: String) -> Option<&'static Carrier> {
    CARRIERS
        .iter()
        .find(|carrier| carrier.domains.iter().any(|domain| under(&host, domain)))
}

/// A tracking number in a carrier's link: a query's value or a part of its path.
fn number_in_link(link: &str) -> Option<String> {
    let rest = link.split_once("://")?.1;
    rest.split(['/', '?', '&', '=', '#', ','])
        .skip(1)
        .map(|part| part.to_ascii_uppercase())
        .find(|part| plausible(part))
}

/// The one carrier the mail is from or names, if only one.
fn carrier_named(from_address: &str, text: &str) -> Option<&'static str> {
    let domain = from_address
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if let Some(carrier) = carrier_at(domain) {
        return Some(carrier.name);
    }
    let mut named = CARRIERS
        .iter()
        .filter(|carrier| carrier.words.iter().any(|word| has_word(text, word)));
    match (named.next(), named.next()) {
        (Some(carrier), None) => Some(carrier.name),
        _ => None,
    }
}

/// `word` in `text`, as written, not inside another word.
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// The carrier only it writes a number like this.
fn carrier_of(number: &str) -> Option<&'static str> {
    let bytes = number.as_bytes();
    if number.starts_with("1Z") && number.len() == 18 {
        Some("UPS")
    } else if number.starts_with("JJD") || number.starts_with("JVGL") {
        Some("DHL")
    } else if number.starts_with("TBA") {
        Some("Amazon")
    } else if is_s10(bytes) && number.ends_with("DE") {
        Some("DHL")
    } else {
        None
    }
}

/// A post's number, as the UPU writes it: `RR123456789DE`.
fn is_s10(bytes: &[u8]) -> bool {
    bytes.len() == 13
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..11].iter().all(u8::is_ascii_digit)
        && bytes[11..].iter().all(u8::is_ascii_uppercase)
}

/// What could be a tracking number: 10 to 35 capitals and digits, at
/// least 6 of them digits, not one digit over and over.
fn plausible(number: &str) -> bool {
    let digits = number.bytes().filter(u8::is_ascii_digit).count();
    (10..=35).contains(&number.len())
        && number
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        && digits >= 6
        && number.bytes().collect::<HashSet<_>>().len() > 2
}

/// What a mail calls its tracking number, in the languages it comes in.
const LABELS: &[&str] = &[
    "sendungsnummer",
    "sendungs-nr",
    "sendungsnr",
    "sendungscode",
    "sendungsverfolgungsnummer",
    "trackingnummer",
    "tracking-nummer",
    "tracking-nr",
    "trackingnr",
    "trackingcode",
    "tracking-code",
    "tracking code",
    "tracking number",
    "tracking no",
    "tracking id",
    "tracking-id",
    "tracking #",
    "tracking:",
    "paketnummer",
    "paketscheinnummer",
    "identcode",
    "parcel number",
    "shipment number",
    "waybill",
    "numéro de suivi",
];

/// Whether a mail is about a parcel at all.
const ABOUT_PARCELS: &[&str] = &[
    "sendung",
    "paket",
    "tracking",
    "shipment",
    "shipped",
    "versand",
    "versendet",
    "verschickt",
    "zustell",
    "lieferung",
    "parcel",
    "package",
    "delivery",
];

/// The numbers a mail's words give: after a label, and those only one
/// carrier writes, in a mail about a parcel.
fn written(subject: &str, text: &str) -> Vec<String> {
    let all = format!("{subject}\n{text}");
    let lower = all.to_ascii_lowercase();
    let mut found: Vec<String> = Vec::new();
    for label in LABELS {
        for (at, _) in lower.match_indices(label) {
            let after = &all[at + label.len()..];
            let window: String = after.chars().take(80).collect();
            if let Some(number) = first_number(&window)
                && !found.contains(&number)
            {
                found.push(number);
            }
        }
    }
    if ABOUT_PARCELS.iter().any(|word| lower.contains(word)) {
        for word in all.split(|c: char| !c.is_ascii_alphanumeric()) {
            if carrier_of(word).is_some() && plausible(word) && !found.iter().any(|f| f == word) {
                found.push(word.to_string());
            }
        }
    }
    found
}

/// The first number in the few words after a label, its groups joined
/// when it's written in them: `0034 0434 1234 5678 9012`.
fn first_number(window: &str) -> Option<String> {
    let words: Vec<&str> = window
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .take(6)
        .collect();
    for (i, word) in words.iter().enumerate() {
        if plausible(word) {
            return Some((*word).to_string());
        }
        let short_digits =
            |w: &&str| (2..=6).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_digit());
        if short_digits(word) {
            let joined: String = words[i..]
                .iter()
                .take_while(|w| short_digits(w))
                .copied()
                .collect();
            if plausible(&joined) {
                return Some(joined);
            }
        }
    }
    None
}

const DELIVERED: &[&str] = &[
    "wurde zugestellt",
    "ist zugestellt",
    "erfolgreich zugestellt",
    "zugestellt worden",
    "wurde heute zugestellt",
    "wurde geliefert",
    "has been delivered",
    "was delivered",
    "been delivered",
];
const PICK_UP: &[&str] = &[
    "abholbereit",
    "zur abholung",
    "abgeholt werden",
    "ready for pickup",
    "ready for pick-up",
    "ready for collection",
    "ready to be picked up",
];
const OUT_FOR_DELIVERY: &[&str] = &[
    "out for delivery",
    "in zustellung",
    "in der zustellung",
    "heute zugestellt",
    "heute geliefert",
    "kommt heute",
    "arriving today",
    "arrives today",
    "zustellfahrzeug",
];

/// Where the mail says its parcel is: its subject first, then its text.
fn state_of(subject: &str, text: &str) -> ParcelState {
    let says = |text: &str| {
        let lower = text.to_lowercase();
        [
            (DELIVERED, ParcelState::Delivered),
            (PICK_UP, ParcelState::PickUp),
            (OUT_FOR_DELIVERY, ParcelState::OutForDelivery),
        ]
        .into_iter()
        .find(|(words, _)| words.iter().any(|word| lower.contains(word)))
        .map(|(_, state)| state)
    };
    let subject_says = says(subject).or_else(|| {
        let lower = subject.to_lowercase();
        (lower.contains("zugestellt") || lower.contains("delivered"))
            .then_some(ParcelState::Delivered)
    });
    subject_says
        .or_else(|| says(text))
        .unwrap_or(ParcelState::Shipped)
}

/// `GET /api/w/{workspace}/parcels`.
pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Parcels>, AppError> {
    Ok(Json(state.mail.parcels(member.actor.workspace_id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(headers: &str, body: &str) -> Vec<u8> {
        format!("From: x@shop.example\r\n{headers}\r\n\r\n{body}").into_bytes()
    }

    #[test]
    fn reads_a_labelled_number_and_the_carrier_the_mail_names() {
        let raw = mail(
            "Subject: Ihre Bestellung ist unterwegs\r\nContent-Type: text/plain; charset=utf-8",
            "Wir haben Ihr Paket an DHL übergeben.\nSendungsnummer: 0034 0434 1234 5678 9012\n",
        );
        let found = sightings(&raw, "shop@shop.example", "Ihre Bestellung ist unterwegs");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number, "00340434123456789012");
        assert_eq!(found[0].carrier.as_deref(), Some("DHL"));
        assert_eq!(found[0].state, ParcelState::Shipped);
        assert_eq!(
            page(found[0].carrier.as_deref(), &found[0].number),
            "https://www.dhl.de/de/privatkunden/pakete-empfangen/verfolgen.html?piececode=00340434123456789012"
        );
    }

    #[test]
    fn keeps_the_mails_own_link_and_reads_the_state() {
        let raw = mail(
            "Subject: Your package is out for delivery\r\nContent-Type: text/html",
            "<p>Tracking number: 1Z999AA10123456784</p>\
             <a href=\"https://www.ups.com/track?loc=en_US&amp;tracknum=1Z999AA10123456784\">Track</a>",
        );
        let found = sightings(&raw, "a@shop.example", "Your package is out for delivery");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].carrier.as_deref(), Some("UPS"));
        assert_eq!(
            found[0].url.as_deref(),
            Some("https://www.ups.com/track?loc=en_US&tracknum=1Z999AA10123456784")
        );
        assert_eq!(found[0].state, ParcelState::OutForDelivery);
    }

    #[test]
    fn reads_a_declared_parcel_delivery() {
        let raw = mail(
            "Subject: Versandbestätigung\r\nContent-Type: text/html",
            r#"<script type="application/ld+json">{"@context":"http://schema.org","@type":"ParcelDelivery",
               "trackingNumber":"09876543210123","carrier":{"@type":"Organization","name":"DPD"},
               "partOfOrder":{"@type":"Order","orderStatus":"http://schema.org/OrderDelivered"}}</script><p>Hallo</p>"#,
        );
        let found = sightings(&raw, "a@shop.example", "Versandbestätigung");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number, "09876543210123");
        assert_eq!(found[0].carrier.as_deref(), Some("DPD"));
        assert_eq!(found[0].state, ParcelState::Delivered);
    }

    #[test]
    fn finds_nothing_in_a_mail_about_something_else() {
        let raw = mail(
            "Subject: Rechnung 2026-1234\r\nContent-Type: text/plain",
            "Bestellnummer 302-1234567-1234567, IBAN DE89370400440532013000\n",
        );
        assert!(sightings(&raw, "a@shop.example", "Rechnung 2026-1234").is_empty());
    }

    #[test]
    fn a_delivered_word_in_the_subject_wins_and_unknown_carriers_go_to_parcels() {
        assert_eq!(
            state_of(
                "Ihr Paket wurde zugestellt",
                "Falls es nicht abholbereit ist…"
            ),
            ParcelState::Delivered
        );
        assert_eq!(
            state_of("Neuigkeiten", "Ihr Paket ist abholbereit in der Filiale"),
            ParcelState::PickUp
        );
        assert_eq!(
            page(None, "ABC1234567890"),
            "https://parcelsapp.com/en/tracking/ABC1234567890"
        );
    }

    #[test]
    fn a_parcel_takes_its_newest_mail_and_furthest_state() {
        let now = Utc::now();
        let summary = |days: i64| MailSummary {
            account_id: Uuid::nil(),
            folder: None,
            uid: days,
            from_name: "Shop".into(),
            from_address: "a@shop.example".into(),
            subject: String::new(),
            snippet: String::new(),
            date: now - Duration::days(days),
            seen: true,
            flagged: false,
            to_name: None,
            unsent: false,
        };
        let seen = |state| Sighting {
            number: "00340434123456789012".into(),
            carrier: Some("DHL".into()),
            url: None,
            state,
        };
        let (old, new) = (summary(3), summary(1));
        let (shipped, delivered) = (seen(ParcelState::Shipped), seen(ParcelState::Delivered));
        let parcels = gather(vec![(&old, &shipped), (&new, &delivered)], now);
        assert_eq!(parcels.len(), 1);
        assert_eq!(parcels[0].state, ParcelState::Delivered);
        assert_eq!(parcels[0].mail.uid, 1);
        assert!(parcels[0].track_url.starts_with("https://www.dhl.de/"));

        let long_ago = summary(10);
        assert!(gather(vec![(&long_ago, &delivered)], now).is_empty());
    }
}
