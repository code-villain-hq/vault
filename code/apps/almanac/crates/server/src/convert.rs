//! Today's pages moved into the one set of blocks (step 3 of
//! `proposals/one-set-of-blocks.md`), once, when the server starts: every
//! block of a kind from before becomes blocks of the set, by one rule per
//! kind, the same on every topic. Chips and lists become bullets and the
//! day's steps a checklist, a block per theme under a heading of its title;
//! sections and questions toggles; cards and tables a collection, a row per
//! card with its facts as fields, and the rest in a toggle under it; a
//! timeline, bars, a trend and parts collections. The week goes: Calendar
//! has it. Points keep their ids, sources and themes wherever they can.

use std::collections::{BTreeMap, HashSet};

use almanac_domain::{
    Actor, Cell, Facet, Field, MOST_FIELDS, ModelKind, Operation, SET_KINDS, Transaction,
};
use almanac_sync::Origin;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::kept::fields as record;

/// The topics with a block of a kind from before, each with a member of
/// its workspace to write as: an admin when there is one.
const OLD: &str = "SELECT DISTINCT b.topic_id, b.workspace_id, m.user_id FROM blocks b \
     CROSS JOIN LATERAL (SELECT user_id FROM workspace_members \
       WHERE workspace_id = b.workspace_id ORDER BY role = 'admin' DESC, created_at LIMIT 1) m \
     WHERE NOT (b.kind = ANY($1))";
const BLOCKS: &str = "SELECT id, kind, title, facets, position FROM blocks \
     WHERE topic_id = $1 ORDER BY position, created_at, id";
const POINTS: &str = "SELECT id, block_id, theme, text, happened, facet, amount, note, done \
     FROM points WHERE topic_id = $1 ORDER BY position, created_at, id";

/// The kinds that stay: the set's, and what they told about themselves.
fn stays(kind: &str) -> bool {
    SET_KINDS.contains(&kind) || kind == "you"
}

#[derive(Debug, Clone)]
pub struct OldBlock {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub facets: Vec<Facet>,
    pub position: i32,
}

#[derive(Debug, Clone, Default)]
pub struct OldPoint {
    pub id: Uuid,
    pub block_id: Uuid,
    pub theme: String,
    pub text: String,
    pub happened: String,
    pub facet: String,
    pub amount: Option<f64>,
    pub note: String,
    pub done: bool,
}

/// Moves every topic with a block from before into the set, a topic at a
/// time, through the sync engine so every client gets it.
pub async fn everywhere(pool: &PgPool) {
    let kinds: Vec<String> = SET_KINDS
        .iter()
        .map(|k| k.to_string())
        .chain(["you".to_string()])
        .collect();
    let topics: Vec<(Uuid, Uuid, Uuid)> =
        match sqlx::query_as(OLD).bind(&kinds).fetch_all(pool).await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "couldn't look for pages from before the set");
                return;
            }
        };
    for (topic_id, workspace_id, user_id) in topics {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        if let Err(error) = one(pool, actor, topic_id).await {
            tracing::warn!(%error, topic = %topic_id, "couldn't move a page into the set");
        }
    }
}

async fn one(pool: &PgPool, actor: Actor, topic_id: Uuid) -> Result<(), AppError> {
    let mut db = pool.begin().await?;
    type BlockRow = (Uuid, String, String, Json<Vec<Facet>>, i32);
    let blocks: Vec<BlockRow> = sqlx::query_as(BLOCKS)
        .bind(topic_id)
        .fetch_all(&mut *db)
        .await?;
    let blocks: Vec<OldBlock> = blocks
        .into_iter()
        .map(|(id, kind, title, facets, position)| OldBlock {
            id,
            kind,
            title,
            facets: facets.0,
            position,
        })
        .collect();
    type Row = (
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        Option<f64>,
        String,
        bool,
    );
    let points: Vec<Row> = sqlx::query_as(POINTS)
        .bind(topic_id)
        .fetch_all(&mut *db)
        .await?;
    let points: Vec<OldPoint> = points
        .into_iter()
        .map(
            |(id, block_id, theme, text, happened, facet, amount, note, done)| OldPoint {
                id,
                block_id,
                theme,
                text,
                happened,
                facet,
                amount,
                note,
                done,
            },
        )
        .collect();
    let operations = page(topic_id, &blocks, &points);
    if !operations.is_empty() {
        let transaction = Transaction {
            id: Uuid::now_v7(),
            operations,
        };
        almanac_sync::apply_in(&mut db, actor, &transaction, Origin::Server).await?;
    }
    db.commit().await?;
    tracing::info!(topic = %topic_id, "moved a page into the set");
    Ok(())
}

