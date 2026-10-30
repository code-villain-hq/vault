//! Almanac's tools for Claude Code, over MCP on this Mac's server
//! (`POST /api/mcp`), as Layer gives its conversations theirs. Each
//! question opens a token ([`Questions::open`]) that Claude Code sends as
//! its bearer, so a tool call lands on the question it belongs to and on
//! nothing else; the token goes when the answer is done. A tool doesn't
//! write: it collects what the answer keeps ([`Kept`]), and the talk
//! writes it with the answer, in one transaction.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use almanac_domain::{
    Actor, BLOCK_KINDS, BLOCK_WIDTHS, FACET_ICONS, ICONS, LOOKS, Proposal, ProposedPoint,
    STICKS_TO_MAX, TEMPLATE_NAMES, TopicChange, TopicPatch,
};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::kept::{Kept, reminder};
use crate::layout::{CELL_FITS, CHIP_FITS, LayoutArgs, Now, lay_out};
use crate::page::Refs;
use crate::state::AppState;
use crate::topics::{SetUp, cut};

const PROTOCOL_VERSION: &str = "2025-06-18";

const TOPIC_NAMED: &str = "SELECT id, name, sticks_to, icon, group_name, kind \
     FROM topics WHERE workspace_id = $1 AND lower(name) = lower($2)";
const TOPIC_BY_ID: &str = "SELECT id, name, sticks_to, icon, group_name, kind \
     FROM topics WHERE workspace_id = $1 AND id = $2";
const NAMES: &str = "SELECT name FROM topics WHERE workspace_id = $1 ORDER BY position";

/// What a question is for, and so which tools it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A talk in a topic: it keeps points, asks to check, offers reminders,
    /// and changes how the topic answers.
    Topic,
    /// A talk in the workspace's own topic: it proposes and changes topics.
    Own,
    /// Setting up a topic that was just started.
    SetUp,
}

impl Mode {
    /// The tools' names, for Claude Code's allowed tools.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Mode::Topic => &[
                "keep_point",
                "tick_points",
                "ask_to_check",
                "offer_reminder",
                "change_brief",
                "lay_out_page",
                "name_talk",
                "fits_elsewhere",
            ],
            Mode::Own => &[
                "propose_topic",
                "change_topic",
                "name_talk",
                "fits_elsewhere",
            ],
            Mode::SetUp => &["set_up_topic"],
        }
    }
}

/// One question Claude Code is answering, and what its tools collected.
pub struct Asking {
    pub actor: Actor,
    pub topic_id: Uuid,
    pub mode: Mode,
    pub kept: Mutex<Kept>,
    pub set_up: Mutex<Option<SetUp>>,
}

impl Asking {
    pub fn new(actor: Actor, topic_id: Uuid, mode: Mode) -> Self {
        Asking {
            actor,
            topic_id,
            mode,
            kept: Mutex::new(Kept::default()),
            set_up: Mutex::new(None),
        }
    }

    /// Keeping for a page with these points and reminders.
    pub fn on(self, refs: Refs) -> Self {
        Asking {
            kept: Mutex::new(Kept::on(refs)),
            ..self
        }
    }

    /// What the tools collected, taken out.
    pub fn take_kept(&self) -> Kept {
        self.kept
            .lock()
            .map(|mut kept| std::mem::take(&mut *kept))
            .unwrap_or_default()
    }

    pub fn take_set_up(&self) -> Option<SetUp> {
        self.set_up.lock().ok().and_then(|mut s| s.take())
    }
}

/// The questions being answered, by their tokens.
#[derive(Clone, Default)]
pub struct Questions(Arc<Mutex<HashMap<String, Arc<Asking>>>>);

impl Questions {
    /// Opens a token for `asking`, which lasts as long as the returned
    /// [`Open`].
    pub fn open(&self, asking: Asking) -> Open {
        let token = crate::auth::new_token();
        let asking = Arc::new(asking);
        if let Ok(mut open) = self.0.lock() {
            open.insert(token.clone(), asking.clone());
        }
        Open {
            token,
            asking,
            questions: self.clone(),
        }
    }

    fn get(&self, token: &str) -> Option<Arc<Asking>> {
        self.0.lock().ok()?.get(token).cloned()
    }
}

/// A question's token, closed when this is dropped.
pub struct Open {
    pub token: String,
    pub asking: Arc<Asking>,
    questions: Questions,
}

impl Drop for Open {
    fn drop(&mut self) {
        if let Ok(mut open) = self.questions.0.lock() {
            open.remove(&self.token);
        }
    }
}

