//! A topic's page laid out anew, when they ask its talk for that: blocks
//! added, renamed, reordered, removed or of another kind, points moved
//! between blocks and themes, some reworded to fit their new place, and
//! new points drawn from ones the page has (a timeline's events from its
//! sections). Nothing is dropped: a point no move names stays where it is,
//! a point's source never changes, and a block the layout empties goes.
//! It's one change in the answer's Changes, and one Undo puts every block
//! and point back ([`undo`]).

use almanac_domain::{
    BLOCK_KINDS, BlockWas, LayoutMove, ModelKind, Operation, PageLayout, PointWas, TopicChange,
    TopicPatch,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::kept::{fields, found};
use crate::page::Refs;
use crate::topics::cut;

const BLOCKS: &str = "SELECT id, kind, title, position FROM blocks \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const POINTS: &str = "SELECT id, block_id, theme, text, happened, position, source, source_url \
     FROM points WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";

/// How much one layout may do.
const MOST_BLOCKS: usize = 24;
const MOST_MOVES: usize = 400;
const MOST_NEW: usize = 80;

/// The page as it is, in the order it shows.
pub struct Now {
    pub blocks: Vec<BlockNow>,
    pub points: Vec<PointNow>,
}

#[derive(sqlx::FromRow)]
pub struct BlockNow {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub position: i32,
}

#[derive(sqlx::FromRow)]
pub struct PointNow {
    pub id: Uuid,
    pub block_id: Uuid,
    pub theme: String,
    pub text: String,
    pub happened: String,
    pub position: i32,
    pub source: Option<String>,
    pub source_url: Option<String>,
}

impl Now {
    pub async fn load(
        pool: &PgPool,
        workspace_id: Uuid,
        topic_id: Uuid,
    ) -> Result<Now, sqlx::Error> {
        Ok(Now {
            blocks: sqlx::query_as(BLOCKS)
                .bind(topic_id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
            points: sqlx::query_as(POINTS)
                .bind(topic_id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
        })
    }
}

/// What Claude asks for: the page's blocks in their new order, the points
/// that move, and the points drawn from others.
#[derive(Deserialize)]
pub struct LayoutArgs {
    pub blocks: Vec<BlockArgs>,
    #[serde(default)]
    pub moves: Vec<MoveArgs>,
    #[serde(default)]
    pub new_points: Vec<NewPointArgs>,
    #[serde(default)]
    pub said: String,
}

#[derive(Deserialize)]
pub struct BlockArgs {
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Deserialize)]
pub struct MoveArgs {
    pub point: String,
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub happened: String,
}

#[derive(Deserialize)]
pub struct NewPointArgs {
    pub from: String,
    pub block: String,
    #[serde(default)]
    pub theme: String,
    pub text: String,
    #[serde(default)]
    pub happened: String,
}

/// A layout, ready to write with the answer: the change it shows under the
/// answer, and its writes. The new points get the talk when it's written.
#[derive(Debug)]
pub struct Laid {
    pub change: TopicChange,
    pub operations: Vec<Operation>,
}

/// A block in the new layout.
struct Placed {
    id: Uuid,
    kind: String,
    title: String,
    /// Where it was on the page, for one the page has.
    was: Option<usize>,
}

/// A point as the layout leaves it.
struct Moving {
    /// What the page calls it, for what's wrong with it: "p3".
    name: String,
    block: Uuid,
    theme: String,
    text: String,
    happened: String,
    /// The move that named it, in order; none for one that stays.
    moved: Option<usize>,
    reworded: bool,
    /// Where it is on the page, or for a new point the one it's drawn from.
    now: usize,
    new: Option<Uuid>,
}

/// Lays out the page of `topic_id` as `args` says, against the page as it
/// is (`now`) and the references Claude read it with (`refs`). What's
/// wrong comes back in words, for Claude to fix.
pub fn lay_out(
    topic_id: Uuid,
    name: &str,
    refs: &Refs,
    now: &Now,
    args: LayoutArgs,
) -> Result<Laid, String> {
    if args.blocks.is_empty() {
        return Err("List every block the page keeps, in its new order.".into());
    }
    if args.blocks.len() > MOST_BLOCKS {
        return Err(format!("A page keeps at most {MOST_BLOCKS} blocks."));
    }
    if args.moves.len() > MOST_MOVES {
        return Err(format!("A layout moves at most {MOST_MOVES} points."));
    }
    if args.new_points.len() > MOST_NEW {
        return Err(format!("A layout draws at most {MOST_NEW} new points."));
    }

    let mut placed: Vec<Placed> = Vec::new();
    for b in &args.blocks {
        let named = b.block.trim();
        let kind = b.kind.trim();
        if !kind.is_empty() && !BLOCK_KINDS.contains(&kind) {
            return Err(format!(
                "There's no kind of block {kind}: it's one of {BLOCK_KINDS:?}."
            ));
        }
        let title = cut(&b.title, 60);
        if named.is_empty() {
            if kind.is_empty() {
                return Err(format!(
                    "The new block {title} needs a kind: one of {BLOCK_KINDS:?}."
                ));
            }
            if title.is_empty() && kind != "week" {
                return Err("A new block needs a short title.".into());
            }
            placed.push(Placed {
                id: Uuid::now_v7(),
                kind: kind.into(),
                title,
                was: None,
            });
            continue;
        }
        let id = found(named, 'b', refs.blocks.len())
            .map(|n| refs.blocks[n].id)
            .or_else(|| {
                now.blocks
                    .iter()
                    .find(|n| n.title.to_lowercase() == named.to_lowercase())
                    .map(|n| n.id)
            })
            .ok_or_else(|| {
                format!(
                    "The page has no block {named}: name one like b2, or leave block out for a new one."
                )
            })?;
        let was = now
            .blocks
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| format!("{named} is gone from the page."))?;
        if placed.iter().any(|p| p.id == id) {
            return Err(format!("{named} is in blocks twice."));
        }
        let block = &now.blocks[was];
        // What they told you about themselves stays off the page, on About.
        if !kind.is_empty() && (kind == "you") != (block.kind == "you") {
            return Err(if block.kind == "you" {
                format!(
                    "{named} is what they told you about themselves, on the topic's About: it stays a you block."
                )
            } else {
                format!("{named} is on the page: move what's about them to the you block instead.")
            });
        }
        placed.push(Placed {
            id,
            kind: if kind.is_empty() {
                block.kind.clone()
            } else {
                kind.into()
            },
            title: if title.is_empty() {
                block.title.clone()
            } else {
                title
            },
            was: Some(was),
        });
    }
    // What they told you about themselves isn't on the page: a layout that
    // leaves it out keeps it as it is, last.
    for (was, block) in now.blocks.iter().enumerate() {
        if block.kind == "you" && !placed.iter().any(|p| p.id == block.id) {
            placed.push(Placed {
                id: block.id,
                kind: block.kind.clone(),
                title: block.title.clone(),
                was: Some(was),
            });
        }
    }

    let target = |named: &str| -> Result<Uuid, String> {
        let named = named.trim();
        if let Some(n) = found(named, 'b', refs.blocks.len()) {
            let id = refs.blocks[n].id;
            return placed
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.id)
                .ok_or_else(|| {
                    format!("{named} isn't in blocks: list every block the page keeps.")
                });
        }
        let mut called = placed
            .iter()
            .filter(|p| p.title.to_lowercase() == named.to_lowercase());
        match (called.next(), called.next()) {
            (Some(p), None) => Ok(p.id),
            (Some(_), Some(_)) => Err(format!(
                "Two blocks are called {named}: name the page's own like b2."
            )),
            _ => Err(format!(
                "No block in blocks is called {named}: name it as blocks does, or like b2."
            )),
        }
    };

    let point_name = |id: Uuid| {
        refs.points
            .iter()
            .position(|(p, _)| *p == id)
            .map_or_else(|| "a point".to_string(), |n| format!("p{}", n + 1))
    };
    let mut points: Vec<Moving> = now
        .points
        .iter()
        .enumerate()
        .map(|(n, p)| Moving {
            name: point_name(p.id),
            block: p.block_id,
            theme: p.theme.clone(),
            text: p.text.clone(),
            happened: p.happened.clone(),
            moved: None,
            reworded: false,
            now: n,
            new: None,
        })
        .collect();
    let named_point = |reference: &str| -> Result<usize, String> {
        let id = found(reference, 'p', refs.points.len())
            .map(|n| refs.points[n].0)
            .ok_or_else(|| format!("The page has no point {reference}: name one like p3."))?;
        now.points
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("{reference} is gone from the page."))
    };
    for (order, m) in args.moves.iter().enumerate() {
        let n = named_point(&m.point)?;
        let point = &mut points[n];
        if point.moved.is_some() {
            return Err(format!("{} is in moves twice.", m.point.trim()));
        }
        point.moved = Some(order);
        if !m.block.trim().is_empty() {
            point.block = target(&m.block)?;
        }
        let theme = cut(&m.theme, 120);
        if !theme.is_empty() {
            point.theme = theme;
        }
        let text = cut(&m.text, 2000);
        if !text.is_empty() && text != point.text {
            point.text = text;
            point.reworded = true;
        }
        let happened = cut(&m.happened, 60);
        if !happened.is_empty() {
            point.happened = happened;
        }
    }
    for (order, p) in args.new_points.iter().enumerate() {
        let from = named_point(&p.from)?;
        let text = cut(&p.text, 2000);
        if text.is_empty() {
            return Err("A new point needs a text.".into());
        }
        points.push(Moving {
            name: format!("the new point from {}", p.from.trim()),
            block: target(&p.block)?,
            theme: cut(&p.theme, 120),
            text,
            happened: cut(&p.happened, 60),
            moved: Some(args.moves.len() + order),
            reworded: false,
            now: from,
            new: Some(Uuid::now_v7()),
        });
    }

    // Each point fits the kind of block it ends in.
    for point in &mut points {
        let Some(block) = placed.iter().find(|p| p.id == point.block) else {
            let was = now.blocks.iter().find(|b| b.id == point.block);
            return Err(format!(
                "{} isn't in blocks but still keeps {}: move it, or keep the block.",
                was.map_or("Its block", |b| shown(&b.title)),
                point.name
            ));
        };
        match block.kind.as_str() {
            "week" => {
                return Err(format!(
                    "{} keeps no points: move {} to another block.",
                    shown(&block.title),
                    point.name
                ));
            }
            "chips" | "list" | "you" => {
                point.theme.clear();
                point.happened.clear();
            }
            kind => {
                if point.theme.is_empty() {
                    return Err(format!(
                        "{} in {} needs a theme: {}.",
                        point.name,
                        shown(&block.title),
                        match kind {
                            "timeline" => "the section it belongs to",
                            "today" => "what comes round",
                            "shelf" => "the thing's name",
                            "text" => "the section's heading",
                            _ => "the thing the card is about",
                        }
                    ));
                }
                if kind == "timeline" {
                    if !point.happened.chars().any(|c| c.is_ascii_digit()) {
                        return Err(format!(
                            "{} on {} says when it happened: a year (1959), a span (1987–1993) or a day (11. September 2001).",
                            point.name,
                            shown(&block.title)
                        ));
                    }
                } else {
                    point.happened.clear();
                }
            }
        }
    }

    // A block the layout empties goes; one it starts needs a point.
    let holds = |id: Uuid| points.iter().filter(|p| p.block == id).count();
    let had = |id: Uuid| now.points.iter().filter(|p| p.block_id == id).count();
    for block in &placed {
        if block.was.is_none() && block.kind != "week" && holds(block.id) == 0 {
            return Err(format!(
                "{} gets no points: leave it out, or move points into it.",
                shown(&block.title)
            ));
        }
    }
    let kept: Vec<Placed> = placed
        .into_iter()
        .filter(|b| b.was.is_none() || holds(b.id) > 0 || had(b.id) == 0)
        .collect();
    let removed: Vec<usize> = (0..now.blocks.len())
        .filter(|n| !kept.iter().any(|b| b.was == Some(*n)))
        .collect();

    // Points in block order: those that stay, those moved in, those drawn.
    let mut order: Vec<usize> = Vec::new();
    for block in &kept {
        let mut here: Vec<usize> = (0..points.len())
            .filter(|n| points[*n].block == block.id)
            .collect();
        here.sort_by_key(|n| {
            let p = &points[*n];
            (p.moved.is_some(), p.moved.unwrap_or(p.now))
        });
        order.extend(here);
    }

    let mut operations = Vec::new();
    let mut blocks_were = Vec::new();
    let mut made_blocks = Vec::new();
    for (position, block) in kept.iter().enumerate() {
        let position = position as i32;
        match block.was {
            None => {
                made_blocks.push(block.id);
                operations.push(Operation::Create {
                    model: ModelKind::Block,
                    id: block.id,
                    data: fields([
                        ("topic_id", json!(topic_id)),
                        ("kind", json!(block.kind)),
                        ("title", json!(block.title)),
                        ("position", json!(position)),
                    ]),
                });
            }
            Some(n) => {
                let was = &now.blocks[n];
                let mut data = serde_json::Map::new();
                if was.kind != block.kind {
                    data.insert("kind".into(), json!(block.kind));
                }
                if was.title != block.title {
                    data.insert("title".into(), json!(block.title));
                }
                if was.position != position {
                    data.insert("position".into(), json!(position));
                }
                if !data.is_empty() {
                    blocks_were.push(block_was(was, false));
                    operations.push(Operation::Update {
                        model: ModelKind::Block,
                        id: was.id,
                        data,
                    });
                }
            }
        }
    }
    let mut points_were = Vec::new();
    let mut made_points = Vec::new();
    for (position, &n) in order.iter().enumerate() {
        let position = position as i32 + 1;
        let point = &points[n];
        let was = &now.points[point.now];
        if let Some(id) = point.new {
            made_points.push(id);
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id,
                data: fields([
                    ("topic_id", json!(topic_id)),
                    ("block_id", json!(point.block)),
                    ("theme", json!(point.theme)),
                    ("text", json!(point.text)),
                    ("happened", json!(point.happened)),
                    ("source", json!(was.source)),
                    ("source_url", json!(was.source_url)),
                    ("position", json!(position)),
                ]),
            });
            continue;
        }
        let mut data = serde_json::Map::new();
        if was.block_id != point.block {
            data.insert("block_id".into(), json!(point.block));
        }
        if was.theme != point.theme {
            data.insert("theme".into(), json!(point.theme));
        }
        if was.text != point.text {
            data.insert("text".into(), json!(point.text));
        }
        if was.happened != point.happened {
            data.insert("happened".into(), json!(point.happened));
        }
        if was.position != position {
            data.insert("position".into(), json!(position));
        }
        if !data.is_empty() {
            points_were.push(PointWas {
                id: was.id,
                block_id: was.block_id,
                theme: was.theme.clone(),
                text: was.text.clone(),
                happened: was.happened.clone(),
                position: was.position,
            });
            operations.push(Operation::Update {
                model: ModelKind::Point,
                id: was.id,
                data,
            });
        }
    }
    for &n in &removed {
        let was = &now.blocks[n];
        blocks_were.push(block_was(was, true));
        operations.push(Operation::Delete {
            model: ModelKind::Block,
            id: was.id,
        });
    }

    let moves = moves(&kept, &removed, &points, &order, now);
    if moves.is_empty() {
        return Err("The page is laid out like that already.".into());
    }
    let said = match cut(&args.said, 200) {
        said if said.is_empty() => "Laid out the page anew.".to_string(),
        said => said,
    };
    Ok(Laid {
        change: TopicChange {
            topic_id,
            name: name.into(),
            said,
            before: TopicPatch::default(),
            after: TopicPatch::default(),
            layout: Some(PageLayout {
                moves,
                blocks: blocks_were,
                points: points_were,
                made_blocks,
                made_points,
            }),
            undone: false,
        },
        operations,
    })
}

