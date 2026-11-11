//! Parcels, at `GET /api/w/{workspace}/parcels`: the tracking numbers in
//! the mail this Mac keeps (the Inbox's and the folders a search asks, but
//! Sent), and the orders a shop said are on their way before any mail gave
//! a number, each with where it is as its newest mail says and a link to
//! follow it. Nothing is asked of a carrier: the link is the mail's own,
//! else the carrier's page the number or the mail names, else the shop's
//! page for the order, else Parcels' page, which finds the carrier itself.
//!
//! A number is found four ways: a `ParcelDelivery` a shop or carrier
//! wrote into the mail (schema.org, as JSON-LD), a link to a carrier's
//! tracking page or one that names its number (`track_sn=…`), the words
//! around it ("Sendungsnummer: …", "Spårningsnummer: …", "Package …"), and
//! numbers only a carrier writes (`1Z…`, `JJD…`, a post's `RR123456785DE`
//! with its check digit), in a mail about a parcel. An order is the shop's
//! number after "Order", "Bestellung", "Beställning"…, in a mail whose
//! subject says it left (shipped, on its way, delivered); a number given
//! with that order later takes its place. What each mail says is read
//! once and kept in memory.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mail_parser::MessageParser;
use postrider_domain::{FolderRole, MailSummary, Parcel, ParcelDelivered, ParcelState, Parcels};
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
        domains: &["dhl.de", "dhl.com", "dhl.se", "deutschepost.de"],
        words: &["DHL", "Deutsche Post"],
    },
    Carrier {
        name: "DPD",
        page: Some("https://tracking.dpd.de/status/de_DE/parcel/{}"),
        domains: &["dpd.de", "dpd.com", "dpd.fr", "dpd.co.uk", "dpdgroup.com"],
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
        name: "PostNord",
        page: Some("https://tracking.postnord.com/se/tracking?id={}"),
        domains: &[
            "postnord.se",
            "postnord.com",
            "postnord.dk",
            "postnord.no",
            "postnord.fi",
        ],
        words: &["PostNord"],
    },
    Carrier {
        name: "Instabox",
        page: Some("https://track.instabox.io/{}"),
        domains: &["instabox.se", "instabox.io", "instabee.com"],
        words: &["Instabox"],
    },
    Carrier {
        name: "Budbee",
        page: None,
        domains: &["budbee.com"],
        words: &["Budbee"],
    },
    Carrier {
        name: "Airmee",
        page: None,
        domains: &["airmee.com"],
        words: &["Airmee"],
    },
    Carrier {
        name: "Early Bird",
        page: None,
        domains: &["earlybird.se"],
        words: &["Early Bird"],
    },
    Carrier {
        name: "Bring",
        page: Some("https://tracking.bring.com/tracking/{}"),
        domains: &["bring.com", "bring.se", "bring.no", "posten.no"],
        // "Bring" starts many a sentence.
        words: &[],
    },
    Carrier {
        name: "InPost",
        page: None,
        domains: &["inpost.pl", "inpost.eu", "inpost.co.uk"],
        words: &["InPost"],
    },
    Carrier {
        name: "Chronopost",
        page: Some("https://www.chronopost.fr/tracking-no-cms/suivi-page?listeNumerosLT={}"),
        domains: &["chronopost.fr"],
        words: &["Chronopost"],
    },
    Carrier {
        name: "Colissimo",
        page: Some("https://www.laposte.fr/outils/suivre-vos-envois?code={}"),
        domains: &["laposte.fr", "colissimo.fr"],
        words: &["Colissimo"],
    },
    Carrier {
        name: "Royal Mail",
        page: Some("https://www.royalmail.com/track-your-item#/tracking-results/{}"),
        domains: &["royalmail.com"],
        words: &["Royal Mail"],
    },
    Carrier {
        name: "USPS",
        page: Some("https://tools.usps.com/go/TrackConfirmAction?tLabels={}"),
        domains: &["usps.com"],
        words: &["USPS"],
    },
    Carrier {
        name: "Amazon",
        page: None,
        domains: &[],
        words: &["Amazon Logistics"],
    },
];

