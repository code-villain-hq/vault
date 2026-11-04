//! Reading the calendar server over CalDAV (RFC 4791), as Radicale serves
//! it: finding the calendars, then each one's events in a span of time,
//! repeats expanded by the server.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use postrider_domain::{Calendar, Day, Event, MailAccount, ServerCalendar};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

const TIMEOUT: Duration = Duration::from_secs(20);

/// How far ahead Settings looks for a calendar's next event.
const AHEAD_DAYS: i64 = 28;

/// A calendar on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub href: String,
    pub name: String,
}

/// The calendar server, signed in to.
pub struct Server {
    http: reqwest::Client,
    base: reqwest::Url,
    username: String,
    password: String,
}

impl Server {
    pub fn new(url: &str, username: &str, password: &str) -> Result<Server, String> {
        let base = reqwest::Url::parse(url).map_err(|e| format!("{url}: {e}"))?;
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Server {
            http,
            base,
            username: username.into(),
            password: password.into(),
        })
    }

    /// The server's host, for which account its calendars belong to.
    pub fn host(&self) -> &str {
        self.base.host_str().unwrap_or_default()
    }

    /// Every calendar the person has: their principal, its calendar home,
    /// and what's in it.
    pub async fn calendars(&self) -> Result<Vec<Found>, String> {
        const PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#;
        const HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><c:calendar-home-set/></d:prop></d:propfind>"#;
        const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:resourcetype/><d:displayname/><c:supported-calendar-component-set/></d:prop></d:propfind>"#;

        let base = self.base.path().to_string();
        let principal = self
            .propfind(&base, "0", PRINCIPAL)
            .await?
            .find_path(&[
                "response",
                "propstat",
                "prop",
                "current-user-principal",
                "href",
            ])
            .map(|n| n.text.trim().to_string())
            .unwrap_or(base);
        let home = self
            .propfind(&principal, "0", HOME)
            .await?
            .find_path(&["response", "propstat", "prop", "calendar-home-set", "href"])
            .map(|n| n.text.trim().to_string())
            .unwrap_or(principal);
        Ok(found_calendars(&self.propfind(&home, "1", LIST).await?))
    }

    /// A calendar's events that overlap `from`..`to`, repeats expanded.
    pub async fn events(
        &self,
        calendar: &Found,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Parsed>, String> {
        let (start, end) = (caldav_time(from), caldav_time(to));
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
<d:prop><c:calendar-data><c:expand start="{start}" end="{end}"/></c:calendar-data></d:prop>
<c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT">
<c:time-range start="{start}" end="{end}"/>
</c:comp-filter></c:comp-filter></c:filter>
</c:calendar-query>"#
        );
        let reply = self.request("REPORT", &calendar.href, "1", body).await?;
        let mut events = Vec::new();
        for response in reply.children_named("response") {
            let data = response
                .children_named("propstat")
                .filter_map(|p| p.child("prop"))
                .filter_map(|p| p.child("calendar-data"))
                .next();
            if let Some(data) = data {
                events.extend(
                    parse_ics(&data.text)
                        .into_iter()
                        .filter(|e| e.start < to && (e.end > from || e.start >= from)),
                );
            }
        }
        Ok(events)
    }

    async fn propfind(&self, path: &str, depth: &str, body: &str) -> Result<Node, String> {
        self.request("PROPFIND", path, depth, body.to_string())
            .await
    }

    async fn request(
        &self,
        method: &str,
        path: &str,
        depth: &str,
        body: String,
    ) -> Result<Node, String> {
        let url = self.base.join(path).map_err(|e| e.to_string())?;
        let method = Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let response = self
            .http
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("{}: {}", self.host(), plain_error(&e)))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(format!("{} refused the password.", self.host()));
            }
            status if !status.is_success() => {
                return Err(format!("{} answered {status}.", self.host()));
            }
            _ => {}
        }
        let text = response.text().await.map_err(|e| e.to_string())?;
        parse_xml(&text)
    }
}

/// What reqwest says went wrong, without its chain of causes repeated.
fn plain_error(e: &reqwest::Error) -> String {
    let mut message = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(cause) = source {
        message = cause.to_string();
        source = cause.source();
    }
    message
}

