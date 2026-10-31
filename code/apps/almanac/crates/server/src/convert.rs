//! Today's pages moved into the one set of blocks (step 3 of
//! `proposals/one-set-of-blocks.md`), once, when the server starts: every
//! block of a kind from before becomes blocks of the set, by one rule per
//! kind, the same on every topic. Chips and lists become bullets and the
//! day's steps a checklist, a block per theme under a heading of its title;
//! sections and questions toggles; cards and tables a collection, a row per
//! card with its facts as fields, and the rest in a toggle under it; a
//! timeline, bars, a trend and parts collections. The week goes: Calendar
//! has it. Points keep their ids, sources and themes wherever they can.
//!
//! A second pass makes the lists collections (C in
//! `proposals/facts-as-data.md`): bullets rows with only a name, a
//! checklist rows with a check, Done, and a toggle a row per theme with its
//! sentences as a paragraph on the row's page.
//!
//! A third pass (`terms`) turns paragraphs that were terms into collections:
//! each opened with a term in bold, and was drawn as a card. A term is a
//! row now, its name the term and what it means a text field, which View as
//! draws as a list, a table or cards. Before, a term that meant more than a
//! cell held stayed whole as its row's name, the bold and all; a fourth
//! pass (`named_terms`) gives those their name and what they mean too.

use std::collections::{BTreeMap, HashSet};

use almanac_domain::{
    Actor, Cell, Facet, Field, LIST_KINDS, MOST_FIELDS, ModelKind, Operation, SET_KINDS,
    Transaction,
};
use almanac_sync::Origin;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::AppError;
use crate::kept::{fields as record, led};

/// The topics with a block of a kind from before, each with a member of
/// its workspace to write as: an admin when there is one.
const OLD: &str = "SELECT DISTINCT b.topic_id, b.workspace_id, m.user_id FROM blocks b \
     CROSS JOIN LATERAL (SELECT user_id FROM workspace_members \
       WHERE workspace_id = b.workspace_id ORDER BY role = 'admin' DESC, created_at LIMIT 1) m \
     WHERE NOT (b.kind = ANY($1))";
const BLOCKS: &str = "SELECT id, kind, title, facets, position, row_id FROM blocks \
     WHERE topic_id = $1 ORDER BY position, created_at, id";
const POINTS: &str = "SELECT id, block_id, theme, text, happened, facet, amount, note, done \
     FROM points WHERE topic_id = $1 ORDER BY position, created_at, id";

/// The rows of a collection still named with a term in bold and what it
/// means, with nothing in their cells, each with its collection's fields
/// and a member of its workspace to write as.
const HALF_TERMS: &str = "SELECT p.id, p.text, b.fields, b.workspace_id, m.user_id FROM points p \
     JOIN blocks b ON b.id = p.block_id \
     CROSS JOIN LATERAL (SELECT user_id FROM workspace_members \
       WHERE workspace_id = b.workspace_id ORDER BY role = 'admin' DESC, created_at LIMIT 1) m \
     WHERE b.kind = 'collection' AND p.cells = '{}'::jsonb \
       AND p.text ~ '^\\s*\\*\\*[^*]+\\*\\*[:.,]?\\s+\\S' \
     ORDER BY b.workspace_id, p.id";

/// The topics with a paragraph of terms, two or more, each opening with
/// one in bold (`led`), and a member to write as.
const TERMS: &str = "SELECT DISTINCT b.topic_id, b.workspace_id, m.user_id FROM blocks b \
     CROSS JOIN LATERAL (SELECT user_id FROM workspace_members \
       WHERE workspace_id = b.workspace_id ORDER BY role = 'admin' DESC, created_at LIMIT 1) m \
     WHERE b.kind = 'paragraph' \
       AND (SELECT count(*) FROM points p WHERE p.block_id = b.id) >= 2 \
       AND NOT EXISTS (SELECT 1 FROM points p WHERE p.block_id = b.id \
         AND p.text !~ '^\\s*\\*\\*[^*]+\\*\\*[:.,]?\\s+\\S')";

