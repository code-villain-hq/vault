//! The HTTP API end to end: requests go through the real router to a fresh
//! Postgres database per test.

mod common;

use axum::http::StatusCode;
use common::App;
use raven_server::accounts;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// Adds an app the way Settings → Apps does; returns its id.
async fn add_app(app: &App, session: &str, workspace: &str, key: &str, health_url: Value) -> Uuid {
    let id = Uuid::now_v7();
    let created = app
        .sync(
            session,
            workspace,
            json!([{ "action": "create", "model": "App", "id": id, "data": {
                "key": key, "name": key, "health_url": health_url,
            }}]),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    id
}

fn span(trace_id: &str) -> Value {
    json!({ "resourceSpans": [{ "scopeSpans": [{ "spans": [{
        "traceId": trace_id,
        "spanId": "EEE19B7EC3C1B174",
        "name": "GET /api/specs/{id}",
        "kind": 2,
        "startTimeUnixNano": "1760000000000000000",
        "endTimeUnixNano": "1760000000180000000",
    }]}]}]})
}

#[sqlx::test(migrations = "../../migrations")]
async fn setting_up_raven_makes_a_workspace_that_watches_raven(pool: PgPool) {
    let app = App::new(pool);
    let setup = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(setup.body["setup"], true);

    let (session, workspace) = app.signup("Ada", "Code Villain").await;
    let me = app.get("/api/me", &session).await;
    assert_eq!(me.body["workspaces"][0]["url_key"], "code-villain");
    assert_eq!(me.body["workspaces"][0]["role"], "admin");

    let apps = app.synced_apps(&session, &workspace).await;
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0]["key"], "raven");
    assert_eq!(apps[0]["is_self"], true);
    assert_eq!(apps[0]["health_url"], Value::Null);

    // Raven is set up once; everyone after comes with an invite.
    let again = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::FORBIDDEN, "{}", again.body);
    let setup = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(setup.body["setup"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invite_links_let_people_join(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;

    let me = app.get("/api/me", &grace).await;
    assert_eq!(me.body["workspaces"][0]["id"], workspace.as_str());
    assert_eq!(me.body["workspaces"][0]["role"], "member");
    // Members see what the workspace watches, and how it is.
    assert_eq!(app.synced_apps(&grace, &workspace).await.len(), 1);
    let statuses = app.get(&format!("/api/w/{workspace}/apps"), &grace).await;
    assert_eq!(statuses.status, StatusCode::OK, "{}", statuses.body);
    assert_eq!(statuses.body[0]["key"], "raven");
    let health = app
        .get(&format!("/api/w/{workspace}/apps/raven/health"), &grace)
        .await;
    assert_eq!(health.status, StatusCode::OK, "{}", health.body);
    assert_eq!(health.body["is_self"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_routes_need_membership(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let token = app.invite(&ada, &studio, "member").await;
    let grace = app.join("Grace", &token).await;

    // A second workspace, only Ada's.
    let ada_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'ada@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut db = pool.begin().await.unwrap();
    let side = accounts::create_workspace(&mut db, "Side project", ada_id)
        .await
        .unwrap();
    db.commit().await.unwrap();
    // Raven itself stays with the first workspace.
    assert!(app.synced_apps(&ada, &side.to_string()).await.is_empty());
    add_app(
        &app,
        &ada,
        &side.to_string(),
        "forge",
        json!("http://127.0.0.1:7385/api/health"),
    )
    .await;

    for path in [
        format!("/api/w/{side}/apps"),
        format!("/api/w/{side}/apps/forge/health"),
        format!("/api/w/{side}/apps/forge/technical"),
        format!("/api/w/{side}/apps/forge/action?name=GET%20%2F"),
        format!("/api/w/{side}/apps/forge/calls/abc"),
        format!("/api/w/{side}/sync/bootstrap"),
        format!("/api/w/{side}/members"),
    ] {
        assert_eq!(
            app.get(&path, &grace).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let key = app
        .post(
            &format!("/api/w/{side}/apps/forge/key"),
            Some(&grace),
            json!({}),
        )
        .await;
    assert_eq!(key.status, StatusCode::NOT_FOUND);
    // Nor does another workspace's app show in hers.
    let forge = app
        .get(&format!("/api/w/{studio}/apps/forge/health"), &grace)
        .await;
    assert_eq!(forge.status, StatusCode::NOT_FOUND);

    let health = app
        .get(&format!("/api/w/{side}/apps/forge/health"), &ada)
        .await;
    assert_eq!(health.status, StatusCode::OK, "{}", health.body);
    assert_eq!(
        health.body["health_url"],
        "http://127.0.0.1:7385/api/health"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_change_which_apps_are_watched(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;

    let create = |key: &str, mut data: Value| {
        data["key"] = json!(key);
        json!([{ "action": "create", "model": "App", "id": Uuid::now_v7(), "data": data }])
    };
    let refused = app
        .sync(
            &grace,
            &workspace,
            create("forge", json!({ "name": "Forge" })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);

    let forge = add_app(
        &app,
        &ada,
        &workspace,
        "forge",
        json!("http://127.0.0.1:7385/api/health"),
    )
    .await;
    let apps = app.synced_apps(&grace, &workspace).await;
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[1]["key"], "forge");
    assert_eq!(apps[1]["position"], 1);

    for (data, status) in [
        (
            json!({ "name": "Glyph", "health_url": "glyph.local" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Glyph", "is_self": true }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (json!({ "name": "Forge again" }), StatusCode::CONFLICT),
    ] {
        let key = if status == StatusCode::CONFLICT {
            "forge"
        } else {
            "glyph"
        };
        let response = app.sync(&ada, &workspace, create(key, data.clone())).await;
        assert_eq!(response.status, status, "{data} gave {}", response.body);
    }

    let raven = apps[0]["id"].as_str().unwrap();
    let delete_raven = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "delete", "model": "App", "id": raven }]),
        )
        .await;
    assert_eq!(delete_raven.status, StatusCode::FORBIDDEN);
    let move_raven = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "update", "model": "App", "id": raven, "data": { "health_url": "http://x/api/health" } }]),
        )
        .await;
    assert_eq!(move_raven.status, StatusCode::FORBIDDEN);
    let rename_raven = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "update", "model": "App", "id": raven, "data": { "name": "Our Raven" } }]),
        )
        .await;
    assert_eq!(rename_raven.status, StatusCode::OK, "{}", rename_raven.body);

    let grace_removes = app
        .sync(
            &grace,
            &workspace,
            json!([{ "action": "delete", "model": "App", "id": forge }]),
        )
        .await;
    assert_eq!(grace_removes.status, StatusCode::FORBIDDEN);
    let ada_removes = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "delete", "model": "App", "id": forge }]),
        )
        .await;
    assert_eq!(ada_removes.status, StatusCode::OK);
    assert_eq!(app.synced_apps(&ada, &workspace).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_key_is_shown_once_and_says_which_app_sends(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;
    let forge = add_app(&app, &ada, &workspace, "forge", Value::Null).await;
    let key_path = format!("/api/w/{workspace}/apps/forge/key");

    assert_eq!(
        app.post(&key_path, Some(&grace), json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    let raven = app
        .post(
            &format!("/api/w/{workspace}/apps/raven/key"),
            Some(&ada),
            json!({}),
        )
        .await;
    assert_eq!(raven.status, StatusCode::UNPROCESSABLE_ENTITY);

    let first = app.post(&key_path, Some(&ada), json!({})).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let first = first.body["key"].as_str().unwrap().to_string();
    // Only its hash is kept, and it never syncs.
    let synced = app.synced_apps(&ada, &workspace).await;
    assert!(!Value::from(synced).to_string().contains(&first));

    assert_eq!(
        app.send("/v1/traces", &first, span("AA")).await,
        StatusCode::OK
    );
    let app_id: Uuid = sqlx::query_scalar("SELECT app_id FROM spans WHERE trace_id = 'aa'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(app_id, forge);
    let health = app
        .get(&format!("/api/w/{workspace}/apps/forge/health"), &grace)
        .await;
    assert_eq!(health.body["sending"], true);

    // A new key replaces the old one.
    let second = app.post(&key_path, Some(&ada), json!({})).await;
    let second = second.body["key"].as_str().unwrap().to_string();
    assert_ne!(first, second);
    assert_eq!(
        app.send("/v1/traces", &first, span("BB")).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.send("/v1/traces", &second, span("BB")).await,
        StatusCode::OK
    );
    assert_eq!(
        app.send("/v1/traces", "not-a-key", span("CC")).await,
        StatusCode::UNAUTHORIZED
    );

    // Removing the app takes what it sent with it, and its key.
    app.sync(
        &ada,
        &workspace,
        json!([{ "action": "delete", "model": "App", "id": forge }]),
    )
    .await;
    assert_eq!(
        app.send("/v1/traces", &second, span("DD")).await,
        StatusCode::UNAUTHORIZED
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_guest_sees_only_the_apps_an_admin_adds_them_to(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let forge = add_app(&app, &ada, &workspace, "forge", Value::Null).await;
    add_app(&app, &ada, &workspace, "glyph", Value::Null).await;
    let token = app.invite(&ada, &workspace, "guest").await;
    let gus = app.join("Gus", &token).await;
    let gus_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'gus@example.com'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Added to nothing, a guest sees no app.
    let me = app.get("/api/me", &gus).await;
    assert_eq!(me.body["workspaces"][0]["role"], "guest");
    assert!(app.synced_apps(&gus, &workspace).await.is_empty());
    let health = app
        .get(&format!("/api/w/{workspace}/apps/forge/health"), &gus)
        .await;
    assert_eq!(health.status, StatusCode::NOT_FOUND, "{}", health.body);
    let epoch_before = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &gus)
        .await
        .body["epoch"]
        .clone();

    // Only admins add guests to apps.
    let access = format!("/api/w/{workspace}/members/{gus_id}/access");
    let refused = app
        .call(
            "PUT",
            &access,
            Some(&gus),
            Some(json!({ "items": [forge] })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    let added = app
        .call(
            "PUT",
            &access,
            Some(&ada),
            Some(json!({ "items": [forge] })),
        )
        .await;
    assert_eq!(added.status, StatusCode::NO_CONTENT, "{}", added.body);

    // Now Forge, and only Forge, on every page; the guest's copy starts over.
    let bootstrap = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &gus)
        .await;
    assert_ne!(bootstrap.body["epoch"], epoch_before);
    let apps = app.synced_apps(&gus, &workspace).await;
    let keys: Vec<&str> = apps.iter().filter_map(|a| a["key"].as_str()).collect();
    assert_eq!(keys, ["forge"]);
    let statuses = app.get(&format!("/api/w/{workspace}/apps"), &gus).await;
    assert_eq!(statuses.status, StatusCode::OK, "{}", statuses.body);
    assert_eq!(statuses.body.as_array().unwrap().len(), 1);
    assert_eq!(statuses.body[0]["key"], "forge");
    for (key, status) in [("forge", StatusCode::OK), ("glyph", StatusCode::NOT_FOUND)] {
        let health = app
            .get(&format!("/api/w/{workspace}/apps/{key}/health"), &gus)
            .await;
        assert_eq!(health.status, status, "{key}: {}", health.body);
    }

    // A guest's Settings are their account's alone, and they change no app.
    let members = app.get(&format!("/api/w/{workspace}/members"), &gus).await;
    assert_eq!(members.status, StatusCode::FORBIDDEN, "{}", members.body);
    let rename = app
        .sync(
            &gus,
            &workspace,
            json!([{ "action": "update", "model": "App", "id": forge, "data": { "name": "Mine" } }]),
        )
        .await;
    assert_ne!(rename.status, StatusCode::OK, "{}", rename.body);

    // Admins see whom they added where; taking it away hides Forge again.
    let members = app.get(&format!("/api/w/{workspace}/members"), &ada).await;
    let row = members
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user"]["name"] == "Gus")
        .unwrap()
        .clone();
    assert_eq!(row["role"], "guest");
    assert_eq!(row["access"], json!([forge]));
    let removed = app
        .call("PUT", &access, Some(&ada), Some(json!({ "items": [] })))
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.body);
    assert!(app.synced_apps(&gus, &workspace).await.is_empty());

    // A member sees every app, whatever their access says.
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;
    assert_eq!(app.synced_apps(&grace, &workspace).await.len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_server_makes_a_workspace_for_someone_with_an_account(pool: PgPool) {
    let app = App::new(pool);
    let (ada, _) = app.signup("Ada", "Acme").await;

    let url_key = accounts::create_workspace_for(&app.pool, " Studio ", "ADA@example.com")
        .await
        .unwrap();

    assert_eq!(url_key, "studio");
    let me = app.get("/api/me", &ada).await.body;
    let studio = me["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["url_key"] == "studio")
        .expect("Ada is in Studio");
    assert_eq!(studio["name"], "Studio");
    assert_eq!(studio["role"], "admin");
    assert!(
        accounts::create_workspace_for(&app.pool, "Studio", "nobody@example.com")
            .await
            .is_err()
    );
}
