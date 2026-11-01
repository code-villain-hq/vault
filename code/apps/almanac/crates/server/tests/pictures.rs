//! A picture block, as a person adds one: a block of its own and a point a
//! photo, its caption left out and written later.

mod common;

use common::App;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn a_person_adds_a_picture_and_its_caption(pool: PgPool) {
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let started = app
        .post(
            &format!("/api/w/{home}/topics"),
            Some(&ada),
            json!({ "name": "KI verstehen", "template": "study" }),
        )
        .await;
    let topic = started.body["id"].as_str().unwrap().to_string();
    let (block, point) = (uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let photo = json!({ "photo": uuid::Uuid::now_v7() });
    let wrote = app
        .sync(
            &ada,
            &home,
            json!([
                { "action": "create", "model": "Block", "id": block,
                  "data": { "topic_id": topic, "kind": "picture", "title": "" } },
                { "action": "create", "model": "Point", "id": point,
                  "data": { "topic_id": topic, "block_id": block, "text": "", "cells": photo } },
            ]),
        )
        .await;
    assert!(wrote.status.is_success(), "{}", wrote.text);

    let captioned = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Point", "id": point,
                     "data": { "text": "Ein Wort nach dem anderen", "cells": photo } }]),
        )
        .await;
    assert!(captioned.status.is_success(), "{}", captioned.text);
    let cleared = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Point", "id": point,
                     "data": { "text": "", "cells": photo } }]),
        )
        .await;
    assert!(cleared.status.is_success(), "{}", cleared.text);

    // Anywhere else a point needs its words.
    let wordless = app
        .sync(
            &ada,
            &home,
            json!([{ "action": "update", "model": "Point", "id": point, "data": { "text": "" } }]),
        )
        .await;
    assert!(!wordless.status.is_success(), "{}", wordless.text);
}