/// The kinds the first pass leaves: the set's, what they told about
/// themselves, and the lists, for the second.
fn stays(kind: &str) -> bool {
    SET_KINDS.contains(&kind) || LIST_KINDS.contains(&kind) || kind == "you"
}

/// What a checklist's check is called once it's a collection.
const DONE: &str = "Done";

#[derive(Debug, Clone)]
pub struct OldBlock {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub facets: Vec<Facet>,
    pub position: i32,
    /// The row whose page it's on; `None` on the topic's.
    pub row_id: Option<Uuid>,
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
    let topics: Vec<(Uuid, Uuid, Uuid)> = match sqlx::query_as(TERMS).fetch_all(pool).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "couldn't look for paragraphs of terms");
            return;
        }
    };
    for (topic_id, workspace_id, user_id) in topics {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        let moved = async {
            let mut db = pool.begin().await?;
            let (blocks, points) = read(&mut db, topic_id).await?;
            write(&mut db, actor, terms(&blocks, &points)).await?;
            db.commit().await?;
            Ok::<_, AppError>(())
        };
        match moved.await {
            Ok(()) => tracing::info!(topic = %topic_id, "made terms rows"),
            Err(error) => tracing::warn!(%error, topic = %topic_id, "couldn't make terms rows"),
        }
    }
    named(pool).await;
}

/// The fourth pass: rows named with a whole term and what it means get the
/// term as their name and the rest in their text field, a workspace at a
/// time.
async fn named(pool: &PgPool) {
    type Row = (Uuid, String, Json<Vec<Field>>, Uuid, Uuid);
    let rows: Vec<Row> = match sqlx::query_as(HALF_TERMS).fetch_all(pool).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "couldn't look for rows named with a term");
            return;
        }
    };
    let mut by_workspace: BTreeMap<(Uuid, Uuid), Vec<NamedRow>> = BTreeMap::new();
    for (id, text, fields, workspace_id, user_id) in rows {
        by_workspace
            .entry((workspace_id, user_id))
            .or_default()
            .push((id, text, fields.0));
    }
    for ((workspace_id, user_id), rows) in by_workspace {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        let named = async {
            let mut db = pool.begin().await?;
            write(&mut db, actor, named_terms(&rows)).await?;
            db.commit().await?;
            Ok::<_, AppError>(())
        };
        match named.await {
            Ok(()) => tracing::info!(workspace = %workspace_id, "named rows by their terms"),
            Err(error) => {
                tracing::warn!(%error, workspace = %workspace_id, "couldn't name rows by their terms")
            }
        }
    }
}

async fn one(pool: &PgPool, actor: Actor, topic_id: Uuid) -> Result<(), AppError> {
    let mut db = pool.begin().await?;
    let (blocks, points) = read(&mut db, topic_id).await?;
    write(&mut db, actor, page(topic_id, &blocks, &points)).await?;
    let (blocks, points) = read(&mut db, topic_id).await?;
    write(&mut db, actor, lists(topic_id, &blocks, &points)).await?;
    db.commit().await?;
    tracing::info!(topic = %topic_id, "moved a page into the set");
    Ok(())
}