/// "What moved": a line for each block the layout changed, in the page's
/// new order, then the blocks it removed. `order` is the points in their
/// new order.
fn moves(
    kept: &[Placed],
    removed: &[usize],
    points: &[Moving],
    order: &[usize],
    now: &Now,
) -> Vec<LayoutMove> {
    let in_place = in_place(kept);
    let mut moves = Vec::new();
    for block in kept {
        let in_it: Vec<&Moving> = order
            .iter()
            .map(|&n| &points[n])
            .filter(|p| p.block == block.id)
            .collect();
        let Some(n) = block.was else {
            moves.push(LayoutMove {
                block: shown(&block.title).into(),
                change: format!("new {}", block.kind),
                holds: holds(&block.kind, &in_it),
            });
            continue;
        };
        let was = &now.blocks[n];
        let mut change = Vec::new();
        if was.kind != block.kind {
            change.push(format!("now {}", block.kind));
        }
        if was.title != block.title {
            change.push(format!("renamed from {}", shown(&was.title)));
        }
        if !in_place.contains(&n) {
            change.push("moved".to_string());
        }
        let came = in_it.iter().any(|p| {
            p.new.is_some()
                || p.reworded
                || now.points[p.now].block_id != block.id
                || now.points[p.now].theme != p.theme
                || now.points[p.now].happened != p.happened
        });
        let went = now
            .points
            .iter()
            .zip(points)
            .any(|(was, p)| was.block_id == block.id && p.block != block.id);
        // The points that were here and stay, in their old order and new.
        let stayed = |p: &Moving| p.new.is_none() && now.points[p.now].block_id == block.id;
        let before: Vec<usize> = points.iter().filter(|p| stayed(p)).map(|p| p.now).collect();
        let after: Vec<usize> = in_it.iter().filter(|p| stayed(p)).map(|p| p.now).collect();
        if change.is_empty() && !(came || went || before != after) {
            continue;
        }
        if change.is_empty() {
            change.push("changed".to_string());
        }
        moves.push(LayoutMove {
            block: shown(&block.title).into(),
            change: change.join(", "),
            holds: holds(&block.kind, &in_it),
        });
    }
    for &n in removed {
        let was = &now.blocks[n];
        let had = now.points.iter().any(|p| p.block_id == was.id);
        moves.push(LayoutMove {
            block: shown(&was.title).into(),
            change: "removed".into(),
            holds: if had {
                "nothing was left in it".into()
            } else {
                "it kept nothing".into()
            },
        });
    }
    moves
}