/// `POST /api/mcp`: one JSON-RPC message from Claude Code.
pub async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let asking = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| state.questions.get(token));
    let Some(asking) = asking else {
        return (StatusCode::UNAUTHORIZED, "This question is over.").into_response();
    };
    let id = message["id"].clone();
    if id.is_null() {
        // A notification, such as `notifications/initialized`.
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "almanac", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools(asking.mode) }),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or("");
            let arguments = message["params"]["arguments"].clone();
            let (text, failed) = match call(&state, &asking, name, arguments).await {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            tracing::info!(tool = name, failed, said = %text, "Claude Code called a tool");
            json!({ "content": [{ "type": "text", "text": text }], "isError": failed })
        }
        _ => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Almanac doesn't offer that." },
            }))
            .into_response();
        }
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

/// Runs one tool. What's wrong comes back as its result, in words, so
/// Claude can fix it and call again.
async fn call(
    state: &AppState,
    asking: &Asking,
    name: &str,
    arguments: Value,
) -> Result<String, String> {
    if !asking.mode.tools().contains(&name) {
        return Err(format!("There's no {name} here."));
    }
    fn read<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
        serde_json::from_value(arguments).map_err(|e| format!("The arguments don't fit: {e}."))
    }
    let kept = || {
        asking
            .kept
            .lock()
            .map_err(|_| "Almanac stumbled.".to_string())
    };
    match name {
        "keep_point" => {
            kept()?.keep_point(read(arguments)?)?;
            Ok("Kept.".into())
        }
        "tick_points" => {
            let ticks: TickArgs = read(arguments)?;
            kept()?.tick_points(&ticks.points, ticks.done)?;
            Ok(if ticks.done {
                "Ticked off."
            } else {
                "Ticked on again."
            }
            .into())
        }
        "ask_to_check" => {
            let question: QuestionArgs = read(arguments)?;
            kept()?.ask_to_check(&question.theme, &question.text, &question.point)?;
            Ok("They'll see it under the answer.".into())
        }
        "offer_reminder" => {
            let r: ReminderArgs = read(arguments)?;
            let reminder = reminder(&r.theme, &r.title, &r.due_on, &r.at, r.every_days)?;
            kept()?.offer_reminder(reminder, &r.replaces)?;
            Ok("Offered: they add it to the calendar with a click.".into())
        }
        "change_brief" => {
            let args: BriefArgs = read(arguments)?;
            let topic: Option<TopicRow> = sqlx::query_as(TOPIC_BY_ID)
                .bind(asking.actor.workspace_id)
                .bind(asking.topic_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't look at the topic.".to_string())?;
            let (id, name, now, ..) = topic.ok_or("This topic is gone.")?;
            kept()?.change_brief(id, &name, &now, &args.sticks_to, &args.said)?;
            Ok(
                "Changed, once your answer is done. They see what you said, and can undo it."
                    .into(),
            )
        }
        "lay_out_page" => {
            let args: LayoutArgs = read(arguments)?;
            let topic: Option<TopicRow> = sqlx::query_as(TOPIC_BY_ID)
                .bind(asking.actor.workspace_id)
                .bind(asking.topic_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't look at the topic.".to_string())?;
            let (id, name, ..) = topic.ok_or("This topic is gone.")?;
            let now = Now::load(&state.pool, asking.actor.workspace_id, id)
                .await
                .map_err(|_| "Almanac couldn't look at the page.".to_string())?;
            let mut guard = kept()?;
            if !guard.points.is_empty() {
                return Err(
                    "You kept points in this answer already: lay out the page in the next one."
                        .into(),
                );
            }
            let laid = lay_out(id, &name, &guard.refs, &now, args)?;
            let again = guard.layout.replace(laid).is_some();
            Ok(if again {
                "Laid out, in place of the layout before, once your answer is done."
            } else {
                "Laid out, once your answer is done. They see what moved under your answer, \
                 and can undo it."
            }
            .into())
        }
        "name_talk" => {
            let args: NameArgs = read(arguments)?;
            kept()?.talk_title = Some(cut(&args.title, 80));
            Ok("Named.".into())
        }
        "fits_elsewhere" => {
            let args: ElsewhereArgs = read(arguments)?;
            kept()?.elsewhere = Some(cut(&args.topic, 80));
            Ok("They'll see where it fits.".into())
        }
        "propose_topic" => {
            let proposal = propose(state, asking, read(arguments)?).await?;
            let replaced = kept()?.proposal.replace(proposal).is_some();
            Ok(if replaced {
                "Proposed, in place of the one before."
            } else {
                "Proposed. They see it as a card under your answer, with Start and Change. \
                 It isn't started until they click Start."
            }
            .into())
        }
        "change_topic" => {
            let change = change(state, asking, read(arguments)?).await?;
            let mut guard = kept()?;
            match guard
                .changes
                .iter_mut()
                .find(|c| c.topic_id == change.topic_id)
            {
                Some(earlier) => merge(earlier, change),
                None => guard.changes.push(change),
            }
            Ok(
                "Changed, once your answer is done. They see what you said, and can undo it."
                    .into(),
            )
        }
        "set_up_topic" => {
            let set_up: SetUp = read(arguments)?;
            if !ICONS.contains(&set_up.icon.as_str())
                || !TEMPLATE_NAMES.contains(&set_up.template.as_str())
            {
                return Err("Pick the icon and the template from their lists.".into());
            }
            if let Ok(mut slot) = asking.set_up.lock() {
                *slot = Some(set_up);
            }
            Ok("Set up.".into())
        }
        _ => Err(format!("There's no {name} here.")),
    }
}

#[derive(Deserialize)]
struct TickArgs {
    points: Vec<String>,
    #[serde(default = "yes")]
    done: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct QuestionArgs {
    #[serde(default)]
    theme: String,
    text: String,
    #[serde(default)]
    point: String,
}

#[derive(Deserialize)]
struct ReminderArgs {
    #[serde(default)]
    theme: String,
    title: String,
    due_on: String,
    #[serde(default)]
    at: String,
    every_days: Option<i64>,
    #[serde(default)]
    replaces: Vec<String>,
}

#[derive(Deserialize)]
struct BriefArgs {
    sticks_to: String,
    #[serde(default)]
    said: String,
}

#[derive(Deserialize)]
struct NameArgs {
    title: String,
}

#[derive(Deserialize)]
struct ElsewhereArgs {
    topic: String,
}

#[derive(Deserialize)]
struct ProposeArgs {
    name: String,
    sticks_to: String,
    icon: String,
    group: String,
    template: String,
    #[serde(default)]
    cards: String,
    #[serde(default)]
    starters: Vec<String>,
    #[serde(default)]
    points: Vec<ProposedPointArgs>,
    #[serde(default)]
    reminders: Vec<ReminderArgs>,
}

#[derive(Deserialize)]
struct ProposedPointArgs {
    #[serde(default)]
    block: String,
    #[serde(default)]
    theme: String,
    text: String,
}

#[derive(Deserialize)]
struct ChangeArgs {
    topic: String,
    name: Option<String>,
    sticks_to: Option<String>,
    icon: Option<String>,
    group: Option<String>,
    said: String,
}

type TopicRow = (Uuid, String, String, String, String, String);

async fn names(state: &AppState, asking: &Asking) -> String {
    let names: Vec<String> = sqlx::query_scalar(NAMES)
        .bind(asking.actor.workspace_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    names.join(", ")
}

async fn propose(state: &AppState, asking: &Asking, args: ProposeArgs) -> Result<Proposal, String> {
    let name = cut(&args.name, 80);
    if name.is_empty() {
        return Err("A topic needs a name.".into());
    }
    let existing: Option<TopicRow> = sqlx::query_as(TOPIC_NAMED)
        .bind(asking.actor.workspace_id)
        .bind(&name)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
    if let Some(existing) = existing {
        return Err(format!(
            "There's a topic called {} already. Say so, or change it with change_topic.",
            existing.1
        ));
    }
    if !ICONS.contains(&args.icon.as_str()) {
        return Err("Pick the icon from its list.".into());
    }
    if !TEMPLATE_NAMES.contains(&args.template.as_str()) {
        return Err(format!("The template is one of {TEMPLATE_NAMES:?}."));
    }
    let points: Vec<ProposedPoint> = args
        .points
        .iter()
        .map(|p| ProposedPoint {
            block: Some(cut(&p.block, 60)).filter(|b| !b.is_empty()),
            theme: cut(&p.theme, 120),
            text: cut(&p.text, 2000),
        })
        .filter(|p| !p.text.is_empty())
        .take(8)
        .collect();
    let reminders = args
        .reminders
        .iter()
        .take(3)
        .map(|r| reminder(&r.theme, &r.title, &r.due_on, &r.at, r.every_days))
        .collect::<Result<Vec<_>, _>>()?;
    let cards = cut(&args.cards, 60);
    Ok(Proposal {
        name,
        sticks_to: cut(&args.sticks_to, STICKS_TO_MAX),
        icon: args.icon,
        group_name: cut(&args.group, 40),
        template: args.template,
        cards: (!cards.is_empty()).then_some(cards),
        starters: args
            .starters
            .iter()
            .map(|s| cut(s, 200))
            .filter(|s| !s.is_empty())
            .take(4)
            .collect(),
        points,
        reminders,
        topic_id: None,
    })
}

async fn change(
    state: &AppState,
    asking: &Asking,
    args: ChangeArgs,
) -> Result<TopicChange, String> {
    let named = cut(&args.topic, 80);
    let earlier = |find: &dyn Fn(&TopicChange) -> bool| {
        asking
            .kept
            .lock()
            .ok()
            .and_then(|kept| kept.changes.iter().find(|c| find(c)).cloned())
    };
    // A topic this answer renamed already goes by its new name.
    let renamed = earlier(&|c| {
        c.after
            .name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(&named))
    });
    let row: Option<TopicRow> = match &renamed {
        Some(change) => sqlx::query_as(TOPIC_BY_ID)
            .bind(asking.actor.workspace_id)
            .bind(change.topic_id),
        None => sqlx::query_as(TOPIC_NAMED)
            .bind(asking.actor.workspace_id)
            .bind(&named),
    }
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
    let Some((id, mut name, mut sticks_to, mut icon, mut group_name, kind)) = row else {
        return Err(format!(
            "No topic is called {named}. The topics: {}.",
            names(state, asking).await
        ));
    };
    // What this answer changed in it already counts as how it is now.
    if let Some(change) = earlier(&|c| c.topic_id == id) {
        let now = change.after;
        for (field, value) in [
            (&mut name, now.name),
            (&mut sticks_to, now.sticks_to),
            (&mut icon, now.icon),
            (&mut group_name, now.group_name),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
    }
    let own = kind == "almanac";
    if own && (args.icon.is_some() || args.group.is_some()) {
        return Err("This topic can only get another name or say what it sticks to.".into());
    }
    let mut before = TopicPatch::default();
    let mut after = TopicPatch::default();
    let set = |given: Option<String>,
               max: usize,
               now: String,
               b: &mut Option<String>,
               a: &mut Option<String>| {
        if let Some(given) = given.map(|g| cut(&g, max))
            && given != now
        {
            *b = Some(now);
            *a = Some(given);
        }
    };
    set(
        args.name,
        80,
        name.clone(),
        &mut before.name,
        &mut after.name,
    );
    set(
        args.sticks_to,
        STICKS_TO_MAX,
        sticks_to,
        &mut before.sticks_to,
        &mut after.sticks_to,
    );
    set(args.icon, 40, icon, &mut before.icon, &mut after.icon);
    set(
        args.group,
        40,
        group_name,
        &mut before.group_name,
        &mut after.group_name,
    );
    if after == TopicPatch::default() {
        return Err(format!("{name} is like that already."));
    }
    if after.name.as_deref() == Some("") {
        return Err("A topic needs a name.".into());
    }
    if after.icon.as_deref().is_some_and(|i| !ICONS.contains(&i)) {
        return Err("Pick the icon from its list.".into());
    }
    if let Some(new_name) = &after.name
        && !new_name.eq_ignore_ascii_case(&name)
    {
        let taken: Option<TopicRow> = sqlx::query_as(TOPIC_NAMED)
            .bind(asking.actor.workspace_id)
            .bind(new_name)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
        if taken.is_some() {
            return Err(format!("There's a topic called {new_name} already."));
        }
    }
    let said = cut(&args.said, 200);
    Ok(TopicChange {
        topic_id: id,
        name: after.name.clone().unwrap_or(name),
        said: if said.is_empty() {
            "Changed it.".into()
        } else {
            said
        },
        before,
        after,
        layout: None,
        undone: false,
    })
}

/// A second change to the same topic in one answer: the fields it sets
/// over the first's, and what was there before the first.
fn merge(earlier: &mut TopicChange, later: TopicChange) {
    fn over(
        b: &mut Option<String>,
        a: &mut Option<String>,
        lb: Option<String>,
        la: Option<String>,
    ) {
        if la.is_some() {
            if b.is_none() {
                *b = lb;
            }
            *a = la;
        }
    }
    let (
        TopicChange {
            before,
            after,
            said,
            ..
        },
        e,
    ) = (later, earlier);
    if said != e.said {
        e.said = format!("{} {said}", e.said);
    }
    over(
        &mut e.before.name,
        &mut e.after.name,
        before.name,
        after.name,
    );
    over(
        &mut e.before.sticks_to,
        &mut e.after.sticks_to,
        before.sticks_to,
        after.sticks_to,
    );
    over(
        &mut e.before.icon,
        &mut e.after.icon,
        before.icon,
        after.icon,
    );
    over(
        &mut e.before.group_name,
        &mut e.after.group_name,
        before.group_name,
        after.group_name,
    );
    if let Some(name) = &e.after.name {
        e.name = name.clone();
    }
}

/// What a topic sticks to, for the tools that write it.
const BRIEF_SAYS: &str = "The topic's brief, which every talk in it follows: first what it's \
about, in a sentence or two (\"Houseplants and their care: watering, light, repotting.\"), then, \
a line each, how it answers: in which language, the tone and the length, what it brings in, what \
it checks, and what it leaves to other topics. Fit it to the topic, with nothing every topic \
does anyway: usually 400 to 1,200 characters. The page doesn't show it, only the topic's \
About.";

/// When a timeline's point happened, or a trend's was measured, for the
/// tools that keep one.
const HAPPENED_SAYS: &str = "Only on a timeline and in a trend. On a timeline when it \
happened, as a year (1959), a span (1987–1993) or a day (11. September 2001), with v. Chr. or \
BC for the years before year 1; in a trend the day it was measured, as 2026-09-24, and left \
out only for a mark they said they aim at (Ziel), never for a number they told you without its \
day. Left out elsewhere.";

/// What a point carries in cards, tables, bars, questions, trends and
/// parts, for the tools that keep or move one.
const FACET_SAYS: &str = "Only in cards and tables whose block names facets: the facet it's \
about, one of the block's, as the page names them (Wasser): a card's line, a table's column. \
Left out for what the thing is, and for the rest.";
const LOOK_SAYS: &str = "Only in cards, tables, bars and questions: warn when it needs a look \
(brown edges, over budget, a gap, no answer yet), info when it's ready for something (a cutting \
to pot). Left out otherwise.";
const AMOUNT_SAYS: &str = "In bars, trends and parts always, the number, plain (900000, 3.4, \
12): a bar's, a measurement's, a part's. In a table, a number cell's, so it sorts (709 for \
\"709 kr\"). Left out elsewhere.";
const NOTE_SAYS: &str = "Only in bars, questions, trends and parts: a few words beside it, \
under 80 characters: a bar's unit or what it counts (\"getötet\", \"im CV\"), a trend's unit \
(\"kg\"), what a part counts (\"Anzeigen\"), who asks a question (on its first point).";

/// What a page laid out anew should be, for lay_out_page: the page's
/// design, not only where its points go.
const LAYING_OUT_SAYS: &str = "Lay it out to read at a glance: change kinds, order and widths \
wherever that reads better, not only add blocks. Give each block the kind its points' shape \
asks for: things that share the same facts are cards with facets, or a table when comparing \
them matters more; numbers of one measure to compare are bars (the shoes' prices), never \
different measures side by side (hours of sleep beside a day without food); a number measured \
on two days or more is a trend; a whole and what it's made of is parts; what to do or read is \
a list; questions to practise are questions; facts of a few words are chips; only what reads \
as prose stays text. A kind whose shape the points don't have yet waits, and your answer says \
what it needs (a second weighing for a trend). Never make up a number or a day to fill a kind, \
even when they ask for every kind: a span (12–16 Stunden) isn't one number, a goal isn't a \
measurement, and a day nobody named isn't its day. Fewer, fuller blocks read better than many \
thin ones: what's about one thing stays together (a profile's facets, one table), and a block \
of two or three points joins the one it belongs with. First what they come back for (today, \
what needs a look), then what they compare, then what they read. Fill the rows: blocks side by \
side add up to the whole (1/2 and 1/2, 2/3 and 1/3), a short block sits beside another and a \
long one takes the whole; blocks side by side are about as long, and the week takes the whole. \
Only chips, trends and parts read at 1/3 or 1/4; a table takes 2/3 with two columns beside its \
names, the whole with more, and keeps every column; the rest need 1/2 or more. A block's title says what its themes \
share, never one theme again (\"Messwerte\" over Lisas Gewicht, not \"Lisas Gewicht\"). Your \
answer is short: a line or two on what changed shape, not every block, and what a kind still \
waits for, asked as a question.";

/// The templates a topic starts as, for the tools that pick one.
const TEMPLATES_SAY: &str = "The blocks its page starts with. routine when it's a routine they \
keep, with steps on certain days and the things they use (a hair or skin routine, a training \
plan): Today, the week, Your shelf and Advice. collection when it's about a set of things \
that each get a card (plants, pets, trips, recipes): Today, the week and the cards. study when \
it's something to learn or write up (a field, a pitch, research): Reading, Terms and What it \
says. Each also has You, for what it learns about them, which only its About shows. When they \
picked one, theirs.";

/// The tools a question gets, with what each is for.
fn tools(mode: Mode) -> Vec<Value> {
    let string = |description: &str| json!({ "type": "string", "description": description });
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
        })
    };
    let reminder_fields = json!({
        "theme": string("The theme it belongs to, or left out."),
        "title": string("What to do in two to four words, as the calendar shows it: \"Water the Monstera\", \"Elvital wash\". The steps go in the page's points, not here."),
        "due_on": string("The next day it's due, as YYYY-MM-DD."),
        "at": string("A time, as HH:MM, or left out for nine in the morning."),
        "every_days": { "type": "integer", "description": "The rhythm in days, or left out for once." },
    });
    let replaces = |what: &str, like: &str| {
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": format!("The page's {what} this one takes the place of, as the page names them ({like}), or left out."),
        })
    };
    let mut offer_fields = reminder_fields.clone();
    offer_fields["replaces"] = replaces("reminders", "r2");
    let name_talk = tool(
        "name_talk",
        "Names the talk, on its first answer: under six words, in the talk's language.",
        json!({ "title": string("The talk's title.") }),
        &["title"],
    );
    let fits_elsewhere = tool(
        "fits_elsewhere",
        "When the question has nothing to do with this topic: the topic it fits, one of the \
         others by name, or a short name for a new one. They see a link to it under the answer.",
        json!({ "topic": string("The topic's name.") }),
        &["topic"],
    );
    match mode {
        Mode::Topic => vec![
            tool(
                "keep_point",
                "Keeps a point on the topic's page: a fact, a decision, or something they told \
                 you about their situation that's worth knowing later; nothing the page has \
                 already. It goes in one of the page's blocks, with the theme and the length \
                 that block's kind asks for, as the page says. The page is kept up to date, not \
                 added to: when what you'd keep changes or corrects points the page has (a wash \
                 day moved, a product swapped), keep it in place of them with replaces, naming \
                 them as the page does (p3), and keep one point where two would say the same. \
                 Usually none to three per answer. Only when they ask for a block the page \
                 lacks (a reading list, the terms), or none fits, start one: block is its short \
                 title, kind its kind. Use an existing theme when one fits.",
                json!({
                    "block": string("The block it goes in: as the page names it (b2), or its title. For a new block, its title."),
                    "kind": { "type": "string", "enum": BLOCK_KINDS, "description": "Only for a block the page lacks: the kind it starts as. Left out otherwise." },
                    "theme": string("What it's under inside the block, as its kind says: what comes round in today, the thing in a shelf, cards or a table, the section in text and on a timeline, what's measured in a trend, the whole in parts. Left out in chips, lists and you."),
                    "text": string("The point: in text two to four sentences that explain; on a timeline one line on what happened; elsewhere one short sentence or a few words."),
                    "happened": string(HAPPENED_SAYS),
                    "facet": string(FACET_SAYS),
                    "look": { "type": "string", "enum": LOOKS, "description": LOOK_SAYS },
                    "amount": { "type": "number", "description": AMOUNT_SAYS },
                    "note": string(NOTE_SAYS),
                    "source": string("Where it comes from, in a few words: \"You said\", \"From the talk\", or the book, guide, institution or site the answer relied on."),
                    "url": string("The page it comes from, when you read one. Never a made-up link."),
                    "replaces": replaces("points", "p3"),
                }),
                &["block", "text", "source"],
            ),
            tool(
                "tick_points",
                "Ticks points done: a list's items they did or read, a question they answered \
                 well when they practised it (every point of the question). Only when they \
                 told you, or answered it in this talk. done false ticks them on again.",
                json!({
                    "points": { "type": "array", "items": { "type": "string" }, "description": "The points, as the page names them (p3)." },
                    "done": { "type": "boolean", "description": "Whether they're done; left out, they are." },
                }),
                &["points"],
            ),
            tool(
                "ask_to_check",
                "When your answer rested on a guess about their situation that you didn't ask \
                 about in the answer, asks them to check it on the page, with the point to keep \
                 if they say yes (text \"Does the Monstera stand by the north window?\", point \
                 \"Stands by the north window.\"). A question you ask in your answer waits for \
                 their reply in the talk: never ask it here too. At most two per answer, and \
                 none the page is already waiting on.",
                json!({
                    "theme": string("The theme the point would go under."),
                    "text": string("The question to check."),
                    "point": string("The point to keep when they say yes, or left out."),
                }),
                &["text"],
            ),
            tool(
                "offer_reminder",
                "Offers a reminder, only when the talk settled something recurring or dated \
                 they'd want to be reminded of (watering every 7 days, repotting in March, \
                 sending the deck on Friday). A routine they told you they keep counts \
                 (washing their hair three times a week): one reminder for each day it comes \
                 round, so something a few times a week is a reminder per weekday, each every \
                 7 days; when they didn't say which days, spread them evenly and say which in \
                 your answer. They add it to the calendar with a click. None the calendar has \
                 already: when a reminder changes (a wash day moved, another rhythm), offer the \
                 new one in place of the old with replaces, naming it as the page does (r2). \
                 One that only goes, with nothing new on its day, is replaced by the reminder \
                 that takes over from it.",
                offer_fields,
                &["title", "due_on"],
            ),
            tool(
                "change_brief",
                "Changes how this topic answers from now on, in what it sticks to: its brief, \
                 which the page you read starts with. Only when they tell you how it should \
                 answer from now on (\"always say when a source was written\", \"in English \
                 from now on\"), or correct the same thing a second time in this talk (\"too \
                 long again\"): never for a wish about one answer. Give the brief whole, as it \
                 is from now on: all of it kept, with the rule added or changed where it fits. \
                 Never its name, group or icon. They see what you said under the answer, and \
                 can undo it.",
                json!({
                    "sticks_to": string("The whole brief as it is from now on."),
                    "said": string("What changed, in a short sentence for under the answer, in the talk's language: \"Names when each source was written.\""),
                }),
                &["sticks_to", "said"],
            ),
            tool(
                "lay_out_page",
                &format!(
                    "Lays out the topic's page anew, only when they ask for that in this talk \
                 (\"sort the page better\", \"make a timeline of it\"): never on your own. \
                 List every block the page keeps, in its new order, with a new kind or title \
                 where it changes; one you leave out must end up empty, and goes. Move points \
                 between blocks and themes, naming them as the page does (p3), and reword one \
                 only to fit its new place, with every fact it had; points no move names stay \
                 where they are. A chip holds {CHIP_FITS} characters and a table's cell \
                 {CELL_FITS}, chips of a profile too: one that says more gets a few words there \
                 (\"Waldkatzen-Mix, ohne Papiere\"), and what else it said is a new point drawn \
                 from it where sentences read, a facet's tile or a list. new_points draws new points from ones the page has, with \
                 their source: a timeline's events from its sections, which keep theirs. \
                 Nothing is dropped and no point's source changes; a block the layout empties \
                 goes. It's done at once, and they see what moved under your answer and can \
                 undo it. Call it once, with the whole layout: a second call replaces the \
                 first. Keep no other points in the same answer. {LAYING_OUT_SAYS}"
                ),
                json!({
                    "blocks": {
                        "type": "array",
                        "description": "Every block the page keeps, in its new order.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "block": string("The page's block, as the page names it (b2); left out for a new one."),
                                "kind": { "type": "string", "enum": BLOCK_KINDS, "description": "Its kind: for a new block, or to change one's kind. Left out, it keeps its own." },
                                "title": string("A new block's short title, or a new title for one the page has. Left out, it keeps its own."),
                                "width": { "type": "string", "enum": BLOCK_WIDTHS, "description": "How wide it is, of the page's twelve columns: 1/4, 1/3, 1/2, 2/3, 3/4 or whole, so a small block sits beside another (a trend at 1/3 beside text at 2/3). \"\" for its kind's own. Left out, it keeps its own." },
                                "facets": {
                                    "type": "array",
                                    "description": "For cards and tables: the facts every thing has, which each card's face shows a line of, or a table's columns, in order. Left out, it keeps its own; [] for none.",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "name": string("A word or two, in the page's language (Licht, Wasser, Platz)."),
                                            "icon": { "type": "string", "enum": FACET_ICONS },
                                        },
                                        "required": ["name", "icon"],
                                        "additionalProperties": false,
                                    },
                                },
                            },
                            "additionalProperties": false,
                        },
                    },
                    "moves": {
                        "type": "array",
                        "description": "The points that move or change, each once.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "point": string("The point, as the page names it (p3)."),
                                "block": string("The block it goes to: as the page names it (b2), or the title of a new block in blocks. Left out, it stays in its block."),
                                "theme": string("Its theme in the block it ends in, as that block's kind asks. Left out, it keeps its own; chips, lists and you have none."),
                                "text": string("Only when it's reworded to fit its new place: the point, with every fact it had."),
                                "happened": string(HAPPENED_SAYS),
                                "facet": string(FACET_SAYS),
                                "look": { "type": "string", "enum": LOOKS, "description": LOOK_SAYS },
                                "amount": { "type": "number", "description": AMOUNT_SAYS },
                                "note": string(NOTE_SAYS),
                            },
                            "required": ["point"],
                            "additionalProperties": false,
                        },
                    },
                    "new_points": {
                        "type": "array",
                        "description": "New points drawn from ones the page has: a timeline's events from its sections.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "from": string("The page's point it's drawn from (p3): it keeps that one's source."),
                                "block": string("The block it goes in, as for moves."),
                                "theme": string("Its theme there: on a timeline, the section it belongs to, named as the page names it."),
                                "text": string("The point: on a timeline, one line on what happened."),
                                "happened": string(HAPPENED_SAYS),
                                "facet": string(FACET_SAYS),
                                "look": { "type": "string", "enum": LOOKS, "description": LOOK_SAYS },
                                "amount": { "type": "number", "description": AMOUNT_SAYS },
                                "note": string(NOTE_SAYS),
                            },
                            "required": ["from", "block", "text"],
                            "additionalProperties": false,
                        },
                    },
                    "said": string("What you did, in a short sentence for under the answer, in the talk's language: \"Sorted the cards by what she does.\""),
                }),
                &["blocks", "said"],
            ),
            name_talk,
            fits_elsewhere,
        ],
        Mode::Own => vec![
            tool(
                "propose_topic",
                "Proposes a new topic, shown under your answer as a card they start with a \
                 click or change first. Only when they said what they want to keep track of \
                 and no topic has it already. One per answer.",
                json!({
                    "name": string("A short name, one to three words, in their language."),
                    "sticks_to": string(BRIEF_SAYS),
                    "icon": { "type": "string", "enum": ICONS, "description": "The icon that fits best." },
                    "group": string("A short group for the sidebar, such as Home, Work, Health or Learning. Reuse an existing group when one fits."),
                    "template": { "type": "string", "enum": TEMPLATE_NAMES, "description": TEMPLATES_SAY },
                    "cards": string("For a collection, what its cards are, in a word or two (\"Plants\", \"Recipes\"); otherwise left out."),
                    "starters": { "type": "array", "items": { "type": "string" }, "description": "Three or four first questions they might ask, each under 12 words." },
                    "points": {
                        "type": "array",
                        "description": "Only what they told you, as its first points: never advice or what you know, which the topic's talks keep with its source.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "block": string("The template's block it goes in, by title: in any of them You (a fact about them, which the topic's About shows); in a routine Your shelf (a thing they use), Today (a step of what comes round) or Advice; in a collection the cards; in a study Reading, Terms or What it says."),
                                "theme": string("What it's under in the block: the thing on the shelf or card, what comes round today, the section. Left out for You, Advice, Reading and Terms."),
                                "text": string("The point, one short sentence."),
                            },
                            "required": ["block", "text"],
                            "additionalProperties": false,
                        },
                    },
                    "reminders": {
                        "type": "array",
                        "description": "Reminders to offer once it's started, only when they asked for one or clearly need it.",
                        "items": {
                            "type": "object",
                            "properties": reminder_fields,
                            "required": ["title", "due_on"],
                            "additionalProperties": false,
                        },
                    },
                }),
                &["name", "sticks_to", "icon", "group", "template", "starters"],
            ),
            tool(
                "change_topic",
                "Changes a topic they named: its name, what it sticks to, its icon or its \
                 group. Only the fields to change. They see what you said and \
                 can undo it. This topic itself can only be renamed or say what it sticks to.",
                json!({
                    "topic": string("The topic's name now."),
                    "name": string("Its new name."),
                    "sticks_to": string("What it sticks to now, its brief, whole: all of it kept, with what changes in its place."),
                    "icon": { "type": "string", "enum": ICONS },
                    "group": string("Its group in the sidebar."),
                    "said": string("What you changed, in a short sentence for under the answer: \"Moved it to Hobby.\""),
                }),
                &["topic", "said"],
            ),
            name_talk,
            fits_elsewhere,
        ],
        Mode::SetUp => vec![tool(
            "set_up_topic",
            "Sets up the new topic. Call it once, with every field.",
            json!({
                "sticks_to": string(&format!("{BRIEF_SAYS} When they wrote some, all of theirs, word for word, at the start.")),
                "icon": { "type": "string", "enum": ICONS },
                "group": string("A short group for the sidebar, such as Home, Work, Health or Learning. Reuse an existing group when one fits."),
                "template": { "type": "string", "enum": TEMPLATE_NAMES, "description": TEMPLATES_SAY },
                "cards": string("For a collection, what its cards are, in a word or two (\"Plants\", \"Recipes\"); otherwise \"\"."),
                "starters": { "type": "array", "items": { "type": "string" }, "description": "Three or four first questions, each under 12 words." },
            }),
            &[
                "sticks_to",
                "icon",
                "group",
                "template",
                "cards",
                "starters",
            ],
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_question_gets_its_own_tools() {
        assert!(Mode::Topic.tools().contains(&"keep_point"));
        assert!(!Mode::Topic.tools().contains(&"propose_topic"));
        assert!(Mode::Own.tools().contains(&"change_topic"));
        for mode in [Mode::Topic, Mode::Own, Mode::SetUp] {
            let tools = tools(mode);
            let described: Vec<&str> = tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect();
            assert_eq!(described, mode.tools());
        }
    }

    #[test]
    fn a_token_closes_with_its_answer() {
        let questions = Questions::default();
        let actor = Actor {
            user_id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
        };
        let open = questions.open(Asking::new(actor, Uuid::now_v7(), Mode::Topic));
        let token = open.token.clone();
        assert!(questions.get(&token).is_some());
        drop(open);
        assert!(questions.get(&token).is_none());
    }

    #[test]
    fn a_second_change_keeps_what_was_there_first() {
        let id = Uuid::now_v7();
        let mut first = TopicChange {
            topic_id: id,
            name: "Plants".into(),
            said: "Moved it to Hobby.".into(),
            before: TopicPatch {
                group_name: Some("Home".into()),
                ..TopicPatch::default()
            },
            after: TopicPatch {
                group_name: Some("Hobby".into()),
                ..TopicPatch::default()
            },
            layout: None,
            undone: false,
        };
        let second = TopicChange {
            topic_id: id,
            name: "Garden".into(),
            said: "Renamed it.".into(),
            before: TopicPatch {
                name: Some("Plants".into()),
                group_name: Some("Hobby".into()),
                ..TopicPatch::default()
            },
            after: TopicPatch {
                name: Some("Garden".into()),
                group_name: Some("Outside".into()),
                ..TopicPatch::default()
            },
            layout: None,
            undone: false,
        };
        merge(&mut first, second);
        assert_eq!(first.before.group_name.as_deref(), Some("Home"));
        assert_eq!(first.after.group_name.as_deref(), Some("Outside"));
        assert_eq!(first.before.name.as_deref(), Some("Plants"));
        assert_eq!(first.name, "Garden");
    }
}