/// The writes that move a topic's page into the set: its `blocks` in order
/// and its `points` in order.
pub fn page(topic_id: Uuid, blocks: &[OldBlock], points: &[OldPoint]) -> Vec<Operation> {
    let mut page = Page {
        topic_id,
        blocks: Vec::new(),
        points: Vec::new(),
        gone_points: Vec::new(),
        gone_blocks: Vec::new(),
    };
    for block in blocks {
        let of: Vec<&OldPoint> = points.iter().filter(|p| p.block_id == block.id).collect();
        if stays(&block.kind) {
            page.blocks.push(NewBlock::kept(block));
            continue;
        }
        match block.kind.as_str() {
            "week" if of.is_empty() => page.gone_blocks.push(block.id),
            "today" => page.split(block, &of, "checklist", |_| Vec::new(), |_| None),
            "list" if of.iter().any(|p| p.done) => {
                page.split(block, &of, "checklist", |_| Vec::new(), |_| None)
            }
            "text" if themes(&of).is_empty() => {
                page.turn(block, "paragraph", &block.title, Vec::new(), &of)
            }
            "text" | "questions" | "shelf" if !themes(&of).is_empty() => {
                page.turn(block, "toggle", &block.title, Vec::new(), &of)
            }
            "cards" | "table" => page.cards(block, &of),
            "timeline" => {
                let when = field("When", "day", "", None);
                page.turn(block, "collection", &block.title, vec![when], &[]);
                for p in &of {
                    page.put(
                        p,
                        block.id,
                        (!p.happened.is_empty())
                            .then(|| cells([("When", Cell::Text(p.happened.clone()))])),
                    );
                }
            }
            "bars" | "parts" => page.split(
                block,
                &of,
                "collection",
                |group| vec![field("Amount", "number", unit_of(group), None)],
                |p| p.amount.map(|a| cells([("Amount", Cell::Number(a))])),
            ),
            "trend" => page.split(
                block,
                &of,
                "collection",
                |group| {
                    let goal = group
                        .iter()
                        .find(|p| p.happened.is_empty())
                        .and_then(|p| p.amount);
                    vec![
                        field("Day", "day", "", None),
                        field("Amount", "number", unit_of(group), goal),
                    ]
                },
                |p| {
                    let mut row = BTreeMap::new();
                    if !p.happened.is_empty() {
                        row.insert("Day".to_string(), Cell::Text(p.happened.clone()));
                    }
                    if let Some(amount) = p.amount {
                        row.insert("Amount".to_string(), Cell::Number(amount));
                    }
                    Some(row)
                },
            ),
            // Chips, lists, a week that kept something, and whatever else
            // was a line each.
            _ => page.split(block, &of, "bullets", |_| Vec::new(), |_| None),
        }
    }
    page.finish()
}

struct NewBlock {
    id: Uuid,
    /// Its writes: everything for one it makes, what changes for one it
    /// keeps; `None` for one that stays as it is.
    data: Option<Map<String, Value>>,
    made: bool,
    /// Where it was, to write only a position that changes.
    was: Option<i32>,
}

impl NewBlock {
    fn kept(block: &OldBlock) -> Self {
        NewBlock {
            id: block.id,
            data: None,
            made: false,
            was: Some(block.position),
        }
    }
}

struct Page {
    topic_id: Uuid,
    blocks: Vec<NewBlock>,
    points: Vec<Operation>,
    gone_points: Vec<Uuid>,
    gone_blocks: Vec<Uuid>,
}