/// The calendars in a calendar home's listing, by name: the rest of what's
/// in it isn't one, nor is a list of tasks, which holds no events.
fn found_calendars(listing: &Node) -> Vec<Found> {
    let mut found: Vec<Found> = listing
        .children_named("response")
        .filter_map(|response| {
            let href = response.child("href")?.text.trim().to_string();
            let prop = response
                .children_named("propstat")
                .filter_map(|p| p.child("prop"))
                .find(|p| p.child("resourcetype").is_some())?;
            prop.child("resourcetype")?.child("calendar")?;
            if let Some(set) = prop.child("supported-calendar-component-set") {
                set.children.iter().find(|c| {
                    c.name == "comp"
                        && c.attributes
                            .iter()
                            .any(|(k, v)| k == "name" && v.eq_ignore_ascii_case("VEVENT"))
                })?;
            }
            // Radicale names a calendar without a name by its path, and an
            // app that made one may leave its own placeholder:
            // DEFAULT_CALENDAR_NAME.
            let name = prop
                .child("displayname")
                .map(|n| n.text.trim().to_string())
                .filter(|n| !n.is_empty() && n != href.trim_matches('/') && !is_placeholder(n))
                .unwrap_or_else(|| last_segment(&href));
            Some(Found { href, name })
        })
        .collect();
    found.sort_by_key(|f| f.name.to_lowercase());
    found
}

/// A name no one gave: an app's key for words it didn't fill in.
fn is_placeholder(name: &str) -> bool {
    name.contains('_')
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn caldav_time(time: DateTime<Utc>) -> String {
    time.format("%Y%m%dT%H%M%SZ").to_string()
}

fn last_segment(href: &str) -> String {
    href.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(href)
        .to_string()
}

/// The account whose colour a calendar wears: the one the person chose for
/// it, else the one whose name is in the calendar's ("Code Villain"), else
/// the one at the calendar server's domain, else the first.
pub fn account_for<'a>(
    calendar: &Found,
    host: &str,
    accounts: &'a [MailAccount],
    chosen: &BTreeMap<String, Uuid>,
) -> Option<&'a MailAccount> {
    let chosen = || {
        let id = chosen.get(&calendar.href)?;
        accounts.iter().find(|a| a.id == *id)
    };
    let name = calendar.name.to_lowercase();
    let named = || {
        accounts
            .iter()
            .find(|a| name.contains(&a.name.to_lowercase()))
    };
    let at_domain = || {
        accounts.iter().find(|a| {
            a.address
                .rsplit_once('@')
                .is_some_and(|(_, domain)| host == domain || host.ends_with(&format!(".{domain}")))
        })
    };
    chosen()
        .or_else(named)
        .or_else(at_domain)
        .or(accounts.first())
}

/// An event, as read from the calendar.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
}

