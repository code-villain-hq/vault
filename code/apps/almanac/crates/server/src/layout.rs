//! Undo for a page an answer changed (`crate::kept`): the blocks and points
//! it changed, moved or removed go back as they were, and the ones it
//! started go, with the blocks it started once they hold nothing else.

use almanac_domain::{ModelKind, Operation, PageLayout};
use serde_json::json;
use uuid::Uuid;

use crate::kept::fields;

/// The writes that put a page back as `layout` says it was, given the
/// `blocks` it has now and its `points`, each with its block.
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
            ("facets", json!(block.facets)),
            ("fields", json!(block.fields)),
            ("width", json!(block.width)),
            ("view", json!(block.view)),
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
        if !back.contains(&point.block_id) {
            continue;
        }
        let data = fields([
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
        let there = points.iter().any(|(id, _)| *id == point.id);
        if point.removed && !there {
            // Made again as it was, with where it came from.
            let mut data = data;
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
        } else if !point.removed && there {
            operations.push(Operation::Update {
                model: ModelKind::Point,
                id: point.id,
                data,
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
    use std::collections::BTreeMap;

    use almanac_domain::{BlockWas, Cell, PointWas};

    use super::*;

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
        }
    }

    #[test]
    fn undo_makes_again_what_went_and_takes_off_what_came() {
        let topic = Uuid::now_v7();
        let (weight, notes, made) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let (row, changed, new) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let layout = PageLayout {
            moves: Vec::new(),
            blocks: vec![BlockWas {
                id: weight,
                kind: "collection".into(),
                title: "Gewicht".into(),
                facets: Vec::new(),
                width: String::new(),
                view: "bars".into(),
                fields: Vec::new(),
                position: 2,
                removed: true,
            }],
            points: vec![
                point_was(row, weight, true),
                point_was(changed, notes, false),
            ],
            made_blocks: vec![made],
            made_points: vec![new],
            collections: Vec::new(),
            rows: Vec::new(),
        };
        let operations = undo(
            topic,
            &layout,
            &[notes, made],
            &[(changed, notes), (new, made)],
        );
        assert!(
            matches!(
                &operations[..],
                [
                    Operation::Create { model: ModelKind::Block, id: b, data: block },
                    Operation::Create { model: ModelKind::Point, id: p, data: point },
                    Operation::Update { id: u, .. },
                    Operation::Delete { id: d, .. },
                    Operation::Delete { model: ModelKind::Block, id: gone },
                ] if *b == weight && block["view"] == json!("bars")
                    && *p == row && point["source"] == json!("You said") && point["cells"] == json!({"Gewicht": 3.5})
                    && *u == changed && *d == new && *gone == made
            ),
            "{operations:?}"
        );
    }
}