impl Page {
    /// `block` as a block of `kind`, keeping its id, with `points` in it.
    fn turn(
        &mut self,
        block: &OldBlock,
        kind: &str,
        title: &str,
        fields: Vec<Field>,
        points: &[&OldPoint],
    ) {
        self.blocks.push(NewBlock {
            id: block.id,
            data: Some(block_data(kind, title, fields)),
            made: false,
            was: Some(block.position),
        });
        for p in points {
            self.put(p, block.id, None);
        }
    }

    /// A new block of `kind`, after the ones before it.
    fn make(&mut self, kind: &str, title: &str, fields: Vec<Field>) -> Uuid {
        let id = Uuid::now_v7();
        let mut data = block_data(kind, title, fields);
        data.insert("topic_id".into(), json!(self.topic_id));
        self.blocks.push(NewBlock {
            id,
            data: Some(data),
            made: true,
            was: None,
        });
        id
    }

    /// A point in `block`, with its cells when it's a row.
    fn put(&mut self, point: &OldPoint, block: Uuid, row: Option<BTreeMap<String, Cell>>) {
        let mut data = Map::new();
        if block != point.block_id {
            data.insert("block_id".into(), json!(block));
        }
        if let Some(row) = row {
            data.insert("cells".into(), json!(row));
        }
        if !data.is_empty() {
            self.points.push(Operation::Update {
                model: ModelKind::Point,
                id: point.id,
                data,
            });
        }
    }

    /// A block of `kind` per theme, under a heading of its title, with its
    /// points that have no theme first; one block when it has one theme or
    /// none. `fields_of` gives each its fields and `row_of` each point's cells.
    fn split(
        &mut self,
        block: &OldBlock,
        points: &[&OldPoint],
        kind: &str,
        fields_of: impl Fn(&[&OldPoint]) -> Vec<Field>,
        row_of: impl Fn(&OldPoint) -> Option<BTreeMap<String, Cell>>,
    ) {
        let themes = themes(points);
        if themes.len() <= 1 {
            let title = if block.title.is_empty() {
                themes.first().copied().unwrap_or("")
            } else {
                &block.title
            };
            self.turn(block, kind, title, fields_of(points), &[]);
            for p in points {
                self.put(p, block.id, row_of(p));
            }
            return;
        }
        let loose: Vec<&OldPoint> = points
            .iter()
            .copied()
            .filter(|p| p.theme.is_empty())
            .collect();
        let mut groups: Vec<(&str, Vec<&OldPoint>)> = Vec::new();
        if !loose.is_empty() {
            groups.push(("", loose));
        }
        for theme in themes {
            groups.push((
                theme,
                points
                    .iter()
                    .copied()
                    .filter(|p| p.theme == theme)
                    .collect(),
            ));
        }
        // The block's own id goes to its heading, or to its first group
        // when it has no title.
        let mut own = Some(block);
        if !block.title.is_empty() {
            self.turn(block, "heading", &block.title, Vec::new(), &[]);
            own = None;
        }
        for (theme, group) in groups {
            let fields = fields_of(&group);
            let id = match own.take() {
                Some(block) => {
                    self.turn(block, kind, theme, fields, &[]);
                    block.id
                }
                None => self.make(kind, theme, fields),
            };
            for p in group {
                self.put(p, id, row_of(p));
            }
        }
    }

