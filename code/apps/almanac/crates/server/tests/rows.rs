//! A collection's rows as people and talks change them: each device sends
//! only the cells it changed, and the server merges them into the row; an
//! answer that took minutes keeps what people did meanwhile.

mod common;

use almanac_server::kept::{Answering, Kept, PointChange};
use almanac_server::page::Page;
use almanac_server::talks::write_kept;
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A topic with a checklist, "Umzug", whose rows are `names`, in order.
async fn checklist(app: &App, session: &str, home: &str, names: &[&str]) -> (Uuid, Vec<Uuid>) {
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(session),
            json!({ "name": "Umzug", "template": "routine" }),
        )
        .await;
    let topic: Uuid = started.body["id"].as_str().unwrap().parse().unwrap();
    let block = Uuid::now_v7();
    let fields = json!([
        { "name": "Done", "kind": "check", "unit": "", "goal": null, "options": [] },
        { "name": "Notiz", "kind": "text", "unit": "", "goal": null, "options": [] },
    ]);
    let rows: Vec<Uuid> = names.iter().map(|_| Uuid::now_v7()).collect();
    let mut operations = vec![json!({ "action": "create", "model": "Block", "id": block,
        "data": { "topic_id": topic, "kind": "collection", "title": "Umzug", "fields": fields } })];
    for (n, (name, id)) in names.iter().zip(&rows).enumerate() {
        operations.push(json!({ "action": "create", "model": "Point", "id": id,
            "data": { "topic_id": topic, "block_id": block, "text": name, "position": n } }));
    }
    let made = app.sync(session, home, Value::Array(operations)).await;
    assert!(made.status.is_success(), "{}", made.text);
    (topic, rows)
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_devices_ticking_different_cells_keep_both(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "Umzug", "template": "routine" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    let (block, row) = (uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let fields = json!([
        { "name": "Gepackt", "kind": "check", "unit": "", "goal": null, "options": [] },
        { "name": "Geputzt", "kind": "check", "unit": "", "goal": null, "options": [] },
        { "name": "Notiz", "kind": "text", "unit": "", "goal": null, "options": [] },
    ]);
    let made = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "create", "model": "Block", "id": block,
                  "data": { "topic_id": topic, "kind": "collection", "title": "Küche", "fields": fields } },
                { "action": "create", "model": "Point", "id": row,
                  "data": { "topic_id": topic, "block_id": block, "text": "Küche",
                            "cells": { "Notiz": "Herd" } } },
            ]),
        )
        .await;
    assert!(made.status.is_success(), "{}", made.text);

    // Each device shows the row whole meanwhile (`cells`), as it saw it.
    for (shown, changed) in [
        (
            json!({ "Notiz": "Herd", "Gepackt": true }),
            json!({ "Gepackt": true }),
        ),
        (
            json!({ "Notiz": "Herd", "Geputzt": true }),
            json!({ "Geputzt": true }),
        ),
    ] {
        let ticked = app
            .sync(
                &ada,
                &home,
                json!([{ "action": "update", "model": "Point", "id": row,
                         "data": { "cells": shown, "changed_cells": changed } }]),
            )
            .await;
        assert!(ticked.status.is_success(), "{}", ticked.text);
    }
    let rows = app.synced(&ada, &home, "Point").await;
    let kept = rows.iter().find(|p| p["id"] == json!(row)).unwrap();
    assert_eq!(
        kept["cells"],
        json!({ "Notiz": "Herd", "Gepackt": true, "Geputzt": true })
    );

    let cleared = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Point", "id": row,
                     "data": { "changed_cells": { "Notiz": null } } }]),
        )
        .await;
    assert!(cleared.status.is_success(), "{}", cleared.text);
    let rows = app.synced(&ada, &home, "Point").await;
    let kept = rows.iter().find(|p| p["id"] == json!(row)).unwrap();
    assert_eq!(kept["cells"], json!({ "Gepackt": true, "Geputzt": true }));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_answer_keeps_what_people_changed_while_it_was_thinking(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, home) = app.signup("Ada", "Home").await;
    let names = ["Küche", "Bad", "Keller", "Dachboden", "Garage"];
    let (topic, rows) = checklist(&app, &ada, &home, &names).await;
    let [kitchen, bath, cellar, attic, garage] = rows[..] else {
        unreachable!()
    };
    let workspace: Uuid = home.parse().unwrap();
    let page = Page::load(&pool, workspace, topic).await.unwrap().unwrap();
    let refs = page.refs();
    let at = |id: Uuid| {
        format!(
            "p{}",
            refs.points.iter().position(|p| p.id == id).unwrap() + 1
        )
    };
    let block = refs.blocks.iter().position(|b| b.title == "Umzug").unwrap() + 1;

    // The answer, from the page as it was asked: Küche done, the
    // Dachboden moved to the top, which numbers the others anew, and notes
    // on the Keller and the Garage.
    let mut kept = Kept::on(page.refs());
    kept.move_points(&[at(attic)], &format!("b{block}"), Some(""))
        .unwrap();
    let cells = |point: Uuid, cells: Value| PointChange {
        point: at(point),
        cells: cells.as_object().cloned(),
        ..PointChange::default()
    };
    kept.change_points(&[
        cells(kitchen, json!({ "Done": true })),
        cells(cellar, json!({ "Notiz": "Zuerst" })),
        cells(garage, json!({ "Notiz": "Leer" })),
    ])
    .unwrap();

    // Meanwhile: a note on the Küche, the Keller moved to the end, and the
    // Garage gone.
    let meanwhile = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "update", "model": "Point", "id": kitchen,
                  "data": { "changed_cells": { "Notiz": "Herd abbauen" } } },
                { "action": "update", "model": "Point", "id": cellar, "data": { "position": 10 } },
                { "action": "delete", "model": "Point", "id": garage },
            ]),
        )
        .await;
    assert!(meanwhile.status.is_success(), "{}", meanwhile.text);

    let answering = Answering {
        topic_id: topic,
        topic_name: "Umzug".into(),
        talk_id: Uuid::now_v7(),
        answer_id: Uuid::now_v7(),
        unnamed: false,
    };
    let (_, operations) = kept.writes(&answering);
    let state = common::state(pool);
    let actor = almanac_domain::Actor {
        workspace_id: workspace,
        user_id: app.person("Ada").await,
    };
    write_kept(&state, actor, operations, &kept.along())
        .await
        .unwrap();

    let points = app.synced(&ada, &home, "Point").await;
    let row = |id: Uuid| points.iter().find(|p| p["id"] == json!(id)).cloned();
    let kitchen = row(kitchen).unwrap();
    assert_eq!(
        kitchen["cells"],
        json!({ "Done": true, "Notiz": "Herd abbauen" })
    );
    assert_eq!(row(attic).unwrap()["position"], json!(0));
    assert_eq!(kitchen["position"], json!(1));
    assert_eq!(row(bath).unwrap()["position"], json!(2));
    let cellar = row(cellar).unwrap();
    assert_eq!(cellar["position"], json!(10));
    assert_eq!(cellar["cells"], json!({ "Notiz": "Zuerst" }));
    assert!(row(garage).is_none());
}