async fn read(
    db: &mut sqlx::PgConnection,
    topic_id: Uuid,
) -> Result<(Vec<OldBlock>, Vec<OldPoint>), AppError> {
    type BlockRow = (Uuid, String, String, Json<Vec<Facet>>, i32, Option<Uuid>);
    let blocks: Vec<BlockRow> = sqlx::query_as(BLOCKS)
        .bind(topic_id)
        .fetch_all(&mut *db)
        .await?;
    let blocks = blocks
        .into_iter()
        .map(|(id, kind, title, facets, position, row_id)| OldBlock {
            id,
            kind,
            title,
            facets: facets.0,
            position,
            row_id,
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
    let points = points
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
    Ok((blocks, points))
}

async fn write(
    db: &mut sqlx::PgConnection,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<(), AppError> {
    if !operations.is_empty() {
        let transaction = Transaction {
            id: Uuid::now_v7(),
            operations,
        };
        almanac_sync::apply_in(db, actor, &transaction, Origin::Server).await?;
    }
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

/// The writes that make a page's lists collections: bullets rows with only
/// a name, a checklist rows with a check, and a toggle a row per theme with
/// its sentences in a paragraph on the row's page, the sentences without a
/// theme in a paragraph after it. Blocks keep their ids, and points theirs,
/// so reminders stay with their items.
pub fn lists(topic_id: Uuid, blocks: &[OldBlock], points: &[OldPoint]) -> Vec<Operation> {
    if !blocks.iter().any(|b| LIST_KINDS.contains(&b.kind.as_str())) {
        return Vec::new();
    }
    let (mut changed, mut rows, mut on_rows, mut moves) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut pages: Vec<Option<Uuid>> = Vec::new();
    for block in blocks {
        if !pages.contains(&block.row_id) {
            pages.push(block.row_id);
        }
    }
    for at in pages {
        let mut order: Vec<NewBlock> = Vec::new();
        for block in blocks.iter().filter(|b| b.row_id == at) {
            let of: Vec<&OldPoint> = points.iter().filter(|p| p.block_id == block.id).collect();
            let turned = |kind: &str, fields: Vec<Field>| NewBlock {
                id: block.id,
                data: Some(block_data(kind, &block.title, fields)),
                made: false,
                was: Some(block.position),
            };
            match block.kind.as_str() {
                "bullets" => order.push(turned("collection", Vec::new())),
                "checklist" => {
                    order.push(turned("collection", vec![field(DONE, "check", "", None)]));
                    for p in of.iter().filter(|p| p.done) {
                        moves.push(Operation::Update {
                            model: ModelKind::Point,
                            id: p.id,
                            data: record([("cells", json!(cells([(DONE, Cell::Check(true))])))]),
                        });
                    }
                }
                "toggle" if themes(&of).is_empty() => order.push(turned("paragraph", Vec::new())),
                "toggle" => {
                    order.push(turned("collection", Vec::new()));
                    for (n, theme) in themes(&of).into_iter().enumerate() {
                        let row = Uuid::now_v7();
                        rows.push(Operation::Create {
                            model: ModelKind::Point,
                            id: row,
                            data: record([
                                ("topic_id", json!(topic_id)),
                                ("block_id", json!(block.id)),
                                ("text", json!(cut(theme, 500))),
                                ("cells", json!({})),
                                ("seen", json!(true)),
                                ("position", json!(n)),
                            ]),
                        });
                        let paragraph = Uuid::now_v7();
                        let mut data = block_data("paragraph", "", Vec::new());
                        data.insert("topic_id".into(), json!(topic_id));
                        data.insert("row_id".into(), json!(row));
                        data.insert("position".into(), json!(0));
                        on_rows.push(Operation::Create {
                            model: ModelKind::Block,
                            id: paragraph,
                            data,
                        });
                        for p in of.iter().filter(|p| p.theme == theme) {
                            moves.push(moved(p.id, paragraph));
                        }
                    }
                    let loose: Vec<&&OldPoint> = of.iter().filter(|p| p.theme.is_empty()).collect();
                    if !loose.is_empty() {
                        let id = Uuid::now_v7();
                        let mut data = block_data("paragraph", "", Vec::new());
                        data.insert("topic_id".into(), json!(topic_id));
                        data.insert("row_id".into(), json!(at));
                        order.push(NewBlock {
                            id,
                            data: Some(data),
                            made: true,
                            was: None,
                        });
                        moves.extend(loose.iter().map(|p| moved(p.id, id)));
                    }
                }
                _ => order.push(NewBlock::kept(block)),
            }
        }
        for (n, block) in order.into_iter().enumerate() {
            let position = n as i32;
            let mut data = block.data.unwrap_or_default();
            if block.was != Some(position) {
                data.insert("position".into(), json!(position));
            }
            if block.made {
                changed.push(Operation::Create {
                    model: ModelKind::Block,
                    id: block.id,
                    data,
                });
            } else if !data.is_empty() {
                changed.push(Operation::Update {
                    model: ModelKind::Block,
                    id: block.id,
                    data,
                });
            }
        }
    }
    // Blocks first, then the rows, then the pages on them, then the
    // sentences that move there.
    changed
        .into_iter()
        .chain(rows)
        .chain(on_rows)
        .chain(moves)
        .collect()
}

/// What a term means, once it's a row.
const MEANS: &str = "Text";

/// The writes that make each paragraph of terms a collection: a block of
/// two or more points that each open with a term in bold and say what it
/// means. The term is the row's name and the rest its text field, so the
/// points keep their ids and sources.
pub fn terms(blocks: &[OldBlock], points: &[OldPoint]) -> Vec<Operation> {
    let mut writes = Vec::new();
    for block in blocks.iter().filter(|b| b.kind == "paragraph") {
        let of: Vec<&OldPoint> = points.iter().filter(|p| p.block_id == block.id).collect();
        let Some(rows) = of
            .iter()
            .map(|p| led(&p.text).map(|(term, means)| (p.id, term, means)))
            .collect::<Option<Vec<_>>>()
            .filter(|rows| rows.len() >= 2)
        else {
            continue;
        };
        writes.push(Operation::Update {
            model: ModelKind::Block,
            id: block.id,
            data: record([
                ("kind", json!("collection")),
                ("fields", json!([field(MEANS, "text", "", None)])),
                ("view", json!("")),
            ]),
        });
        for (id, term, means) in rows {
            writes.push(Operation::Update {
                model: ModelKind::Point,
                id,
                data: record([
                    ("text", json!(cut(term, 500))),
                    ("cells", json!(cells([(MEANS, Cell::Text(means.into()))]))),
                ]),
            });
        }
    }
    writes
}

/// A row of a collection: its id, its name and its collection's fields.
pub type NamedRow = (Uuid, String, Vec<Field>);

/// The writes that name each of `rows` by the term in bold it opens with,
/// and put what the term means in the collection's first text field. A row
/// whose collection has no text field stays as it is.
pub fn named_terms(rows: &[NamedRow]) -> Vec<Operation> {
    rows.iter()
        .filter_map(|(id, text, fields)| {
            let (term, means) = led(text)?;
            let field = fields.iter().find(|f| f.kind == "text")?;
            Some(Operation::Update {
                model: ModelKind::Point,
                id: *id,
                data: record([
                    ("text", json!(cut(term, 500))),
                    (
                        "cells",
                        json!(cells([(field.name.as_str(), Cell::Text(means.into()))])),
                    ),
                ]),
            })
        })
        .collect()
}

/// `point` into `block`, out of its theme.
fn moved(point: Uuid, block: Uuid) -> Operation {
    Operation::Update {
        model: ModelKind::Point,
        id: point,
        data: record([("block_id", json!(block)), ("theme", json!(""))]),
    }
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
            row_id: None,
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

    #[test]
    fn paragraphs_of_terms_become_rows_with_what_they_mean() {
        let terms_block = block("paragraph", "Grundlagen", 0);
        let prose = block("paragraph", "", 1);
        let points = vec![
            point(
                &terms_block,
                "",
                "**Managing Up** heißt, die Beziehung zu gestalten.",
                "",
            ),
            point(
                &terms_block,
                "",
                "**Issue Selling**: Themen nach oben bringen.",
                "",
            ),
            point(&prose, "", "**Wichtig** ist das hier.", ""),
            point(&prose, "", "Und das ist nur ein Satz.", ""),
        ];
        let long = block("paragraph", "", 2);
        let mut points = points;
        points.push(point(&long, "", "**Kurz**: geht.", ""));
        points.push(point(
            &long,
            "",
            &format!("**Lang**: {}", "x".repeat(501)),
            "",
        ));
        let writes = terms(&[terms_block.clone(), prose, long], &points);
        // What a long term means is a row's too, never left whole as its name.
        assert_eq!(kinds(&writes), ["collection", "collection"]);
        let rows: Vec<&Map<String, Value>> = writes
            .iter()
            .filter_map(|w| match w {
                Operation::Update {
                    model: ModelKind::Point,
                    data,
                    ..
                } => Some(data),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[3]["text"], "Lang");
        assert_eq!(rows[3]["cells"]["Text"], "x".repeat(501));
        assert_eq!(rows[0]["text"], "Managing Up");
        assert_eq!(
            rows[0]["cells"]["Text"],
            "heißt, die Beziehung zu gestalten."
        );
        assert_eq!(rows[1]["text"], "Issue Selling");
        assert_eq!(rows[1]["cells"]["Text"], "Themen nach oben bringen.");
    }

    #[test]
    fn a_row_named_with_a_whole_term_gets_its_name() {
        let text = |name: &str| Field {
            name: name.into(),
            kind: "text".into(),
            unit: String::new(),
            goal: None,
            options: vec![],
        };
        let (half, plain, no_text) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let writes = named_terms(&[
            (
                half,
                "**Nakam**, eine Gruppe von Überlebenden, plante 1945 einen Anschlag.".into(),
                vec![text("Text")],
            ),
            (plain, "Antideutsche".into(), vec![text("Text")]),
            (no_text, "**Kurz**: ohne Feld.".into(), vec![]),
        ]);
        assert_eq!(writes.len(), 1);
        let Operation::Update { id, data, .. } = &writes[0] else {
            panic!("{writes:?}")
        };
        assert_eq!(*id, half);
        assert_eq!(data["text"], "Nakam");
        assert_eq!(
            data["cells"]["Text"],
            "eine Gruppe von Überlebenden, plante 1945 einen Anschlag."
        );
    }

    #[test]
    fn lists_become_collections_and_a_toggle_rows_with_pages() {
        let things = block("bullets", "Dinge", 0);
        let steps = block("checklist", "Morgens", 1);
        let history = block("toggle", "Geschichte", 2);
        let mut brushed = point(&steps, "", "Zähne", "");
        brushed.done = true;
        let points = [
            point(&things, "", "Leine", ""),
            brushed.clone(),
            point(&steps, "", "Lesen", ""),
            point(&history, "Intifada", "Begann 1987.", ""),
            point(&history, "Intifada", "Endete 1993.", ""),
            point(&history, "", "Ein Satz ohne Thema.", ""),
        ];
        let operations = lists(Uuid::nil(), &[things, steps, history.clone()], &points);
        assert_eq!(
            kinds(&operations),
            [
                "collection",
                "collection",
                "collection",
                "paragraph",
                "paragraph"
            ]
        );
        let Operation::Update { data, .. } = &operations[1] else {
            panic!("{operations:?}")
        };
        assert_eq!(data["fields"][0]["kind"], "check");
        assert!(operations.iter().any(|o| matches!(o,
            Operation::Update { model: ModelKind::Point, id, data }
                if *id == brushed.id && data["cells"] == json!({ "Done": true }))));
        let row = operations
            .iter()
            .find_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Point,
                    id,
                    data,
                } if data["text"] == "Intifada" => Some(*id),
                _ => None,
            })
            .unwrap();
        let page = operations
            .iter()
            .find_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Block,
                    id,
                    data,
                } if data["row_id"] == json!(row) => Some(*id),
                _ => None,
            })
            .unwrap();
        let moved = |text: &str| {
            let p = points.iter().find(|p| p.text == text).unwrap();
            operations.iter().find_map(|o| match o {
                Operation::Update {
                    model: ModelKind::Point,
                    id,
                    data,
                } if *id == p.id => data.get("block_id").cloned(),
                _ => None,
            })
        };
        assert_eq!(moved("Endete 1993."), Some(json!(page)));
        let loose = moved("Ein Satz ohne Thema.").unwrap();
        assert_ne!(loose, json!(page));
        assert_ne!(loose, json!(history.id));
        assert!(lists(Uuid::nil(), &[block("paragraph", "", 0)], &[]).is_empty());
    }
}