    /// Cards or a table: a collection with a row per card, its facts as
    /// fields, and the rest in a toggle under it. Without facts, or with one
    /// card, the points go in a toggle; without cards, in bullets.
    fn cards(&mut self, block: &OldBlock, points: &[&OldPoint]) {
        let themes = themes(points);
        let mut names: Vec<String> = Vec::new();
        let facets = block.facets.iter().map(|f| f.name.as_str());
        for name in facets.chain(points.iter().map(|p| p.facet.as_str())) {
            let name = cut(name.trim(), 40);
            if !name.is_empty()
                && names.len() < MOST_FIELDS
                && !names
                    .iter()
                    .any(|n| n.to_lowercase() == name.to_lowercase())
                && points.iter().any(|p| same(&p.facet, &name))
            {
                names.push(name);
            }
        }
        if themes.is_empty() {
            return self.split(block, points, "bullets", |_| Vec::new(), |_| None);
        }
        if names.is_empty() || themes.len() < 2 {
            return self.turn(block, "toggle", &block.title, Vec::new(), points);
        }
        let fields = names.iter().map(|n| field(n, "text", "", None)).collect();
        self.turn(block, "collection", &block.title, fields, &[]);
        let mut used = HashSet::new();
        for (n, theme) in themes.iter().enumerate() {
            let mut row = BTreeMap::new();
            for name in &names {
                if let Some(p) = points
                    .iter()
                    .find(|p| p.theme == *theme && same(&p.facet, name) && !used.contains(&p.id))
                {
                    row.insert(name.clone(), Cell::Text(cut(&p.text, 500)));
                    used.insert(p.id);
                }
            }
            self.points.push(Operation::Create {
                model: ModelKind::Point,
                id: Uuid::now_v7(),
                data: record([
                    ("topic_id", json!(self.topic_id)),
                    ("block_id", json!(block.id)),
                    ("text", json!(theme)),
                    ("cells", json!(row)),
                    ("seen", json!(true)),
                    ("position", json!(n)),
                ]),
            });
        }
        let rest: Vec<&OldPoint> = points
            .iter()
            .copied()
            .filter(|p| !used.contains(&p.id))
            .collect();
        if !rest.is_empty() {
            let toggle = self.make("toggle", "", Vec::new());
            for p in rest {
                self.put(p, toggle, None);
            }
        }
        self.gone_points.extend(used);
    }

    /// The writes: blocks made and changed in the page's new order, then
    /// the points, then what goes.
    fn finish(self) -> Vec<Operation> {
        let mut operations = Vec::new();
        let changed = self.blocks.iter().any(|b| b.data.is_some()) || !self.gone_blocks.is_empty();
        if !changed {
            return operations;
        }
        for (n, block) in self.blocks.into_iter().enumerate() {
            let position = n as i32;
            let mut data = block.data.unwrap_or_default();
            if block.was != Some(position) {
                data.insert("position".into(), json!(position));
            }
            if block.made {
                operations.push(Operation::Create {
                    model: ModelKind::Block,
                    id: block.id,
                    data,
                });
            } else if !data.is_empty() {
                operations.push(Operation::Update {
                    model: ModelKind::Block,
                    id: block.id,
                    data,
                });
            }
        }
        operations.extend(self.points);
        operations.extend(self.gone_points.into_iter().map(|id| Operation::Delete {
            model: ModelKind::Point,
            id,
        }));
        operations.extend(self.gone_blocks.into_iter().map(|id| Operation::Delete {
            model: ModelKind::Block,
            id,
        }));
        operations
    }
}

fn block_data(kind: &str, title: &str, fields: Vec<Field>) -> Map<String, Value> {
    record([
        ("kind", json!(kind)),
        ("title", json!(cut(title, 60))),
        ("fields", json!(fields)),
        ("facets", json!([])),
        ("width", json!("")),
        ("view", json!("")),
    ])
}

/// The themes of `points`, in the order they first come.
fn themes<'a>(points: &[&'a OldPoint]) -> Vec<&'a str> {
    let mut themes: Vec<&str> = Vec::new();
    for p in points {
        if !p.theme.is_empty() && !themes.contains(&p.theme.as_str()) {
            themes.push(&p.theme);
        }
    }
    themes
}

fn field(name: &str, kind: &str, unit: &str, goal: Option<f64>) -> Field {
    Field {
        name: name.into(),
        kind: kind.into(),
        unit: unit.into(),
        goal,
        options: Vec::new(),
    }
}

fn cells<const N: usize>(pairs: [(&str, Cell); N]) -> BTreeMap<String, Cell> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// What a set's numbers count, from the first note short enough to be a unit.
fn unit_of<'a>(points: &[&'a OldPoint]) -> &'a str {
    points
        .iter()
        .map(|p| p.note.trim())
        .find(|n| !n.is_empty())
        .filter(|n| n.chars().count() <= 12)
        .unwrap_or("")
}