/// What one mail says of one parcel: its number, its order, or both.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sighting {
    pub number: Option<String>,
    pub order: Option<String>,
    pub carrier: Option<String>,
    /// The mail's own link to follow it, with its number.
    pub url: Option<String>,
    /// The shop's link for its order, which follows it too.
    pub order_url: Option<String>,
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
        let marks: Marks = sqlx::query_as::<_, (String, DateTime<Utc>)>(
            "SELECT key, created_at FROM delivered_parcels WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect();

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
        let parcels = gather(ordered, &marks, Utc::now());
        Ok(Parcels { parcels })
    }
}

/// The parcels you said came, by key, and when you said so.
type Marks = HashMap<String, DateTime<Utc>>;

/// One parcel per number, oldest mail first in: its newest mail, the
/// furthest state any mail gave it, and the link to follow it. An order
/// without a number is one parcel with the number its shop gives it later.
/// One you said came is delivered from then on, until a mail says so too.
fn gather(
    ordered: Vec<(&MailSummary, &Sighting)>,
    marks: &Marks,
    now: DateTime<Utc>,
) -> Vec<Parcel> {
    struct Gathered {
        parcel: Parcel,
        shop: String,
        order_url: Option<String>,
    }
    let mut gathered: Vec<Gathered> = Vec::new();
    for (mail, sighting) in ordered {
        let shop = shop_of(&mail.from_address);
        let same = |known: &Gathered| match (&known.parcel.number, &sighting.number) {
            (Some(a), Some(b)) => a == b,
            // An order and the number its shop gave it, or the order again.
            _ => {
                sighting.order.is_some()
                    && known.parcel.order == sighting.order
                    && known.shop == shop
            }
        };
        let Some(at) = gathered.iter().position(same) else {
            gathered.push(Gathered {
                parcel: Parcel {
                    number: sighting.number.clone(),
                    order: sighting.order.clone(),
                    carrier: sighting.carrier.clone(),
                    track_url: sighting.url.clone(),
                    state: sighting.state,
                    marked_at: None,
                    key: String::new(),
                    mail: mail.clone(),
                },
                shop,
                order_url: sighting.order_url.clone(),
            });
            continue;
        };
        let known = &mut gathered[at];
        let parcel = &mut known.parcel;
        parcel.mail = mail.clone();
        parcel.state = parcel.state.max(sighting.state);
        if parcel.number.is_none() {
            parcel.number.clone_from(&sighting.number);
        }
        if parcel.order.is_none() {
            parcel.order.clone_from(&sighting.order);
        }
        if sighting.carrier.is_some() {
            parcel.carrier.clone_from(&sighting.carrier);
        }
        if sighting.url.is_some() {
            parcel.track_url.clone_from(&sighting.url);
        }
        if sighting.order_url.is_some() {
            known.order_url.clone_from(&sighting.order_url);
        }
    }
    let mut parcels: Vec<Parcel> = gathered
        .into_iter()
        .map(|known| {
            let mut parcel = known.parcel;
            if parcel.track_url.is_none() {
                parcel.track_url = match &parcel.number {
                    Some(number) => Some(page(parcel.carrier.as_deref(), number, known.order_url)),
                    None => known.order_url,
                };
            }
            // Its number, else its shop and order, the same while it has
            // none and once it has: a mark made either way holds.
            let order = parcel
                .order
                .as_ref()
                .map(|order| format!("{} {order}", known.shop));
            let keys: Vec<String> = parcel.number.iter().cloned().chain(order).collect();
            let marked = keys.iter().find_map(|key| marks.get_key_value(key));
            parcel.key = match marked {
                Some((key, _)) => key.clone(),
                None => keys.into_iter().next().unwrap_or_default(),
            };
            if let Some((_, at)) = marked
                && parcel.state != ParcelState::Delivered
            {
                parcel.state = ParcelState::Delivered;
                parcel.marked_at = Some(*at);
            }
            parcel
        })
        .collect();
    parcels.retain(|parcel| {
        let last = parcel.mail.date;
        let age = now - parcel.marked_at.map_or(last, |at| at.max(last));
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

/// Where to follow a number no mail linked with: the carrier's page, else
/// the shop's page for its order, else Parcels'.
fn page(carrier: Option<&str>, number: &str, order_url: Option<String>) -> String {
    CARRIERS
        .iter()
        .find(|known| Some(known.name) == carrier)
        .and_then(|known| known.page)
        .map(|page| page.replace("{}", number))
        .or(order_url)
        .unwrap_or_else(|| format!("{ANY_CARRIER}{number}"))
}

/// Whose shop a mail is from: the domain its sender registered, so
/// `notice.shop.example` and `orders.shop.example` are one.
fn shop_of(address: &str) -> String {
    let domain = address
        .rsplit_once('@')
        .map_or(address, |(_, domain)| domain)
        .to_ascii_lowercase();
    psl::domain_str(&domain).map_or_else(|| domain.clone(), String::from)
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
    // The carrier that wrote the mail knows best, even when it links to
    // the one that delivers.
    let sender = sender(from_address);
    let named = carrier_named(from_address, &text);
    let about = {
        let lower = format!("{subject}\n{text}").to_lowercase();
        ABOUT_PARCELS.iter().any(|word| lower.contains(word))
    };
    let mut found: Vec<Sighting> = declared(&html);

    // Links to a carrier's own page with the number in them, and, in a
    // mail about a parcel, any link that names its tracking number.
    for link in &links {
        let carrier = host(link).and_then(carrier_at);
        let number = match carrier {
            Some(_) => number_in_link(link),
            None if about => tracked_in_link(link),
            None => None,
        };
        let Some(number) = number else {
            continue;
        };
        let carrier = sender
            .or(carrier.map(|carrier| carrier.name))
            .or_else(|| carrier_of(&number))
            .or(named);
        found.push(Sighting {
            number: Some(number),
            order: None,
            carrier: carrier.map(String::from),
            url: Some(link.clone()),
            order_url: None,
            state,
        });
    }

    // Numbers in the words.
    for number in written(subject, &text, about) {
        let carrier = sender
            .or_else(|| carrier_of(&number))
            .or(named)
            .map(String::from);
        let url = links
            .iter()
            .find(|link| link.to_ascii_uppercase().contains(&number))
            .cloned();
        found.push(Sighting {
            number: Some(number),
            order: None,
            carrier,
            url,
            order_url: None,
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
                if k.order.is_none() {
                    k.order = sighting.order;
                }
            }
            None => kept.push(sighting),
        }
    }

    // The shop's order, which a number joins and which, once its subject
    // says it left, is a parcel before any number comes.
    let order = kept
        .iter()
        .find_map(|sighting| sighting.order.clone())
        .or_else(|| order_of(subject, &text))
        .or_else(|| links.iter().find_map(|link| order_in_link(link)));
    let Some(order) = order else {
        return kept;
    };
    let order_url = links
        .iter()
        .filter(|link| has_word(link, &order))
        .min_by_key(|link| !link.to_ascii_lowercase().contains("track"))
        .cloned();
    if kept.is_empty() && subject_said(subject).is_some() {
        kept.push(Sighting {
            number: None,
            order: None,
            carrier: named.map(String::from),
            url: None,
            order_url: None,
            state,
        });
    }
    for sighting in &mut kept {
        sighting.order.get_or_insert_with(|| order.clone());
        if sighting.order_url.is_none() {
            sighting.order_url.clone_from(&order_url);
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
                    let order = fields
                        .get("partOfOrder")
                        .and_then(|order| order.get("orderNumber"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|order| order_token(order))
                        .map(String::from);
                    found.push(Sighting {
                        number: Some(number),
                        order,
                        carrier,
                        url: fields
                            .get("trackingUrl")
                            .and_then(serde_json::Value::as_str)
                            .filter(|url| url.starts_with("https://"))
                            .map(String::from),
                        order_url: None,
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

/// A link's query, name and value, as written.
fn query(link: &str) -> impl Iterator<Item = (&str, &str)> {
    let query = link.split_once('?').map_or("", |(_, query)| query);
    query
        .split('#')
        .next()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
}

/// A tracking number any link names, as a shop's tracking page takes it:
/// `track_sn=…`, `trackingNumber=…`.
fn tracked_in_link(link: &str) -> Option<String> {
    query(link)
        .find(|(name, value)| name.to_ascii_lowercase().contains("track") && plausible(value))
        .map(|(_, value)| value.to_string())
}

/// An order number a link names: `orderId=…`, `tradeOrderId=…`.
fn order_in_link(link: &str) -> Option<String> {
    query(link)
        .find(|(name, value)| name.to_ascii_lowercase().contains("order") && order_token(value))
        .map(|(_, value)| value.to_string())
}

/// The carrier a mail is from.
fn sender(from_address: &str) -> Option<&'static str> {
    let domain = from_address
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    carrier_at(domain).map(|carrier| carrier.name)
}

/// The one carrier the mail is from or names, if only one.
fn carrier_named(from_address: &str, text: &str) -> Option<&'static str> {
    if let Some(carrier) = sender(from_address) {
        return Some(carrier);
    }
    let mut named = CARRIERS
        .iter()
        .filter(|carrier| carrier.words.iter().any(|word| has_word(text, word)));
    match (named.next(), named.next()) {
        (Some(carrier), None) => Some(carrier.name),
        _ => None,
    }
}

/// Where `word` is in `text`, as written, not inside another word.
fn words_at<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .filter(move |(at, _)| {
            let before = text[..*at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
        .map(|(at, _)| at)
}

/// `word` in `text`, as written, not inside another word.
fn has_word(text: &str, word: &str) -> bool {
    words_at(text, word).next().is_some()
}

/// The carrier only it writes a number like this.
fn carrier_of(number: &str) -> Option<&'static str> {
    if number.starts_with("1Z") && number.len() == 18 {
        Some("UPS")
    } else if number.starts_with("JJD") || number.starts_with("JVGL") {
        Some("DHL")
    } else if number.starts_with("TBA") {
        Some("Amazon")
    } else if is_s10(number) {
        post_of(&number[11..])
    } else {
        None
    }
}

/// Who delivers a post's number, by the country it ends in.
fn post_of(country: &str) -> Option<&'static str> {
    match country {
        "DE" => Some("DHL"),
        "SE" | "DK" => Some("PostNord"),
        "NO" => Some("Bring"),
        "FR" => Some("Colissimo"),
        "GB" => Some("Royal Mail"),
        "US" => Some("USPS"),
        _ => None,
    }
}

/// A post's number, as the UPU writes it: `RR123456785DE`, the ninth
/// digit checking the eight before it.
fn is_s10(number: &str) -> bool {
    let bytes = number.as_bytes();
    if !(bytes.len() == 13
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..11].iter().all(u8::is_ascii_digit)
        && bytes[11..].iter().all(u8::is_ascii_uppercase))
    {
        return false;
    }
    let sum: u32 = bytes[2..10]
        .iter()
        .zip([8, 6, 4, 2, 3, 5, 9, 7])
        .map(|(digit, weight)| u32::from(digit - b'0') * weight)
        .sum();
    let check = match 11 - sum % 11 {
        10 => 0,
        11 => 5,
        check => check,
    };
    u32::from(bytes[10] - b'0') == check
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

/// What could be a shop's order number: 5 to 30 capitals, digits and
/// dashes, at least 4 of them digits, and not a date.
fn order_token(word: &str) -> bool {
    let bytes = word.as_bytes();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    let date = bytes.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-' && digits == 8;
    (5..=30).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() || *b == b'-')
        && digits >= 4
        && !date
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
    "parcel id",
    "package id",
    "shipment number",
    "waybill",
    "spårningsnummer",
    "spårningsnr",
    "spårnings-id",
    "spårningsid",
    "spårningskod",
    "sändningsnummer",
    "sändnings-id",
    "sändningsid",
    "kollinummer",
    "kolli-id",
    "paket-id",
    "paketid",
    "försändelse-id",
    "sporingsnummer",
    "pakkenummer",
    "numéro de suivi",
    "numéro de colis",
    "numéro d'envoi",
    "n° de suivi",
    "n° de colis",
];

/// What a mail calls its parcel, its number right after: "Package 0012…".
const PARCEL_WORDS: &[&str] = &[
    "package",
    "parcel",
    "shipment",
    "paket",
    "sendung",
    "kolli",
    "försändelse",
    "forsendelse",
    "sändning",
    "colis",
];

/// Whether a mail is about a parcel at all.
const ABOUT_PARCELS: &[&str] = &[
    "sendung",
    "paket",
    "tracking",
    "shipment",
    "shipped",
    "dispatch",
    "versand",
    "versendet",
    "verschickt",
    "zustell",
    "lieferung",
    "parcel",
    "package",
    "delivery",
    "pickup",
    "pick up",
    "leverans",
    "levere",
    "försändelse",
    "spårning",
    "skickat",
    "skickad",
    "colis",
    "livraison",
    "expédi",
    "suivi",
];

/// The numbers a mail's words give: after a label or a parcel's own word,
/// and those only a carrier writes, in a mail about a parcel.
fn written(subject: &str, text: &str, about: bool) -> Vec<String> {
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
    for word in PARCEL_WORDS {
        for at in words_at(&lower, word) {
            let next = all[at + word.len()..]
                .split_whitespace()
                .map(trimmed)
                .find(|next| !next.is_empty());
            if let Some(next) = next
                && plausible(next)
                && !found.iter().any(|f| f == next)
            {
                found.push(next.to_string());
            }
        }
    }
    if about {
        for word in all.split(|c: char| !c.is_ascii_alphanumeric()) {
            let known = carrier_of(word).is_some() || is_s10(word);
            if known && plausible(word) && !found.iter().any(|f| f == word) {
                found.push(word.to_string());
            }
        }
    }
    found
}

/// A word without the marks around it: `#1234567:` is `1234567`.
fn trimmed(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_ascii_alphanumeric())
}

/// The first number in the few words after a label, its groups joined
/// when it's written in them: `0034 0434 1234 5678 9012`.
fn first_number(window: &str) -> Option<String> {
    let words: Vec<&str> = window.split_whitespace().map(trimmed).take(6).collect();
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

/// What a mail calls the shop's order.
const ORDER_WORDS: &[&str] = &[
    "order",
    "ordernummer",
    "ordernr",
    "beställning",
    "beställningsnummer",
    "bestellung",
    "bestellnummer",
    "commande",
];

/// The shop's order number: after "Order", "Bestellung", "Beställning"…
/// anywhere in the subject, else within a few words of it in the text.
fn order_of(subject: &str, text: &str) -> Option<String> {
    [(subject, usize::MAX), (text, 5)]
        .into_iter()
        .find_map(|(part, reach)| {
            let lower = part.to_ascii_lowercase();
            let mut at: Vec<(usize, usize)> = ORDER_WORDS
                .iter()
                .flat_map(|word| words_at(&lower, word).map(|at| (at, word.len())))
                .collect();
            at.sort_unstable();
            at.into_iter().find_map(|(at, len)| {
                part[at + len..]
                    .split_whitespace()
                    .map(trimmed)
                    .filter(|word| !word.is_empty())
                    .take(reach)
                    .find(|word| order_token(word))
                    .map(String::from)
            })
        })
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
    "har levererats",
    "har lämnats ut",
    "a été livré",
];
const PICK_UP: &[&str] = &[
    "abholbereit",
    "zur abholung",
    "abgeholt werden",
    "ready for pickup",
    "ready for pick-up",
    "ready for pick up",
    "ready to pick up",
    "ready for collection",
    "ready to be picked up",
    "available for pickup",
    "redo att hämtas",
    "redo för upphämtning",
    "finns att hämta",
    "kan hämtas",
    "disponible en point",
    "prêt à être retiré",
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
    "ute för leverans",
    "ute för utkörning",
    "levereras idag",
    "levereras i dag",
    "kommer idag",
    "kommer i dag",
    "en cours de livraison",
];
/// What a subject says when the parcel left, and nothing more.
const ON_ITS_WAY: &[&str] = &[
    "shipped",
    "dispatched",
    "on its way",
    "on the way",
    "in transit",
    "collected by the carrier",
    "delivery attempted",
    "arriving",
    "versendet",
    "versandt",
    "verschickt",
    "unterwegs",
    "auf dem weg",
    "zustellversuch",
    "på väg",
    "skickat",
    "skickad",
    "har skickats",
    "leveransförsök",
    "expédié",
    "en route",
    "en cours d'acheminement",
];

/// Where the words say a parcel is, the furthest first.
fn said(text: &str) -> Option<ParcelState> {
    let lower = text.to_lowercase();
    [
        (DELIVERED, ParcelState::Delivered),
        (PICK_UP, ParcelState::PickUp),
        (OUT_FOR_DELIVERY, ParcelState::OutForDelivery),
    ]
    .into_iter()
    .find(|(words, _)| words.iter().any(|word| lower.contains(word)))
    .map(|(_, state)| state)
}

/// Where a subject says its parcel is, if it says it left at all.
fn subject_said(subject: &str) -> Option<ParcelState> {
    said(subject).or_else(|| {
        let lower = subject.to_lowercase();
        if ["zugestellt", "delivered", "levererad", "levererat", "livré"]
            .iter()
            .any(|word| lower.contains(word))
        {
            Some(ParcelState::Delivered)
        } else if ON_ITS_WAY.iter().any(|word| lower.contains(word)) {
            Some(ParcelState::Shipped)
        } else {
            None
        }
    })
}

/// Where the mail says its parcel is: its subject first, then its text.
fn state_of(subject: &str, text: &str) -> ParcelState {
    subject_said(subject)
        .or_else(|| said(text))
        .unwrap_or(ParcelState::Shipped)
}

/// `GET /api/w/{workspace}/parcels`.
pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Parcels>, AppError> {
    Ok(Json(state.mail.parcels(member.actor.workspace_id).await?))
}

/// `POST /api/w/{workspace}/parcels/delivered`: you say a parcel came, or
/// take that back.
pub async fn delivered(
    State(state): State<AppState>,
    member: Member,
    Json(mark): Json<ParcelDelivered>,
) -> Result<StatusCode, AppError> {
    let key = mark.key.trim();
    if key.is_empty() || key.len() > 200 {
        return Err(AppError::invalid("That isn't a parcel."));
    }
    let query = if mark.delivered {
        "INSERT INTO delivered_parcels (workspace_id, key) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM delivered_parcels WHERE workspace_id = $1 AND key = $2"
    };
    sqlx::query(query)
        .bind(member.actor.workspace_id)
        .bind(key)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(headers: &str, body: &str) -> Vec<u8> {
        format!("From: x@shop.example\r\n{headers}\r\n\r\n{body}").into_bytes()
    }

    fn received(days: i64, from: &str) -> MailSummary {
        MailSummary {
            account_id: Uuid::nil(),
            folder: None,
            uid: days,
            from_name: "Shop".into(),
            from_address: from.into(),
            subject: String::new(),
            snippet: String::new(),
            date: Utc::now() - Duration::days(days),
            seen: true,
            flagged: false,
            to_name: None,
            unsent: false,
        }
    }

    #[test]
    fn reads_a_labelled_number_and_the_carrier_the_mail_names() {
        let raw = mail(
            "Subject: Ihre Bestellung ist unterwegs\r\nContent-Type: text/plain; charset=utf-8",
            "Wir haben Ihr Paket an DHL übergeben.\nSendungsnummer: 0034 0434 1234 5678 9012\n",
        );
        let found = sightings(&raw, "shop@shop.example", "Ihre Bestellung ist unterwegs");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("00340434123456789012"));
        assert_eq!(found[0].carrier.as_deref(), Some("DHL"));
        assert_eq!(found[0].state, ParcelState::Shipped);
        assert_eq!(
            page(found[0].carrier.as_deref(), "00340434123456789012", None),
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
               "partOfOrder":{"@type":"Order","orderNumber":"A-10042","orderStatus":"http://schema.org/OrderDelivered"}}</script><p>Hallo</p>"#,
        );
        let found = sightings(&raw, "a@shop.example", "Versandbestätigung");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("09876543210123"));
        assert_eq!(found[0].order.as_deref(), Some("A-10042"));
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
    fn a_carriers_link_and_a_posts_number_from_beyond_germany() {
        let subject = "Your parcel is on its way XT123456785TS";
        let raw = mail(
            &format!("Subject: {subject}\r\nContent-Type: text/html"),
            "<p>Your parcel SHOP XT123456785TS / 01234567890123K is being handled in our network.</p>\
             <a href=\"https://www.chronopost.fr/tracking-no-cms/suivi-page?listeNumerosLT=XT123456785TS&amp;langue=en_GB\">here</a>",
        );
        let found = sightings(&raw, "avisage@chronopost.fr", subject);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("XT123456785TS"));
        assert_eq!(found[0].carrier.as_deref(), Some("Chronopost"));
        assert_eq!(found[0].state, ParcelState::Shipped);

        // A wrong check digit is no post's number.
        assert!(is_s10("RR123456785DE"));
        assert!(!is_s10("RR123456784DE"));
        assert_eq!(carrier_of("UO123456785SE"), Some("PostNord"));
        let subject = "Your parcel is on its way XT123456784TS";
        let raw = mail(&format!("Subject: {subject}"), "Your parcel is coming.\n");
        assert!(sightings(&raw, "a@shop.example", subject).is_empty());
    }

    #[test]
    fn reads_swedish_labels_and_trusts_the_subject_over_the_text() {
        let subject = "Leverans på väg!";
        let raw = mail(
            &format!("Subject: {subject}\r\nContent-Type: text/plain; charset=utf-8"),
            "Så snart ditt paket är redo att hämtas ut får du en pin-kod.\nPaket-ID\nKA012345678901\n",
        );
        let found = sightings(&raw, "info@instabox.se", subject);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("KA012345678901"));
        assert_eq!(found[0].carrier.as_deref(), Some("Instabox"));
        assert_eq!(found[0].state, ParcelState::Shipped);

        let subject = "Ditt paket är på väg!";
        let raw = mail(
            &format!("Subject: {subject}\r\nContent-Type: text/plain; charset=utf-8"),
            "Din beställning nr. 10042 hanteras av PostNord.\n\
             Spåra mitt paket ( https://tracking.postnord.com/se/tracking?id=01234567890123 )\n\
             *Spårningsnummer:* 01234567890123\n",
        );
        let found = sightings(&raw, "hello@shop.example", subject);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("01234567890123"));
        assert_eq!(found[0].carrier.as_deref(), Some("PostNord"));
        assert_eq!(found[0].order.as_deref(), Some("10042"));
    }

    #[test]
    fn an_order_on_its_way_is_a_parcel_until_its_number_comes() {
        let shop = "transaction@notice.shop.example";
        let shipped = "Order 1234567890123456: order shipped";
        let raw = mail(
            &format!("Subject: {shipped}\r\nContent-Type: text/html"),
            "<p>Your order 1234567890123456 has shipped.</p>\
             <a href=\"https://www.shop.example/p/order/detail.html?orderId=1234567890123456&amp;x=1\">Track order</a>",
        );
        let ordered = sightings(&raw, shop, shipped);
        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].number, None);
        assert_eq!(ordered[0].order.as_deref(), Some("1234567890123456"));
        assert_eq!(ordered[0].state, ParcelState::Shipped);

        // Only once it left: a confirmed order is no parcel yet.
        let confirmed = "Order 1234567890123456: order confirmed";
        let raw = mail(
            &format!("Subject: {confirmed}\r\nContent-Type: text/plain"),
            "Your order 1234567890123456 is confirmed.\n",
        );
        assert!(sightings(&raw, shop, confirmed).is_empty());

        // Its number comes with the order in the shop's tracking link.
        let coming = "Package 00123456789012345678 is out for delivery";
        let raw = mail(
            &format!("Subject: {coming}\r\nContent-Type: text/html"),
            "<p>Your package 00123456789012345678 is out for delivery.</p>\
             <a href=\"https://www.shop.example/p/tracking/index.html?tradeOrderId=1234567890123456\">Track</a>",
        );
        let numbered = sightings(&raw, shop, coming);
        assert_eq!(numbered.len(), 1);
        assert_eq!(numbered[0].number.as_deref(), Some("00123456789012345678"));
        assert_eq!(numbered[0].order.as_deref(), Some("1234567890123456"));

        let (old, new) = (received(3, shop), received(1, shop));
        let parcels = gather(
            vec![(&old, &ordered[0]), (&new, &numbered[0])],
            &Marks::new(),
            Utc::now(),
        );
        assert_eq!(parcels.len(), 1);
        assert_eq!(parcels[0].number.as_deref(), Some("00123456789012345678"));
        assert_eq!(parcels[0].state, ParcelState::OutForDelivery);
        assert_eq!(
            parcels[0].track_url.as_deref(),
            Some("https://www.shop.example/p/tracking/index.html?tradeOrderId=1234567890123456")
        );

        // Alone, the order is followed on the shop's page for it.
        let parcels = gather(vec![(&old, &ordered[0])], &Marks::new(), Utc::now());
        assert_eq!(
            parcels[0].track_url.as_deref(),
            Some("https://www.shop.example/p/order/detail.html?orderId=1234567890123456&x=1")
        );

        // Another shop's order of the same number is another parcel.
        let elsewhere = received(2, "orders@other.example");
        let parcels = gather(
            vec![(&old, &ordered[0]), (&elsewhere, &ordered[0])],
            &Marks::new(),
            Utc::now(),
        );
        assert_eq!(parcels.len(), 2);
    }

    #[test]
    fn a_shops_tracking_link_names_the_number() {
        let subject = "Your order shipped and its tracking number is ready (#PO-191-0123456789)";
        let raw = mail(
            &format!("Subject: {subject}\r\nContent-Type: text/html"),
            "<a href=\"https://www.shop.example/logistic.html?provider_id=7&amp;track_sn=1234567890123\">Track</a>",
        );
        let found = sightings(&raw, "orders@shop.example", subject);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number.as_deref(), Some("1234567890123"));
        assert_eq!(found[0].order.as_deref(), Some("PO-191-0123456789"));
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
            page(None, "ABC1234567890", None),
            "https://parcelsapp.com/en/tracking/ABC1234567890"
        );
    }

    #[test]
    fn a_parcel_takes_its_newest_mail_and_furthest_state() {
        let now = Utc::now();
        let seen = |state| Sighting {
            number: Some("00340434123456789012".into()),
            order: None,
            carrier: Some("DHL".into()),
            url: None,
            order_url: None,
            state,
        };
        let (old, new) = (received(3, "a@shop.example"), received(1, "a@shop.example"));
        let (shipped, delivered) = (seen(ParcelState::Shipped), seen(ParcelState::Delivered));
        let parcels = gather(
            vec![(&old, &shipped), (&new, &delivered)],
            &Marks::new(),
            now,
        );
        assert_eq!(parcels.len(), 1);
        assert_eq!(parcels[0].state, ParcelState::Delivered);
        assert_eq!(parcels[0].mail.uid, 1);
        assert!(
            parcels[0]
                .track_url
                .as_deref()
                .is_some_and(|url| url.starts_with("https://www.dhl.de/"))
        );

        let long_ago = received(10, "a@shop.example");
        assert!(gather(vec![(&long_ago, &delivered)], &Marks::new(), now).is_empty());
    }

    #[test]
    fn a_parcel_you_said_came_is_delivered_until_you_take_it_back() {
        let now = Utc::now();
        let seen = |number: Option<&str>| Sighting {
            number: number.map(Into::into),
            order: Some("123456".into()),
            carrier: None,
            url: None,
            order_url: None,
            state: ParcelState::Shipped,
        };
        let (mail, later) = (
            received(20, "orders@shop.example"),
            received(19, "orders@shop.example"),
        );
        let (order, numbered) = (seen(None), seen(Some("00340434123456789012")));
        let marked = |at| Marks::from([("shop.example 123456".to_string(), at)]);

        // Said while only its order was known, long after its mail.
        let parcels = gather(vec![(&mail, &order)], &marked(now), now);
        assert_eq!(parcels[0].state, ParcelState::Delivered);
        assert_eq!(parcels[0].marked_at, Some(now));
        assert_eq!(parcels[0].key, "shop.example 123456");

        // The number its shop gives later keeps what you said, and its key.
        let parcels = gather(
            vec![(&mail, &order), (&later, &numbered)],
            &marked(now),
            now,
        );
        assert_eq!(parcels.len(), 1);
        assert_eq!(parcels[0].state, ParcelState::Delivered);
        assert_eq!(parcels[0].key, "shop.example 123456");

        // It goes a week after you said so.
        let week_ago = now - Duration::days(8);
        assert!(gather(vec![(&mail, &order)], &marked(week_ago), now).is_empty());

        // Taken back, it's on its way again, by its number.
        let parcels = gather(
            vec![(&mail, &order), (&later, &numbered)],
            &Marks::new(),
            now,
        );
        assert_eq!(parcels[0].state, ParcelState::Shipped);
        assert_eq!(parcels[0].marked_at, None);
        assert_eq!(parcels[0].key, "00340434123456789012");
    }
}