/// The page's own blocks that keep their place: the longest run of them
/// still in the order they had. The others moved.
fn in_place(kept: &[Placed]) -> Vec<usize> {
    let was: Vec<usize> = kept.iter().filter_map(|b| b.was).collect();
    let mut longest = vec![1; was.len()];
    let mut before: Vec<Option<usize>> = vec![None; was.len()];
    for i in 0..was.len() {
        for j in 0..i {
            if was[j] < was[i] && longest[j] + 1 > longest[i] {
                longest[i] = longest[j] + 1;
                before[i] = Some(j);
            }
        }
    }
    let mut at = (0..was.len()).max_by_key(|&i| longest[i]);
    let mut run = Vec::new();
    while let Some(i) = at {
        run.push(was[i]);
        at = before[i];
    }
    run
}

/// What a block holds now, in a line: its themes with how many points
/// each, or how many points, and how many were reworded to fit.
fn holds(kind: &str, points: &[&Moving]) -> String {
    let count = points.len();
    let mut said = match kind {
        "week" => "the days of its reminders".to_string(),
        "chips" | "list" | "you" => match count {
            1 => "a point".into(),
            n => format!("{n} points"),
        },
        "timeline" => match count {
            1 => "an event".into(),
            n => format!("{n} events"),
        },
        _ => {
            let mut themes: Vec<(&str, usize)> = Vec::new();
            for point in points {
                match themes.iter_mut().find(|(t, _)| *t == point.theme) {
                    Some((_, n)) => *n += 1,
                    None => themes.push((&point.theme, 1)),
                }
            }
            let shown: Vec<String> = themes
                .iter()
                .take(6)
                .map(|(t, n)| format!("{t} {n}"))
                .collect();
            match themes.len() {
                0..=6 => shown.join(", "),
                n => format!("{} and {} more", shown.join(", "), n - 6),
            }
        }
    };
    let reworded = points.iter().filter(|p| p.reworded).count();
    if reworded > 0 && reworded == count {
        said.push_str(", all reworded to fit");
    } else if reworded > 0 {
        said.push_str(&format!(", {reworded} reworded to fit"));
    }
    said
}