fn same(a: &str, b: &str) -> bool {
    !a.is_empty() && a.trim().to_lowercase() == b.to_lowercase()
}

fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: &str, title: &str, position: i32) -> OldBlock {
        OldBlock {
            id: Uuid::now_v7(),
            kind: kind.into(),
            title: title.into(),
            facets: Vec::new(),
            position,
        }
    }

    fn point(block: &OldBlock, theme: &str, text: &str, facet: &str) -> OldPoint {
        OldPoint {
            id: Uuid::now_v7(),
            block_id: block.id,
            theme: theme.into(),
            text: text.into(),
            facet: facet.into(),
            ..OldPoint::default()
        }
    }

    fn kinds(operations: &[Operation]) -> Vec<String> {
        operations
            .iter()
            .filter_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Block,
                    data,
                    ..
                }
                | Operation::Update {
                    model: ModelKind::Block,
                    data,
                    ..
                } => data.get("kind").map(|k| k.as_str().unwrap().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_page_of_the_set_stays_as_it_is() {
        let you = block("you", "You", 0);
        let list = block("bullets", "", 1);
        assert!(page(Uuid::nil(), &[you, list], &[]).is_empty());
    }

    #[test]
    fn cards_become_a_collection_with_the_rest_under_it() {
        let mut cats = block("cards", "Katzen", 0);
        cats.facets = vec![Facet {
            name: "Alter".into(),
            icon: "cake".into(),
        }];
        let week = block("week", "", 1);
        let points = [
            point(&cats, "Lisa", "Eine Tigerkatze", ""),
            point(&cats, "Lisa", "12 Jahre", "Alter"),
            point(&cats, "Mia", "3 Jahre", "alter"),
        ];
        let operations = page(Uuid::nil(), &[cats.clone(), week.clone()], &points);
        assert_eq!(kinds(&operations), ["collection", "toggle"]);
        let rows: Vec<&Map<String, Value>> = operations
            .iter()
            .filter_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Point,
                    data,
                    ..
                } => Some(data),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["text"], "Lisa");
        assert_eq!(rows[0]["cells"], json!({ "Alter": "12 Jahre" }));
        assert_eq!(rows[1]["cells"], json!({ "Alter": "3 Jahre" }));
        assert!(operations.iter().any(|o| matches!(o,
            Operation::Update { model: ModelKind::Point, id, .. } if *id == points[0].id)));
        assert!(operations.iter().any(|o| matches!(o,
            Operation::Delete { model: ModelKind::Point, id } if *id == points[1].id)));
        assert!(operations.iter().any(|o| matches!(o,
            Operation::Delete { model: ModelKind::Block, id } if *id == week.id)));
    }

    #[test]
    fn steps_of_two_routines_become_a_checklist_each_under_a_heading() {
        let today = block("today", "Heute", 3);
        let points = [
            point(&today, "Morgens", "Zähne", ""),
            point(&today, "Abends", "Lesen", ""),
        ];
        let operations = page(Uuid::nil(), std::slice::from_ref(&today), &points);
        assert_eq!(kinds(&operations), ["heading", "checklist", "checklist"]);
        let Operation::Update { data, .. } = &operations[0] else {
            panic!("{operations:?}")
        };
        assert_eq!(data["position"], 0);
        assert_eq!(data["title"], "Heute");
    }

    #[test]
    fn sections_become_toggles_and_a_timeline_a_collection_of_days() {
        let text = block("text", "Geschichte", 0);
        let line = block("timeline", "Zeitleiste", 1);
        let mut event = point(&line, "", "Mauerfall", "");
        event.happened = "1989".into();
        let points = [point(&text, "Intifada", "Begann 1987.", ""), event.clone()];
        let operations = page(Uuid::nil(), &[text, line], &points);
        assert_eq!(kinds(&operations), ["toggle", "collection"]);
        assert!(operations.iter().any(|o| matches!(o,
            Operation::Update { model: ModelKind::Point, id, data }
                if *id == event.id && data["cells"] == json!({ "When": "1989" }))));
    }
}
