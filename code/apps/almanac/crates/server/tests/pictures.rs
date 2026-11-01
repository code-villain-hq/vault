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

/// A photo the app named itself, as it does offline: sent again it's kept
/// once, and its id is this workspace's only.
#[sqlx::test(migrations = "../../migrations")]
async fn a_photo_named_in_the_app_is_kept_once(pool: PgPool) {
    use base64::Engine;
    let app = App::new(pool);
    let (ada, home) = app.signup("Ada", "Home").await;
    let (bea, away) = app.signup("Bea", "Away").await;
    let photo = uuid::Uuid::now_v7();
    let upload = json!({
        "media_type": "image/jpeg",
        "data": base64::engine::general_purpose::STANDARD.encode(b"jpeg bytes"),
        "width": 800,
        "height": 600,
    });
    let path = format!("/api/w/{home}/photos/{photo}");
    for _ in 0..2 {
        let kept = app
            .call("PUT", &path, Some(&ada), Some(upload.clone()))
            .await;
        assert!(kept.status.is_success(), "{}", kept.text);
        assert_eq!(kept.body["id"], json!(photo));
    }
    let shown = app.get(&path, &ada).await;
    assert_eq!(shown.text, "jpeg bytes");

    let taken = app
        .call(
            "PUT",
            &format!("/api/w/{away}/photos/{photo}"),
            Some(&bea),
            Some(upload),
        )
        .await;
    assert!(!taken.status.is_success(), "{}", taken.text);
    let elsewhere = app
        .get(&format!("/api/w/{away}/photos/{photo}"), &bea)
        .await;
    assert!(!elsewhere.status.is_success());
}