fn shown(title: &str) -> &str {
    match title {
        "" => "The week",
        title => title,
    }
}

fn block_was(block: &BlockNow, removed: bool) -> BlockWas {
    BlockWas {
        id: block.id,
        kind: block.kind.clone(),
        title: block.title.clone(),
        position: block.position,
        removed,
    }
}

/// The writes that put a page back as it was before `layout`: the blocks
/// it removed come back and those it changed are as they were, its points
/// go back where they were, and what it started goes. Of the topic's
/// blocks and points (`blocks`, `points` with their block), only what's
/// still there is touched; a block it started stays while it holds a
/// point kept since.
pub fn undo(
    topic_id: Uuid,
    layout: &PageLayout,
    blocks: &[Uuid],
    points: &[(Uuid, Uuid)],
) -> Vec<Operation> {
    let mut operations = Vec::new();
    let mut back: Vec<Uuid> = blocks.to_vec();
    for block in &layout.blocks {
        let data = fields([
            ("kind", json!(block.kind)),
            ("title", json!(block.title)),
            ("position", json!(block.position)),
        ]);
        if block.removed && !blocks.contains(&block.id) {
            let mut data = data;
            data.insert("topic_id".into(), json!(topic_id));
            operations.push(Operation::Create {
                model: ModelKind::Block,
                id: block.id,
                data,
            });
            back.push(block.id);
        } else if !block.removed && blocks.contains(&block.id) {
            operations.push(Operation::Update {
                model: ModelKind::Block,
                id: block.id,
                data,
            });
        }
    }
    let mut put_back = Vec::new();
    for point in &layout.points {
        if points.iter().any(|(id, _)| *id == point.id) && back.contains(&point.block_id) {
            operations.push(Operation::Update {
                model: ModelKind::Point,
                id: point.id,
                data: fields([
                    ("block_id", json!(point.block_id)),
                    ("theme", json!(point.theme)),
                    ("text", json!(point.text)),
                    ("happened", json!(point.happened)),
                    ("position", json!(point.position)),
                ]),
            });
            put_back.push(point.id);
        }
    }
    for &id in &layout.made_points {
        if points.iter().any(|(p, _)| *p == id) {
            operations.push(Operation::Delete {
                model: ModelKind::Point,
                id,
            });
        }
    }
    for &id in &layout.made_blocks {
        let empty = points.iter().all(|(p, block)| {
            *block != id || put_back.contains(p) || layout.made_points.contains(p)
        });
        if blocks.contains(&id) && empty {
            operations.push(Operation::Delete {
                model: ModelKind::Block,
                id,
            });
        }
    }
    operations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::RefBlock;
    use serde_json::Value;

    /// A block's kind, title and points (theme, text).
    type Shown<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

    /// A page of `blocks`, and the references Claude reads it with.
    fn page(blocks: &[Shown]) -> (Now, Refs) {
        let mut now = Now {
            blocks: Vec::new(),
            points: Vec::new(),
        };
        let mut refs = Refs::default();
        for (n, (kind, title, points)) in blocks.iter().enumerate() {
            let id = Uuid::now_v7();
            now.blocks.push(BlockNow {
                id,
                kind: kind.to_string(),
                title: title.to_string(),
                position: n as i32,
            });
            refs.blocks.push(RefBlock {
                id,
                kind: kind.to_string(),
                title: title.to_string(),
            });
            for (theme, text) in points.iter() {
                let point = Uuid::now_v7();
                now.points.push(PointNow {
                    id: point,
                    block_id: id,
                    theme: theme.to_string(),
                    text: text.to_string(),
                    happened: String::new(),
                    position: now.points.len() as i32 + 1,
                    source: Some(format!("Source of {text}")),
                    source_url: None,
                });
                refs.points.push((point, id));
            }
        }
        (now, refs)
    }

    fn block(block: &str, kind: &str, title: &str) -> BlockArgs {
        BlockArgs {
            block: block.into(),
            kind: kind.into(),
            title: title.into(),
        }
    }

    fn moving(point: &str, block: &str, theme: &str, text: &str) -> MoveArgs {
        MoveArgs {
            point: point.into(),
            block: block.into(),
            theme: theme.into(),
            text: text.into(),
            happened: String::new(),
        }
    }

    fn args(blocks: Vec<BlockArgs>, moves: Vec<MoveArgs>) -> LayoutArgs {
        LayoutArgs {
            blocks,
            moves,
            new_points: Vec::new(),
            said: String::new(),
        }
    }

    fn layout_of(laid: &Laid) -> &PageLayout {
        laid.change.layout.as_ref().unwrap()
    }

    #[test]
    fn a_timeline_draws_its_events_and_leaves_the_sections() {
        let (now, refs) = page(&[(
            "text",
            "Geschichte",
            &[
                ("Intifada", "Die erste Intifada …"),
                ("Oslo", "Die Osloer Verträge …"),
            ],
        )]);
        let mut layout = args(
            vec![block("b1", "", ""), block("", "timeline", "Zeitleiste")],
            Vec::new(),
        );
        layout.new_points = vec![
            NewPointArgs {
                from: "p1".into(),
                block: "Zeitleiste".into(),
                theme: "Intifada".into(),
                text: "Die erste Intifada beginnt".into(),
                happened: "1987–1993".into(),
            },
            NewPointArgs {
                from: "p2".into(),
                block: "Zeitleiste".into(),
                theme: "Oslo".into(),
                text: "Oslo I wird unterzeichnet".into(),
                happened: "13. September 1993".into(),
            },
        ];
        let topic = Uuid::now_v7();
        let laid = lay_out(topic, "Israel", &refs, &now, layout).unwrap();
        let made = layout_of(&laid);
        assert_eq!(made.made_blocks.len(), 1);
        assert_eq!(made.made_points.len(), 2);
        assert!(made.points.is_empty(), "the sections stay as they are");
        assert_eq!(
            made.moves,
            vec![LayoutMove {
                block: "Zeitleiste".into(),
                change: "new timeline".into(),
                holds: "2 events".into(),
            }]
        );
        // Each event keeps the source of the point it's drawn from.
        let sources: Vec<&Value> = laid
            .operations
            .iter()
            .filter_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Point,
                    data,
                    ..
                } => data.get("source"),
                _ => None,
            })
            .collect();
        assert_eq!(
            sources,
            vec![
                &json!("Source of Die erste Intifada …"),
                &json!("Source of Die Osloer Verträge …")
            ]
        );

        // Undo takes the events and their block off again.
        let timeline = made.made_blocks[0];
        let blocks = vec![now.blocks[0].id, timeline];
        let mut points: Vec<(Uuid, Uuid)> = now.points.iter().map(|p| (p.id, p.block_id)).collect();
        points.extend(made.made_points.iter().map(|p| (*p, timeline)));
        let undo = undo(topic, made, &blocks, &points);
        assert_eq!(undo.len(), 3);
        assert!(undo.iter().all(|o| matches!(o, Operation::Delete { .. })));
    }

    #[test]
    fn a_timeline_event_says_when_it_happened() {
        let (now, refs) = page(&[("text", "Geschichte", &[("Intifada", "…")])]);
        let mut layout = args(
            vec![block("b1", "", ""), block("", "timeline", "Zeitleiste")],
            Vec::new(),
        );
        layout.new_points = vec![NewPointArgs {
            from: "p1".into(),
            block: "Zeitleiste".into(),
            theme: "Intifada".into(),
            text: "Die erste Intifada beginnt".into(),
            happened: "damals".into(),
        }];
        let wrong = lay_out(Uuid::now_v7(), "Israel", &refs, &now, layout).unwrap_err();
        assert!(wrong.contains("says when it happened"), "{wrong}");
    }

    #[test]
    fn a_block_it_empties_goes_and_undo_brings_it_back() {
        let (now, refs) = page(&[
            (
                "chips",
                "Lisa",
                &[("", "Schnurrt laut"), ("", "Mag Thunfisch")],
            ),
            ("cards", "Wie sie ist", &[("Verhalten", "Schläft viel")]),
        ]);
        let layout = args(
            vec![block("b2", "", "")],
            vec![
                moving("p1", "b2", "Verhalten", "Sie schnurrt laut."),
                moving("p2", "b2", "Futter", "Sie mag Thunfisch."),
            ],
        );
        let topic = Uuid::now_v7();
        let laid = lay_out(topic, "Katzen", &refs, &now, layout).unwrap();
        let made = layout_of(&laid);
        assert_eq!(
            made.moves,
            vec![
                LayoutMove {
                    block: "Wie sie ist".into(),
                    change: "changed".into(),
                    holds: "Verhalten 2, Futter 1, 2 reworded to fit".into(),
                },
                LayoutMove {
                    block: "Lisa".into(),
                    change: "removed".into(),
                    holds: "nothing was left in it".into(),
                },
            ]
        );
        let lisa = now.blocks[0].id;
        assert!(laid.operations.iter().any(|o| matches!(
            o,
            Operation::Delete { model: ModelKind::Block, id } if *id == lisa
        )));

        // Undo, after it: Lisa is back, with her points as they were.
        let cards = now.blocks[1].id;
        let points: Vec<(Uuid, Uuid)> = now.points.iter().map(|p| (p.id, cards)).collect();
        let undo = undo(topic, made, &[cards], &points);
        assert!(undo.iter().any(|o| matches!(
            o,
            Operation::Create { model: ModelKind::Block, id, .. } if *id == lisa
        )));
        let back: Vec<&Value> = undo
            .iter()
            .filter_map(|o| match o {
                Operation::Update {
                    model: ModelKind::Point,
                    data,
                    ..
                } => data.get("text"),
                _ => None,
            })
            .collect();
        // Schläft viel only moved up the page, and goes back down.
        assert_eq!(
            back,
            vec![
                &json!("Schläft viel"),
                &json!("Schnurrt laut"),
                &json!("Mag Thunfisch")
            ]
        );
    }

    #[test]
    fn nothing_is_dropped() {
        let (now, refs) = page(&[
            ("chips", "Lisa", &[("", "Schnurrt laut")]),
            ("cards", "Wie sie ist", &[("Verhalten", "Schläft viel")]),
        ]);
        let left_out = lay_out(
            Uuid::now_v7(),
            "Katzen",
            &refs,
            &now,
            args(vec![block("b2", "", "")], Vec::new()),
        )
        .unwrap_err();
        assert!(left_out.contains("still keeps p1"), "{left_out}");
        let twice = lay_out(
            Uuid::now_v7(),
            "Katzen",
            &refs,
            &now,
            args(
                vec![block("b1", "", ""), block("b2", "", "")],
                vec![
                    moving("p1", "b2", "Verhalten", ""),
                    moving("p1", "", "", ""),
                ],
            ),
        )
        .unwrap_err();
        assert!(twice.contains("twice"), "{twice}");
        let empty = lay_out(
            Uuid::now_v7(),
            "Katzen",
            &refs,
            &now,
            args(
                vec![
                    block("b1", "", ""),
                    block("b2", "", ""),
                    block("", "list", "Lesen"),
                ],
                Vec::new(),
            ),
        )
        .unwrap_err();
        assert!(empty.contains("gets no points"), "{empty}");
    }

    #[test]
    fn a_point_fits_the_kind_it_moves_to() {
        let (now, refs) = page(&[
            (
                "chips",
                "Lisa",
                &[("", "Schnurrt laut"), ("", "Mag Thunfisch")],
            ),
            ("cards", "Wie sie ist", &[("Verhalten", "Schläft viel")]),
        ]);
        let no_theme = lay_out(
            Uuid::now_v7(),
            "Katzen",
            &refs,
            &now,
            args(
                vec![block("b1", "", ""), block("b2", "", "")],
                vec![moving("p1", "b2", "", "")],
            ),
        )
        .unwrap_err();
        assert!(no_theme.contains("needs a theme"), "{no_theme}");
        // Into chips, a card's point leaves its theme behind.
        let laid = lay_out(
            Uuid::now_v7(),
            "Katzen",
            &refs,
            &now,
            args(
                vec![block("b1", "", ""), block("b2", "", "")],
                vec![moving("p3", "b1", "", "")],
            ),
        )
        .unwrap();
        let theme = laid.operations.iter().find_map(|o| match o {
            Operation::Update {
                model: ModelKind::Point,
                id,
                data,
            } if *id == now.points[2].id => data.get("theme"),
            _ => None,
        });
        assert_eq!(theme, Some(&json!("")));
    }

    #[test]
    fn what_they_told_about_themselves_stays_on_about() {
        let (now, refs) = page(&[
            ("chips", "Terms", &[]),
            ("you", "You", &[("", "Low porosity")]),
        ]);
        let lay = |blocks| {
            lay_out(
                Uuid::now_v7(),
                "Haare",
                &refs,
                &now,
                args(blocks, Vec::new()),
            )
        };
        let kept = lay(vec![block("b2", "chips", ""), block("b1", "", "")]).unwrap_err();
        assert!(kept.contains("stays a you block"), "{kept}");
        let moved = lay(vec![block("b1", "you", ""), block("b2", "", "")]).unwrap_err();
        assert!(moved.contains("move what's about them"), "{moved}");
        assert!(lay(vec![block("b2", "", "Du"), block("b1", "", "")]).is_ok());

        // Left out, it stays, with what it keeps.
        let laid = lay(vec![block("b1", "", "Begriffe")]).unwrap();
        let you = now.blocks[1].id;
        assert!(!laid.operations.iter().any(|o| matches!(
            o,
            Operation::Delete { model: ModelKind::Block, id } if *id == you
        )));
        let moved: Vec<&str> = layout_of(&laid)
            .moves
            .iter()
            .map(|m| m.block.as_str())
            .collect();
        assert_eq!(moved, vec!["Begriffe"]);
    }

    #[test]
    fn only_the_block_that_moved_says_so() {
        let (now, refs) = page(&[
            ("chips", "A", &[]),
            ("chips", "B", &[]),
            ("chips", "C", &[]),
        ]);
        let laid = lay_out(
            Uuid::now_v7(),
            "Ordnung",
            &refs,
            &now,
            args(
                vec![
                    block("b3", "", ""),
                    block("b1", "", ""),
                    block("b2", "", ""),
                ],
                Vec::new(),
            ),
        )
        .unwrap();
        let moved: Vec<&str> = layout_of(&laid)
            .moves
            .iter()
            .map(|m| m.block.as_str())
            .collect();
        assert_eq!(moved, vec!["C"]);
        let again = lay_out(
            Uuid::now_v7(),
            "Ordnung",
            &refs,
            &now,
            args(
                vec![
                    block("b1", "", ""),
                    block("b2", "", ""),
                    block("b3", "", ""),
                ],
                Vec::new(),
            ),
        )
        .unwrap_err();
        assert!(again.contains("like that already"), "{again}");
    }
}