/// The events in an iCalendar text (RFC 5545): each VEVENT's summary and
/// its start and end, in UTC. Times with a TZID are read in that zone;
/// times with none, in this Mac's. Cancelled events are left out.
pub fn parse_ics(text: &str) -> Vec<Parsed> {
    let unfolded = text
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "");
    let mut events = Vec::new();
    let mut depth = 0usize;
    let mut event: Option<Draft> = None;
    for line in unfolded.lines() {
        let Some((head, value)) = split_property(line) else {
            continue;
        };
        let mut params = head.split(';');
        let name = params.next().unwrap_or_default().to_ascii_uppercase();
        let params: Vec<(String, String)> = params
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
            .collect();
        let param = |key: &str| {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        match (name.as_str(), value) {
            ("BEGIN", "VEVENT") if event.is_none() => {
                event = Some(Draft::default());
                depth = 0;
            }
            ("BEGIN", _) if event.is_some() => depth += 1,
            ("END", "VEVENT") if depth == 0 => {
                if let Some(parsed) = event.take().and_then(Draft::finish) {
                    events.push(parsed);
                }
            }
            ("END", _) if event.is_some() => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            (name, value) => {
                let Some(draft) = event.as_mut() else {
                    continue;
                };
                match name {
                    "SUMMARY" => draft.title = Some(unescape(value)),
                    "DTSTART" => draft.start = parse_time(value, param("VALUE"), param("TZID")),
                    "DTEND" => draft.end = parse_time(value, param("VALUE"), param("TZID")),
                    "DURATION" => draft.duration = parse_duration(value),
                    "STATUS" => draft.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
        }
    }
    events
}

#[derive(Default)]
struct Draft {
    title: Option<String>,
    start: Option<(DateTime<Utc>, bool)>,
    end: Option<(DateTime<Utc>, bool)>,
    duration: Option<chrono::Duration>,
    cancelled: bool,
}

impl Draft {
    fn finish(self) -> Option<Parsed> {
        if self.cancelled {
            return None;
        }
        let (start, all_day) = self.start?;
        // An end past the last date there is (`P999999999D`) is no event
        // anyone keeps, and adding it would panic: it's left out.
        let end = match (self.end, self.duration) {
            (Some((end, _)), _) => end,
            (None, Some(duration)) => start.checked_add_signed(duration)?,
            (None, None) if all_day => start.checked_add_signed(chrono::Duration::days(1))?,
            (None, None) => start,
        };
        Some(Parsed {
            title: self.title.unwrap_or_default(),
            start,
            end: end.max(start),
            all_day,
        })
    }
}

/// `NAME;PARAM="a:b":value` → (`NAME;PARAM="a:b"`, `value`): the first
/// colon outside quotes.
fn split_property(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// A DTSTART or DTEND, and whether it's a whole day.
fn parse_time(
    value: &str,
    kind: Option<&str>,
    tzid: Option<&str>,
) -> Option<(DateTime<Utc>, bool)> {
    let value = value.trim();
    if kind.is_some_and(|k| k.eq_ignore_ascii_case("DATE")) || value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some((local(date.and_hms_opt(0, 0, 0)?)?, true));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let time = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some((time.and_utc(), false));
    }
    let time = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let zoned = tzid
        .and_then(|id| id.parse::<chrono_tz::Tz>().ok())
        .and_then(|tz| tz.from_local_datetime(&time).earliest())
        .map(|t| t.with_timezone(&Utc));
    Some((zoned.or_else(|| local(time))?, false))
}

/// A time on this Mac's clock.
fn local(time: NaiveDateTime) -> Option<DateTime<Utc>> {
    chrono::Local
        .from_local_datetime(&time)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
}

/// `PT1H30M`, `P1D`, `-PT15M`, `P1W`. One too long to count is as long as
/// a duration can be, so the event it ends is left out.
fn parse_duration(value: &str) -> Option<chrono::Duration> {
    let (negative, value) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let value = value.strip_prefix('P')?;
    // None once it's too long for an i64.
    let mut seconds = Some(0i64);
    let mut number = String::new();
    for c in value.chars() {
        match c {
            'T' => {}
            '0'..='9' => number.push(c),
            unit => {
                if number.is_empty() {
                    return None;
                }
                // Only digits are in it: a number that won't parse is too big.
                let n = number.parse::<i64>().ok();
                number.clear();
                let unit = match unit {
                    'W' => 7 * 86400,
                    'D' => 86400,
                    'H' => 3600,
                    'M' => 60,
                    'S' => 1,
                    _ => return None,
                };
                seconds = seconds
                    .zip(n)
                    .and_then(|(seconds, n)| seconds.checked_add(n.checked_mul(unit)?));
            }
        }
    }
    let duration = seconds
        .and_then(chrono::Duration::try_seconds)
        .unwrap_or(chrono::Duration::MAX);
    Some(if negative { -duration } else { duration })
}

/// An XML element: its local name (namespaces dropped), its text and its
/// children. Enough of WebDAV's replies to read them.
#[derive(Debug, Default)]
pub struct Node {
    pub name: String,
    /// Its attributes, by local name.
    pub attributes: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children_named<'a, 'n>(
        &'a self,
        name: &'n str,
    ) -> impl Iterator<Item = &'a Node> + use<'a, 'n> {
        // The reply's root is `multistatus`, under the document.
        let parent = match self.children.as_slice() {
            [root] if root.name == "multistatus" => root,
            _ => self,
        };
        parent.children.iter().filter(move |c| c.name == name)
    }

    /// The first element down `path`, from the root's children on.
    fn find_path<'a>(&'a self, path: &[&'a str]) -> Option<&'a Node> {
        let mut nodes: Vec<&Node> = self.children_named(path[0]).collect();
        for name in &path[1..] {
            nodes = nodes
                .into_iter()
                .flat_map(|n| n.children.iter().filter(|c| c.name == *name))
                .collect();
        }
        nodes.into_iter().next()
    }
}

pub fn parse_xml(xml: &str) -> Result<Node, String> {
    use quick_xml::events::Event as Xml;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack = vec![Node::default()];
    let name = |local: quick_xml::name::LocalName<'_>| local.as_ref().to_string();
    let attributes = |e: &quick_xml::events::BytesStart<'_>| {
        e.attributes()
            .flatten()
            .map(|a| {
                let key = a.key.local_name().into_inner().to_string();
                (key, a.value.to_string())
            })
            .collect()
    };
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("the calendar server's reply: {e}"))?;
        match event {
            Xml::Start(e) => stack.push(Node {
                name: name(e.local_name()),
                attributes: attributes(&e),
                ..Node::default()
            }),
            Xml::Empty(e) => {
                let node = Node {
                    name: name(e.local_name()),
                    attributes: attributes(&e),
                    ..Node::default()
                };
                stack.last_mut().expect("the document").children.push(node);
            }
            Xml::End(_) if stack.len() > 1 => {
                let node = stack.pop().expect("checked");
                stack.last_mut().expect("the document").children.push(node);
            }
            Xml::Text(t) => stack
                .last_mut()
                .expect("the document")
                .text
                .push_str(&t.xml10_content()),
            Xml::CData(t) => stack
                .last_mut()
                .expect("the document")
                .text
                .push_str(&t.into_inner()),
            Xml::GeneralRef(r) => {
                let text = match r.resolve_char_ref() {
                    Ok(Some(c)) => c.to_string(),
                    _ => match r.into_inner().as_ref() {
                        "amp" => "&".into(),
                        "lt" => "<".into(),
                        "gt" => ">".into(),
                        "quot" => "\"".into(),
                        "apos" => "'".into(),
                        _ => String::new(),
                    },
                };
                stack.last_mut().expect("the document").text.push_str(&text);
            }
            Xml::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().expect("checked");
        stack.last_mut().expect("the document").children.push(node);
    }
    Ok(stack.pop().expect("the document"))
}

