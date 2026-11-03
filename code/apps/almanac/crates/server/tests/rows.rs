//! A collection's rows as people change them by hand: each device sends
//! only the cells it changed, and the server merges them into the row.

mod common;

use common::App;
use serde_json::json;
use sqlx::PgPool;

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
