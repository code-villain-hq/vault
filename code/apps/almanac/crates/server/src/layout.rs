//! Undo for a page an answer changed (`crate::kept`): the blocks and points
//! it changed, moved or removed go back as they were, and the ones it
//! started go, with the blocks it started once they hold nothing else. Of
//! a row's cells and a block's or point's place, only what the answer
//! changed goes back, and only while it's as the answer left it: what
//! people changed since stays.

use std::collections::BTreeMap;

use almanac_domain::{Cell, ModelKind, Operation, PICTURE_CELL, PageLayout, PointWas};
use serde_json::json;
use uuid::Uuid;

use crate::kept::fields;
use crate::page::{PageBlock, PagePoint};

/// The writes that put a page back as `layout` says it was, given the
/// `page`'s blocks as they are now, each with its points.
pub fn undo(topic_id: Uuid, layout: &PageLayout, page: &[PageBlock]) -> Vec<Operation> {
    let blocks: Vec<Uuid> = page.iter().map(|b| b.block.id).collect();
    let points: Vec<&PagePoint> = page.iter().flat_map(|b| &b.points).collect();
    let mut operations = Vec::new();
    let mut back: Vec<Uuid> = blocks.clone();
    for block in &layout.blocks {
        let mut data = fields([
            ("kind", json!(block.kind)),
            ("title", json!(block.title)),
            ("facets", json!(block.facets)),
            ("fields", json!(block.fields)),
            ("width", json!(block.width)),
            ("view", json!(block.view)),
            ("row_id", json!(block.row_id)),
            ("position", json!(block.position)),
        ]);
        if block.removed && !blocks.contains(&block.id) {
            data.insert("topic_id".into(), json!(topic_id));
            operations.push(Operation::Create {
                model: ModelKind::Block,
                id: block.id,
                data,
            });
            back.push(block.id);
        } else if !block.removed
            && let Some(now) = page.iter().find(|b| b.block.id == block.id)
        {
            // Its place goes back only if the answer moved it there and
            // nobody moved it since.
            if let Some(wrote) = &block.wrote
                && wrote.moved_to != Some(now.block.position)
            {
                data.remove("position");
            }
            operations.push(Operation::Update {
                model: ModelKind::Block,
                id: block.id,
                data,
            });
        }
    }
    let mut put_back = Vec::new();
    for point in &layout.points {
        if !back.contains(&point.block_id) {
            continue;
        }
        let mut data = fields([
            ("block_id", json!(point.block_id)),
            ("theme", json!(point.theme)),
            ("text", json!(point.text)),
            ("happened", json!(point.happened)),
            ("facet", json!(point.facet)),
            ("look", json!(point.look)),
            ("amount", json!(point.amount)),
            ("note", json!(point.note)),
            ("cells", json!(point.cells)),
            ("position", json!(point.position)),
        ]);
        let now = points.iter().find(|p| p.id == point.id);
        if point.removed && now.is_none() {
            // Made again as it was, with where it came from.
            data.insert("topic_id".into(), json!(topic_id));
            data.insert("done".into(), json!(point.done));
            data.insert("source".into(), json!(point.source));
            data.insert("source_url".into(), json!(point.source_url));
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id: point.id,
                data,
            });
            put_back.push(point.id);
        } else if !point.removed
            && let Some(now) = now
        {
            // An answer kept before Undo knew what it wrote puts its cells
            // and place back whole.
            let mut moves = true;
            if point.wrote.is_some() {
                data.remove("cells");
                let changed = cells_back(point, now);
                if !changed.is_empty() {
                    data.insert("changed_cells".into(), json!(changed));
                }
                moves = point
                    .wrote
                    .as_ref()
                    .and_then(|w| w.moved_to)
                    .is_some_and(|to| to == (now.block_id, now.position));
                if !moves {
                    data.remove("block_id");
                    data.remove("position");
                }
            }
            if moves {
                put_back.push(point.id);
            }
            operations.push(Operation::Update {
                model: ModelKind::Point,
                id: point.id,
                data,
            });
        }
    }
    for &id in &layout.made_points {
        if points.iter().any(|p| p.id == id) {
            operations.push(Operation::Delete {
                model: ModelKind::Point,
                id,
            });
        }
    }
    for &id in &layout.made_blocks {
        let empty = points.iter().all(|p| {
            p.block_id != id || put_back.contains(&p.id) || layout.made_points.contains(&p.id)
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

/// The cells of `point` to put back as they were before the answer, none
/// for one it didn't have: only those the answer changed and that are
/// still as it left them, so a cell someone changed since keeps theirs.
/// A picture whose caption goes back to none gives its photo again, which
/// a point with no words needs.
fn cells_back(point: &PointWas, now: &PagePoint) -> BTreeMap<String, Option<Cell>> {
    let mut back: BTreeMap<String, Option<Cell>> = point
        .wrote
        .iter()
        .flat_map(|w| &w.cells)
        .filter(|(name, left)| now.cells.0.get(*name) == left.as_ref())
        .map(|(name, _)| (name.clone(), point.cells.get(name).cloned()))
        .collect();
    if point.text.is_empty()
        && let Some(photo) = point.cells.get(PICTURE_CELL)
    {
        back.entry(PICTURE_CELL.into())
            .or_insert_with(|| Some(photo.clone()));
    }
    back
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use almanac_domain::{BlockWas, BlockWrote, Cell, PointWas, PointWrote};
    use serde_json::Value;
    use sqlx::types::Json;

    use super::*;
    use crate::page::RefBlock;

    fn point_was(id: Uuid, block_id: Uuid, removed: bool) -> PointWas {
        PointWas {
            id,
            block_id,
            theme: String::new(),
            text: "Lisa".into(),
            happened: String::new(),
            facet: String::new(),
            look: String::new(),
            amount: None,
            note: String::new(),
            cells: BTreeMap::from([("Gewicht".into(), Cell::Number(3.5))]),
            done: false,
            position: 0,
            removed,
            source: Some("You said".into()),
            source_url: None,
            wrote: None,
        }
    }

    fn block_was(id: Uuid, position: i32, wrote: Option<BlockWrote>) -> BlockWas {
        BlockWas {
            id,
            kind: "collection".into(),
            title: "Gewicht".into(),
            facets: Vec::new(),
            width: String::new(),
            view: "bars".into(),
            fields: Vec::new(),
            row_id: None,
            position,
            removed: false,
            wrote,
        }
    }

    fn layout(blocks: Vec<BlockWas>, points: Vec<PointWas>) -> PageLayout {
        PageLayout {
            moves: Vec::new(),
            blocks,
            points,
            made_blocks: Vec::new(),
            made_points: Vec::new(),
            collections: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// A block as the page has it now, at `position`, with `points`.
    fn now(id: Uuid, position: i32, points: Vec<PagePoint>) -> PageBlock {
        PageBlock {
            block: RefBlock {
                id,
                kind: "collection".into(),
                title: String::new(),
                facets: Json(Vec::new()),
                fields: Json(Vec::new()),
                width: String::new(),
                view: String::new(),
                row_id: None,
                position,
            },
            points,
        }
    }

    /// A point as the page has it now, at `position` in `block_id`.
    fn point_now(id: Uuid, block_id: Uuid, position: i32, cells: Value) -> PagePoint {
        PagePoint {
            id,
            block_id,
            theme: String::new(),
            text: "Lisa".into(),
            happened: String::new(),
            facet: String::new(),
            look: String::new(),
            amount: None,
            note: String::new(),
            cells: Json(serde_json::from_value(cells).unwrap()),
            done: false,
            position,
            source: None,
            source_url: None,
        }
    }

    fn update(operations: &[Operation], of: Uuid) -> &serde_json::Map<String, Value> {
        operations
            .iter()
            .find_map(|o| match o {
                Operation::Update { id, data, .. } if *id == of => Some(data),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no update of {of}: {operations:?}"))
    }

    #[test]
    fn undo_makes_again_what_went_and_takes_off_what_came() {
        let topic = Uuid::now_v7();
        let (weight, notes, made) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let (row, changed, new) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let mut layout = layout(
            vec![BlockWas {
                removed: true,
                ..block_was(weight, 2, None)
            }],
            vec![
                point_was(row, weight, true),
                point_was(changed, notes, false),
            ],
        );
        layout.made_blocks = vec![made];
        layout.made_points = vec![new];
        let page = [
            now(notes, 0, vec![point_now(changed, notes, 0, json!({}))]),
            now(made, 1, vec![point_now(new, made, 0, json!({}))]),
        ];
        let operations = undo(topic, &layout, &page);
        assert!(
            matches!(
                &operations[..],
                [
                    Operation::Create { model: ModelKind::Block, id: b, data: block },
                    Operation::Create { model: ModelKind::Point, id: p, data: point },
                    Operation::Update { id: u, data: whole, .. },
                    Operation::Delete { id: d, .. },
                    Operation::Delete { model: ModelKind::Block, id: gone },
                ] if *b == weight && block["view"] == json!("bars")
                    && *p == row && point["source"] == json!("You said") && point["cells"] == json!({"Gewicht": 3.5})
                    // An answer from before Undo knew what it wrote: whole.
                    && *u == changed && whole["cells"] == json!({"Gewicht": 3.5})
                    && whole["position"] == json!(0)
                    && *d == new && *gone == made
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn undo_puts_back_only_the_cells_the_answer_changed_while_they_are_as_it_left_them() {
        let (kitchen, row) = (Uuid::now_v7(), Uuid::now_v7());
        let was = PointWas {
            cells: BTreeMap::from([("Notiz".into(), Cell::Text("Herd".into()))]),
            wrote: Some(PointWrote {
                cells: BTreeMap::from([
                    ("Gepackt".into(), Some(Cell::Check(true))),
                    ("Notiz".into(), Some(Cell::Text("Herd und Ofen".into()))),
                ]),
                moved_to: None,
            }),
            ..point_was(row, kitchen, false)
        };
        // Since the answer someone ticked Geputzt and wrote another Notiz.
        let page = [now(
            kitchen,
            0,
            vec![point_now(
                row,
                kitchen,
                4,
                json!({"Gepackt": true, "Geputzt": true, "Notiz": "Spüle"}),
            )],
        )];
        let operations = undo(Uuid::now_v7(), &layout(Vec::new(), vec![was]), &page);
        let data = update(&operations, row);
        assert_eq!(data["changed_cells"], json!({"Gepackt": null}));
        assert!(data.get("cells").is_none(), "{data:?}");
        // The answer didn't move it: it stays where it is.
        assert!(data.get("position").is_none() && data.get("block_id").is_none());
    }

    #[test]
    fn undo_moves_back_only_what_the_answer_moved_and_nobody_moved_since() {
        let (a, b, c) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let (still, since) = (Uuid::now_v7(), Uuid::now_v7());
        let moved = |id, to| PointWas {
            wrote: Some(PointWrote {
                cells: BTreeMap::new(),
                moved_to: Some(to),
            }),
            ..point_was(id, a, false)
        };
        let layout = layout(
            vec![
                block_was(b, 1, Some(BlockWrote { moved_to: Some(0) })),
                block_was(c, 2, Some(BlockWrote { moved_to: Some(1) })),
            ],
            vec![moved(still, (b, 3)), moved(since, (b, 4))],
        );
        let page = [
            now(a, 2, Vec::new()),
            // b is where the answer put it; c someone moved since.
            now(b, 0, vec![point_now(still, b, 3, json!({}))]),
            now(c, 5, vec![point_now(since, c, 0, json!({}))]),
        ];
        let operations = undo(Uuid::now_v7(), &layout, &page);
        let back = update(&operations, still);
        assert_eq!(back["block_id"], json!(a));
        assert_eq!(back["position"], json!(0));
        let left = update(&operations, since);
        assert!(left.get("block_id").is_none() && left.get("position").is_none());
        assert_eq!(update(&operations, b)["position"], json!(1));
        assert!(update(&operations, c).get("position").is_none());
    }

    #[test]
    fn a_picture_whose_caption_goes_back_to_none_gives_its_photo_again() {
        let (pictures, photo) = (Uuid::now_v7(), Uuid::now_v7());
        let was = PointWas {
            text: String::new(),
            cells: BTreeMap::from([(PICTURE_CELL.into(), Cell::Text("abc".into()))]),
            wrote: Some(PointWrote::default()),
            ..point_was(photo, pictures, false)
        };
        let page = [now(
            pictures,
            0,
            vec![point_now(photo, pictures, 0, json!({"photo": "abc"}))],
        )];
        let operations = undo(Uuid::now_v7(), &layout(Vec::new(), vec![was]), &page);
        let data = update(&operations, photo);
        assert_eq!(data["text"], json!(""));
        assert_eq!(data["changed_cells"], json!({"photo": "abc"}));
    }
}