#[derive(Deserialize)]
pub struct DayParams {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

/// `GET /api/w/{workspace}/day?from=&to=`: the events in a day (the web
/// app sends its own day's bounds), from every calendar on the server,
/// each in its account's hue.
pub async fn day(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<DayParams>,
) -> Result<Json<Day>, AppError> {
    if params.to <= params.from || params.to - params.from > chrono::Duration::days(42) {
        return Err(AppError::invalid("A day is from a time to a later one."));
    }
    let Some((calendar, accounts)) = kept(&state, member.actor.workspace_id).await? else {
        return Ok(Json(Day {
            events: Vec::new(),
            error: None,
            read_at: None,
        }));
    };
    Ok(Json(
        match read_day(&state, &calendar, &accounts, params.from, params.to).await {
            Ok(events) => {
                keep_day(&state, calendar.id, &params, &events).await?;
                Day {
                    events,
                    error: None,
                    read_at: None,
                }
            }
            Err(error) => {
                let (events, read_at) = kept_day(&state, calendar.id, &params)
                    .await?
                    .map_or((Vec::new(), None), |(events, at)| (events, Some(at)));
                Day {
                    events,
                    error: Some(error),
                    read_at,
                }
            }
        },
    ))
}

/// Keeps the day just read, in place of the one before.
async fn keep_day(
    state: &AppState,
    calendar_id: Uuid,
    day: &DayParams,
    events: &[Event],
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO kept_days (calendar_id, day_from, day_to, events, read_at) \
         VALUES ($1, $2, $3, $4, now()) \
         ON CONFLICT (calendar_id) DO UPDATE SET day_from = $2, day_to = $3, events = $4, read_at = now()",
    )
    .bind(calendar_id)
    .bind(day.from)
    .bind(day.to)
    .bind(serde_json::to_value(events).map_err(AppError::internal)?)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// The day last read from the calendar, and when, if it's this one.
async fn kept_day(
    state: &AppState,
    calendar_id: Uuid,
    day: &DayParams,
) -> Result<Option<(Vec<Event>, DateTime<Utc>)>, AppError> {
    let kept: Option<(serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT events, read_at FROM kept_days \
         WHERE calendar_id = $1 AND day_from = $2 AND day_to = $3",
    )
    .bind(calendar_id)
    .bind(day.from)
    .bind(day.to)
    .fetch_optional(&state.pool)
    .await?;
    Ok(kept.and_then(|(events, at)| Some((serde_json::from_value(events).ok()?, at))))
}

/// `GET /api/w/{workspace}/calendars`: the calendars on the server, for
/// Settings to say whose colour each wears.
pub async fn calendars(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<ServerCalendar>>, AppError> {
    let Some((calendar, accounts)) = kept(&state, member.actor.workspace_id).await? else {
        return Ok(Json(Vec::new()));
    };
    let listed = list_calendars(&state, &calendar, &accounts)
        .await
        .map_err(AppError::refused)?;
    Ok(Json(listed))
}

/// The workspace's calendar server and its mail accounts, when it has one.
async fn kept(
    state: &AppState,
    workspace_id: Uuid,
) -> Result<Option<(Calendar, Vec<MailAccount>)>, AppError> {
    let calendar: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(c.*) FROM calendars c WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some(calendar) = calendar else {
        return Ok(None);
    };
    let calendar: Calendar = serde_json::from_value(calendar).map_err(AppError::internal)?;
    let accounts: Vec<MailAccount> = sqlx::query_scalar(
        "SELECT to_jsonb(a.*) FROM mail_accounts a WHERE workspace_id = $1 ORDER BY position",
    )
    .bind(workspace_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .filter_map(|row: serde_json::Value| serde_json::from_value(row).ok())
    .collect();
    Ok(Some((calendar, accounts)))
}

/// The calendar server, signed in to with its password from the keychain.
async fn sign_in(state: &AppState, calendar: &Calendar) -> Result<Server, String> {
    let password = state
        .secrets
        .get(&calendar.url)
        .await?
        .ok_or("Its password isn't in the keychain. Add the calendar again.")?;
    Server::new(&calendar.url, &calendar.username, &password)
}

async fn read_day(
    state: &AppState,
    calendar: &Calendar,
    accounts: &[MailAccount],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Event>, String> {
    let server = sign_in(state, calendar).await?;
    let mut events = Vec::new();
    for found in server.calendars().await? {
        let hue =
            account_for(&found, server.host(), accounts, &calendar.accounts).map_or(1, |a| a.hue);
        for parsed in server.events(&found, from, to).await? {
            events.push(event(parsed, &found, hue));
        }
    }
    events.sort_by_key(|a| (a.start, a.end));
    Ok(events)
}

async fn list_calendars(
    state: &AppState,
    calendar: &Calendar,
    accounts: &[MailAccount],
) -> Result<Vec<ServerCalendar>, String> {
    let server = sign_in(state, calendar).await?;
    let from = Utc::now();
    let to = from + chrono::Duration::days(AHEAD_DAYS);
    let mut listed = Vec::new();
    for found in server.calendars().await? {
        let hue =
            account_for(&found, server.host(), accounts, &calendar.accounts).map_or(1, |a| a.hue);
        let next = server
            .events(&found, from, to)
            .await?
            .into_iter()
            .filter(|parsed| parsed.end > from)
            .min_by_key(|parsed| parsed.start)
            .map(|parsed| event(parsed, &found, hue));
        listed.push(ServerCalendar {
            default_account: account_for(&found, server.host(), accounts, &BTreeMap::new())
                .map(|a| a.id),
            href: found.href,
            name: found.name,
            next,
        });
    }
    Ok(listed)
}

fn event(parsed: Parsed, calendar: &Found, hue: i32) -> Event {
    Event {
        title: parsed.title,
        start: parsed.start,
        end: parsed.end,
        all_day: parsed.all_day,
        calendar: calendar.name.clone(),
        hue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_reads_in_utc() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Standup\\, team\r\n\
            DTSTART;TZID=Europe/Vienna:20261102T093000\r\nDURATION:PT15M\r\n\
            BEGIN:VALARM\r\nDTSTART:19700101T000000\r\nEND:VALARM\r\n\
            END:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261102\r\n\
            END:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Gone\r\nSTATUS:CANCELLED\r\n\
            DTSTART:20261102T120000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse_ics(ics);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "Standup, team");
        assert_eq!(events[0].start.to_rfc3339(), "2026-11-02T08:30:00+00:00");
        assert_eq!(events[0].end.to_rfc3339(), "2026-11-02T08:45:00+00:00");
        assert!(events[1].all_day);
        assert_eq!(events[1].end - events[1].start, chrono::Duration::days(1));
    }

    #[test]
    fn an_endless_event_is_left_out() {
        let event = |duration: &str| {
            format!(
                "BEGIN:VEVENT\r\nSUMMARY:Forever\r\nDTSTART:20261102T090000Z\r\n\
                DURATION:{duration}\r\nEND:VEVENT\r\n"
            )
        };
        for duration in [
            "P999999999D",
            "P9223372036854775807W",
            "PT99999999999999999999S",
            "-P999999999D",
        ] {
            assert!(parse_ics(&event(duration)).is_empty(), "{duration}");
        }
        assert_eq!(
            parse_duration("P1W2DT3H4M5S"),
            Some(chrono::Duration::seconds(9 * 86400 + 3 * 3600 + 4 * 60 + 5))
        );
        assert_eq!(parse_duration("PTH"), None);
        let events = parse_ics(&event("PT90M"));
        assert_eq!(
            events[0].end - events[0].start,
            chrono::Duration::minutes(90)
        );
    }

    #[test]
    fn a_folded_summary_is_one_line() {
        let ics = "BEGIN:VEVENT\r\nSUMMARY:Lunch with\r\n  Kristina\r\nDTSTART:20261102T110000Z\r\n\
            DTEND:20261102T120000Z\r\nEND:VEVENT\r\n";
        assert_eq!(parse_ics(ics)[0].title, "Lunch with Kristina");
    }

    #[test]
    fn a_reply_reads_without_namespaces() {
        let xml = r#"<?xml version="1.0"?><multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
            <response><href>/michael/work/</href><propstat><prop>
            <resourcetype><collection/><C:calendar/></resourcetype><displayname>Code Villain</displayname>
            </prop></propstat></response>
            <response><href>/michael/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>
            <response><href>/michael/0f8e2c4a/</href><propstat><prop>
            <resourcetype><collection/><C:calendar/></resourcetype><displayname>michael/0f8e2c4a</displayname>
            <C:supported-calendar-component-set><C:comp name="VEVENT"/><C:comp name="VTODO"/></C:supported-calendar-component-set>
            </prop></propstat></response>
            <response><href>/michael/tasks/</href><propstat><prop>
            <resourcetype><collection/><C:calendar/></resourcetype><displayname>DEFAULT_TASK_CALENDAR_NAME</displayname>
            <C:supported-calendar-component-set><C:comp name="VTODO"/></C:supported-calendar-component-set>
            </prop></propstat></response>
            <response><href>/michael/b7a1/</href><propstat><prop>
            <resourcetype><collection/><C:calendar/></resourcetype><displayname>DEFAULT_CALENDAR_NAME</displayname>
            </prop></propstat></response>
            </multistatus>"#;
        let found = found_calendars(&parse_xml(xml).unwrap());
        let names: Vec<_> = found
            .iter()
            .map(|f| (f.href.as_str(), f.name.as_str()))
            .collect();
        // A calendar Radicale names by its path, or an app by its
        // placeholder, has no name but its id; a list of tasks isn't one.
        assert_eq!(
            names,
            [
                ("/michael/0f8e2c4a/", "0f8e2c4a"),
                ("/michael/b7a1/", "b7a1"),
                ("/michael/work/", "Code Villain")
            ]
        );
    }

    #[test]
    fn a_calendar_wears_its_accounts_hue() {
        let account = |name: &str, address: &str, hue: i32| MailAccount {
            id: uuid::Uuid::now_v7(),
            workspace_id: uuid::Uuid::nil(),
            name: name.into(),
            address: address.into(),
            host: "styx.uberspace.de".into(),
            port: 993,
            username: address.into(),
            hue,
            position: 0,
            signature: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let accounts = [
            account("Lazarski", "michael@lazarski.test", 2),
            account("Code Villain", "contact@codevillain.test", 5),
        ];
        let hue = |name: &str, host: &str, chosen: &BTreeMap<String, Uuid>| {
            let found = Found {
                href: format!("/michael/{name}/"),
                name: name.into(),
            };
            account_for(&found, host, &accounts, chosen).map(|a| a.hue)
        };
        let none = BTreeMap::new();
        assert_eq!(
            hue("Code Villain", "calendar.lazarski.test", &none),
            Some(5)
        );
        assert_eq!(hue("Family", "calendar.lazarski.test", &none), Some(2));
        assert_eq!(hue("Family", "127.0.0.1", &none), Some(2));

        // Named by an id, a calendar wears the colour the person chose.
        let id = "0f8e2c4a-7d1b-4f3e-9a6c-5b2d8e1f4a7c";
        assert_eq!(hue(id, "calendar.lazarski.test", &none), Some(2));
        let chosen = BTreeMap::from([(format!("/michael/{id}/"), accounts[1].id)]);
        assert_eq!(hue(id, "calendar.lazarski.test", &chosen), Some(5));
        // A removed account's choice is forgotten.
        let gone = BTreeMap::from([(format!("/michael/{id}/"), Uuid::now_v7())]);
        assert_eq!(hue(id, "calendar.lazarski.test", &gone), Some(2));
        assert_eq!(
            account_for(
                &Found {
                    href: "/".into(),
                    name: "x".into()
                },
                "x",
                &[],
                &none
            ),
            None
        );
    }
}
