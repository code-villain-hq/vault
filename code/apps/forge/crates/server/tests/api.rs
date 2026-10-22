//! The HTTP API end to end: requests go through the real router to a fresh
//! Postgres database per test.

mod common;

use axum::http::StatusCode;
use common::App;
use forge_server::accounts;
use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn signing_up_creates_a_workspace_you_administer(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Analytical Engines").await;

    let me = app.get("/api/me", &session).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["user"]["email"], "ada@example.com");
    assert_eq!(me.body["workspaces"][0]["name"], "Analytical Engines");
    assert_eq!(me.body["workspaces"][0]["url_key"], "analytical-engines");
    assert_eq!(me.body["workspaces"][0]["role"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_url_keys_are_unique(pool: PgPool) {
    let app = App::new(pool);
    let (first, _) = app.signup("Ada", "Acme").await;
    let (second, _) = app.signup("Grace", "Acme").await;

    assert_eq!(
        app.get("/api/me", &first).await.body["workspaces"][0]["url_key"],
        "acme"
    );
    assert_eq!(
        app.get("/api/me", &second).await.body["workspaces"][0]["url_key"],
        "acme-2"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn signup_rejects_bad_input_and_taken_emails(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;

    let cases = [
        (
            json!({ "name": "Ada", "email": "ADA@example.com", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "short", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "not-an-email", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": " ", "email": "bo@example.com", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "correct horse" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "correct horse", "workspace_name": "X", "invite_token": "t" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];
    for (body, expected) in cases {
        let response = app.post("/api/auth/signup", None, body.clone()).await;
        assert_eq!(response.status, expected, "{body} gave {}", response.body);
        assert!(response.session.is_none());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn signing_up_doesnt_reveal_who_has_an_account(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let signup = async |email: &str, extra: Value| {
        let mut body = json!({ "name": "Someone", "email": email, "password": "correct horse" });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        app.post("/api/auth/signup", None, body).await
    };

    let taken = signup(" Ada@Example.com", json!({ "workspace_name": "X" })).await;
    assert_eq!(taken.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(taken.body["message"], accounts::SIGNUP_REFUSED);
    assert!(taken.session.is_none());
    // With an invite too.
    let invite = app.invite(&ada, &acme, "member").await;
    let joining = signup("ada@example.com", json!({ "invite_token": invite })).await;
    assert_eq!(
        (joining.status, &joining.body),
        (taken.status, &taken.body),
        "the same answer"
    );
    // The same status as any other refusal.
    let short = signup(
        "bo@example.com",
        json!({ "workspace_name": "X", "password": "short" }),
    )
    .await;
    assert_eq!(short.status, taken.status);
    assert!(
        !taken
            .body
            .to_string()
            .to_lowercase()
            .contains("already an account"),
        "{}",
        taken.body
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_addresses_are_not_for_people(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let taken = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Bo", "email": "ada@example.com", "password": "correct horse", "workspace_name": "X" }),
        )
        .await;

    // Forge's own agents, before and after they exist, and added agents.
    let invite = app.invite(&ada, &acme, "member").await;
    for email in [
        format!("github@{acme}.agents.forge.invalid"),
        format!("Forge-AI@{acme}.AGENTS.forge.invalid"),
        format!("agent-{}@agents.forge.invalid", uuid::Uuid::now_v7()),
    ] {
        for extra in [
            json!({ "workspace_name": "Mine" }),
            json!({ "invite_token": invite }),
        ] {
            let mut body =
                json!({ "name": "Mallory", "email": email, "password": "correct horse" });
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let refused = app.post("/api/auth/signup", None, body).await;
            assert_eq!(
                (refused.status, &refused.body),
                (taken.status, &taken.body),
                "{email}: the same answer as a taken email"
            );
            assert!(refused.session.is_none());
        }
    }
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email LIKE '%.invalid'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(people, 0);

    // Someone who got such an address before can't be taken for the agent,
    // nor join a workspace with it.
    let mallory = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $2, 'Mallory')")
        .bind(mallory)
        .bind(format!("forge-ai@{acme}.agents.forge.invalid"))
        .execute(&app.pool)
        .await
        .unwrap();
    let mut db = app.pool.acquire().await.unwrap();
    assert!(
        forge_server::drift::ai_agent(&mut db, acme.parse().unwrap())
            .await
            .is_err()
    );
    drop(db);
    let session = forge_server::auth::start_session(&app.pool, mallory)
        .await
        .unwrap();
    let joined = app
        .post(
            &format!("/api/invites/{invite}/accept"),
            Some(&session),
            json!(null),
        )
        .await;
    assert_eq!(joined.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(joined.body["message"], accounts::SIGNUP_REFUSED);
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_members WHERE user_id = $1")
            .bind(mallory)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(members, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sign_ups_are_limited_per_client(pool: PgPool) {
    let app = App::new(pool);
    let here: std::net::SocketAddr = "203.0.113.7:50000".parse().unwrap();
    let signup = async |client: std::net::SocketAddr, name: &str, password: &str| {
        let email = format!("{}@example.com", name.to_lowercase());
        app.call_from(
            client,
            "POST",
            "/api/auth/signup",
            None,
            Some(json!({ "name": name, "email": email, "password": password, "workspace_name": name })),
        )
        .await
    };

    // Five tries an hour, whether they work or not.
    for (name, password) in [
        ("Ada", "correct horse"),
        ("Bo", "short"),
        ("Ada", "correct horse"),
        ("Cy", "correct horse"),
        ("Di", "short"),
    ] {
        let response = signup(here, name, password).await;
        assert_ne!(response.status, StatusCode::TOO_MANY_REQUESTS, "{name}");
    }
    let sixth = signup(here, "Eve", "correct horse").await;
    assert_eq!(
        sixth.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        sixth.body
    );
    assert_eq!(
        sixth.body["message"],
        "too many sign-ups from here; try again in 60 minutes"
    );
    assert!(sixth.session.is_none());
    let eve: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'eve@example.com'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(eve, 0);

    // Other clients aren't affected, and signing in isn't either.
    let elsewhere = signup(
        "198.51.100.2:50000".parse().unwrap(),
        "Eve",
        "correct horse",
    )
    .await;
    assert_eq!(elsewhere.status, StatusCode::OK, "{}", elsewhere.body);
    let login = app
        .call_from(
            here,
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "ada@example.com", "password": "correct horse" })),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_checks_the_password(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;

    let wrong = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "wrong horse" }),
        )
        .await;
    let unknown = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "bob@example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong.body["message"], unknown.body["message"],
        "don't reveal which emails have accounts"
    );

    let right = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": " Ada@Example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(right.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/me", &right.session.unwrap()).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn too_many_wrong_passwords_lock_the_account_for_a_while(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;
    let login = async |password: &str| {
        app.post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": password }),
        )
        .await
    };
    for _ in 0..10 {
        assert_eq!(login("wrong horse").await.status, StatusCode::UNAUTHORIZED);
    }
    let locked = login("correct horse").await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        locked.body["message"]
            .as_str()
            .unwrap()
            .contains("try again in 15 minutes"),
        "{}",
        locked.body
    );

    // Other accounts are unaffected.
    app.signup("Bob", "Bobco").await;
    let bob = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "bob@example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(bob.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logging_out_ends_the_session(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Acme").await;

    let response = app
        .call("POST", "/api/auth/logout", Some(&session), None)
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT);
    assert_eq!(
        response.session.as_deref(),
        Some(""),
        "the cookie is cleared"
    );
    assert_eq!(
        app.get("/api/me", &session).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_routes_need_membership(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let (grace, _) = app.signup("Grace", "Navy").await;
    let bootstrap = format!("/api/w/{acme}/sync/bootstrap");

    assert_eq!(
        app.call("GET", &bootstrap, None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get(&bootstrap, &grace).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.get("/api/w/not-a-uuid/sync/bootstrap", &ada)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get(&bootstrap, &ada).await.status, StatusCode::OK);

    let team = app.get(&bootstrap, &ada).await.body["records"]["Team"][0]["id"].clone();
    let issue = json!({ "id": uuid::Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": uuid::Uuid::now_v7(), "data": { "team_id": team, "title": "Hello" } }
    ]});
    let transactions = format!("/api/w/{acme}/sync/transactions");
    assert_eq!(
        app.post(&transactions, Some(&grace), issue.clone())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.post(&transactions, Some(&ada), issue).await.status,
        StatusCode::OK
    );
    let records = app.get(&bootstrap, &ada).await.body;
    assert_eq!(records["records"]["Issue"][0]["title"], "Hello");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invite_links_let_people_join(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let token = app.invite(&ada, &acme, "member").await;

    let preview = app
        .call("GET", &format!("/api/invites/{token}"), None, None)
        .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert_eq!(
        preview.body,
        json!({ "workspace_name": "Acme", "workspace_url_key": "acme", "invited_by": "Ada", "role": "member" })
    );

    // A new account joins through the link instead of creating a workspace.
    let bob = app.join("Bob", &token).await;
    let me = app.get("/api/me", &bob).await.body;
    assert_eq!(me["workspaces"].as_array().unwrap().len(), 1);
    assert_eq!(me["workspaces"][0]["id"], acme.as_str());
    assert_eq!(me["workspaces"][0]["role"], "member");

    // An existing account accepts the same link while signed in.
    let (grace, _) = app.signup("Grace", "Navy").await;
    let accepted = app
        .call(
            "POST",
            &format!("/api/invites/{token}/accept"),
            Some(&grace),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(accepted.body["workspaces"].as_array().unwrap().len(), 2);

    let members = app.get(&format!("/api/w/{acme}/members"), &bob).await.body;
    let names: Vec<&str> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["user"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Ada", "Bob", "Grace"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_invite_links_stop_working(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let token = app.invite(&ada, &acme, "member").await;

    let invites = app.get(&format!("/api/w/{acme}/invites"), &ada).await.body;
    assert_eq!(invites.as_array().unwrap().len(), 1);
    let invite_id = invites[0]["id"].as_str().unwrap();
    let revoked = app
        .call(
            "DELETE",
            &format!("/api/w/{acme}/invites/{invite_id}"),
            Some(&ada),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);

    assert_eq!(
        app.call("GET", &format!("/api/invites/{token}"), None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Bob", "email": "bob@example.com", "password": "correct horse", "invite_token": token }),
        )
        .await;
    assert_eq!(signup.status, StatusCode::NOT_FOUND);
    assert!(
        app.get(&format!("/api/w/{acme}/invites"), &ada)
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_manage_people(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let ada_id = app.get("/api/me", &ada).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let invite = app
        .post(
            &format!("/api/w/{acme}/invites"),
            Some(&bob),
            json!({ "role": "member" }),
        )
        .await;
    assert_eq!(invite.status, StatusCode::FORBIDDEN);
    let demote = app
        .call(
            "PATCH",
            &format!("/api/w/{acme}/members/{ada_id}"),
            Some(&bob),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote.status, StatusCode::FORBIDDEN);
    assert_eq!(
        app.get(&format!("/api/w/{acme}/members"), &bob)
            .await
            .status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_workspace_always_keeps_an_admin(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let id_of = async |session: &str| {
        app.get("/api/me", session).await.body["user"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (ada_id, bob_id) = (id_of(&ada).await, id_of(&bob).await);
    let member_path = |id: &str| format!("/api/w/{acme}/members/{id}");

    let demote_self = app
        .call(
            "PATCH",
            &member_path(&ada_id),
            Some(&ada),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote_self.status, StatusCode::CONFLICT);
    assert_eq!(
        app.call("DELETE", &member_path(&ada_id), Some(&ada), None)
            .await
            .status,
        StatusCode::CONFLICT
    );

    let promote = app
        .call(
            "PATCH",
            &member_path(&bob_id),
            Some(&ada),
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(promote.status, StatusCode::NO_CONTENT);
    let demote_self = app
        .call(
            "PATCH",
            &member_path(&ada_id),
            Some(&ada),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote_self.status, StatusCode::NO_CONTENT);

    // Removed people lose access at once.
    assert_eq!(
        app.call("DELETE", &member_path(&ada_id), Some(&bob), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn changing_the_password_signs_out_other_sessions(pool: PgPool) {
    let app = App::new(pool);
    let (here, _) = app.signup("Ada", "Acme").await;
    let elsewhere = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "correct horse" }),
        )
        .await
        .session
        .unwrap();

    let wrong = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&here),
            Some(json!({ "current_password": "nope nope", "new_password": "battery staple" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);

    let changed = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&here),
            Some(json!({ "current_password": "correct horse", "new_password": "battery staple" })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/me", &here).await.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/me", &elsewhere).await.status,
        StatusCode::UNAUTHORIZED
    );

    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "battery staple" }),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reset_password_sets_a_new_one_and_signs_out_everywhere(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Acme").await;

    let password = accounts::reset_password(&app.pool, "ADA@example.com")
        .await
        .unwrap();

    assert_eq!(
        app.get("/api/me", &session).await.status,
        StatusCode::UNAUTHORIZED
    );
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": password }),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
    assert!(
        accounts::reset_password(&app.pool, "nobody@example.com")
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_workspace_starts_with_a_team_and_its_workflow(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme Rockets").await;

    let records = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body["records"]
        .clone();
    assert_eq!(records["Team"][0]["name"], "Acme Rockets");
    assert_eq!(records["Team"][0]["key"], "ACM");
    let states: Vec<&str> = records["WorkflowState"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        states,
        [
            "Triage",
            "Backlog",
            "Todo",
            "In Progress",
            "In Review",
            "Done",
            "Canceled"
        ]
    );
    assert_eq!(records["User"][0]["name"], "Ada");
    assert_eq!(records["User"][0]["role"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admins_create_teams_with_unique_keys(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let teams = format!("/api/w/{acme}/teams");

    // Teams are team settings: members don't make them.
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let by_member = app
        .post(&teams, Some(&bob), json!({ "name": "Mine", "key": "MIN" }))
        .await;
    assert_eq!(
        by_member.status,
        StatusCode::FORBIDDEN,
        "{}",
        by_member.body
    );

    let created = app
        .post(
            &teams,
            Some(&ada),
            json!({ "name": "Design", "key": "DES" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let again = app
        .post(
            &teams,
            Some(&ada),
            json!({ "name": "Other design", "key": "DES" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let bad = app
        .post(&teams, Some(&ada), json!({ "name": "Lower", "key": "low" }))
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);

    let records = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body["records"]
        .clone();
    let keys: Vec<&str> = records["Team"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["ACM", "DES"]);
    assert_eq!(records["WorkflowState"].as_array().unwrap().len(), 14);
}

#[sqlx::test(migrations = "../../migrations")]
async fn removing_someone_unassigns_them_and_tells_clients(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let bob_id = app.get("/api/me", &bob).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bootstrap = format!("/api/w/{acme}/sync/bootstrap");
    let team = app.get(&bootstrap, &ada).await.body["records"]["Team"][0]["id"].clone();

    let issue_id = uuid::Uuid::now_v7();
    let assign = json!({ "id": uuid::Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": issue_id, "data": { "team_id": team, "title": "For Bob", "assignee_id": bob_id } }
    ]});
    let created = app
        .post(
            &format!("/api/w/{acme}/sync/transactions"),
            Some(&ada),
            assign,
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);

    let removed = app
        .call(
            "DELETE",
            &format!("/api/w/{acme}/members/{bob_id}"),
            Some(&ada),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);

    let records = app.get(&bootstrap, &ada).await.body["records"].clone();
    assert_eq!(records["Issue"][0]["assignee_id"], Value::Null);
    assert_eq!(records["User"].as_array().unwrap().len(), 1);
    let log: Vec<(String, String)> =
        sqlx::query_as("SELECT model, action FROM sync_log ORDER BY sync_id DESC LIMIT 3")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        log,
        [
            ("User".into(), "delete".into()),
            ("TeamMember".into(), "delete".into()),
            ("Issue".into(), "update".into())
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn removing_someone_takes_them_off_reviews(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let bob_id = app.get("/api/me", &bob).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bootstrap = format!("/api/w/{acme}/sync/bootstrap");
    let team = app.get(&bootstrap, &ada).await.body["records"]["Team"][0]["id"].clone();
    let ask = json!({ "id": uuid::Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": uuid::Uuid::now_v7(), "data": { "team_id": team, "title": "For Bob to review", "reviewer_id": bob_id } }
    ]});
    let created = app
        .post(&format!("/api/w/{acme}/sync/transactions"), Some(&ada), ask)
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);

    let removed = app
        .call(
            "DELETE",
            &format!("/api/w/{acme}/members/{bob_id}"),
            Some(&ada),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let records = app.get(&bootstrap, &ada).await.body["records"].clone();
    assert_eq!(records["Issue"][0]["reviewer_id"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_server_can_serve_the_web_app(pool: PgPool) {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>Forge</title>",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app.js"), "console.log(1)").unwrap();
    let app = forge_server::app(forge_server::AppState::new(pool, false), Some(dir.path()));

    let get = |path: &'static str| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            (status, String::from_utf8_lossy(&bytes).to_string())
        }
    };
    assert_eq!(
        get("/assets/app.js").await,
        (StatusCode::OK, "console.log(1)".to_string())
    );
    let (status, body) = get("/forge/issue/FOR-1").await;
    assert_eq!(
        (status, body.contains("<title>Forge</title>")),
        (StatusCode::OK, true),
        "app routes get index.html"
    );
    assert_eq!(get("/api/health").await.0, StatusCode::OK);
    let (status, body) = get("/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.contains("no such endpoint"),
        "unknown API paths stay JSON errors"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn issue_history_says_who_changed_what(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let records = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body["records"]
        .clone();
    let team = records["Team"][0]["id"].clone();
    let state = |name: &str| {
        records["WorkflowState"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["id"]
            .clone()
    };
    let issue = uuid::Uuid::now_v7();
    let write = async |who: &str, operation: Value| {
        let tx = json!({ "id": uuid::Uuid::now_v7(), "operations": [operation] });
        let response = app
            .post(&format!("/api/w/{acme}/sync/transactions"), Some(who), tx)
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    };
    write(&ada, json!({ "action": "create", "model": "Issue", "id": issue, "data": { "team_id": team, "title": "Rate limits", "state_id": state("Backlog") } })).await;
    write(&bob, json!({ "action": "update", "model": "Issue", "id": issue, "data": { "state_id": state("In Progress"), "priority": 2, "sort_order": 5.0 } })).await;

    let history = app
        .get(&format!("/api/w/{acme}/issues/{issue}/history"), &bob)
        .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    let events: Vec<(String, Value)> = history
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["field"].as_str().unwrap().to_string(), e["to"].clone()))
        .collect();
    assert_eq!(
        events,
        [
            ("created".into(), Value::Null),
            ("state".into(), state("In Progress")),
            ("priority".into(), json!(2)),
        ]
    );
    let me = app.get("/api/me", &bob).await.body["user"]["id"].clone();
    assert_eq!(history.body[2]["actor_id"], me);

    // Not visible from another workspace.
    let (eve, other) = app.signup("Eve", "Other").await;
    let elsewhere = app
        .get(&format!("/api/w/{other}/issues/{issue}/history"), &eve)
        .await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn spec_history_lists_the_versions_of_its_text(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let spec = uuid::Uuid::now_v7();
    for operation in [
        json!({ "action": "create", "model": "Spec", "id": spec, "data": { "title": "Limits", "body": "One." } }),
        json!({ "action": "update", "model": "Spec", "id": spec, "data": { "title": "Rate limits" } }),
        json!({ "action": "update", "model": "Spec", "id": spec, "data": { "body": "Two." } }),
    ] {
        let tx = json!({ "id": uuid::Uuid::now_v7(), "operations": [operation] });
        let response = app
            .post(&format!("/api/w/{acme}/sync/transactions"), Some(&ada), tx)
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    }

    let history = app
        .get(&format!("/api/w/{acme}/specs/{spec}/history"), &ada)
        .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    let versions: Vec<(Value, Value)> = history
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["body"].clone(), v["created"].clone()))
        .collect();
    assert_eq!(
        versions,
        [(json!("One."), json!(true)), (json!("Two."), json!(false))]
    );

    let (eve, other) = app.signup("Eve", "Other").await;
    let elsewhere = app
        .get(&format!("/api/w/{other}/specs/{spec}/history"), &eve)
        .await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_desktop_app_signs_in_with_a_token_from_its_own_origin(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    let app = App::new(pool);
    app.signup("Ada", "Acme").await;
    let send = async |request: Request<Body>| app.router.clone().oneshot(request).await.unwrap();

    let login = send(
        Request::post("/api/auth/login")
            .header(header::ORIGIN, "tauri://localhost")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forge-client", "desktop")
            .body(Body::from(
                json!({ "email": "ada@example.com", "password": "correct horse" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let headers = login.headers();
    assert!(headers.get(header::SET_COOKIE).is_none(), "no cookie");
    assert_eq!(headers["access-control-allow-origin"], "tauri://localhost");
    assert!(
        headers["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .contains("x-forge-session")
    );
    let token = headers["x-forge-session"].to_str().unwrap().to_string();

    let me = send(
        Request::get("/api/me")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);

    // WebSockets can't carry headers, so the token comes as a subprotocol.
    let socket_style = send(
        Request::get("/api/me")
            .header(
                header::SEC_WEBSOCKET_PROTOCOL,
                format!("forge, bearer.{token}"),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(socket_style.status(), StatusCode::OK);

    let preflight = |origin: &'static str| {
        Request::options("/api/me")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap()
    };
    let allowed = send(preflight("tauri://localhost")).await;
    assert_eq!(
        allowed.headers()["access-control-allow-origin"],
        "tauri://localhost"
    );
    let elsewhere = send(preflight("https://example.com")).await;
    assert!(
        elsewhere
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "other sites can't call the API"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_can_rename_themselves_and_teammates_see_it(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;

    let renamed = app
        .call(
            "PUT",
            "/api/me",
            Some(&bob),
            Some(json!({ "name": "  Robert  " })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["user"]["name"], "Robert");

    let users = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body["records"]["User"]
        .clone();
    let names: Vec<&str> = users
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Robert"), "{names:?}");
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sync_log WHERE model = 'User' AND action = 'update' AND data->>'name' = 'Robert'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(logged, 1, "clients hear about it");

    let empty = app
        .call("PUT", "/api/me", Some(&bob), Some(json!({ "name": " " })))
        .await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../../migrations")]
async fn members_attach_files_that_only_members_can_fetch(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    let data = tempfile::tempdir().unwrap();
    let app = App::with_state(
        forge_server::AppState::new(pool.clone(), false).with_data_dir(data.path()),
    );
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let send = async |request: Request<Body>| app.router.clone().oneshot(request).await.unwrap();
    let upload = async |name: &str, content_type: &str, body: Vec<u8>| {
        send(
            Request::post(format!("/api/w/{acme}/attachments?name={name}"))
                .header(header::COOKIE, format!("forge_session={ada}"))
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    };
    let json = async |response: axum::response::Response| -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    };

    let png = upload("shot.png", "image/png", b"\x89PNG fake".to_vec()).await;
    assert_eq!(png.status(), StatusCode::OK);
    let png = json(png).await;
    assert_eq!(png["inline"], true);
    let url = png["url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("/api/w/{acme}/attachments/")));

    let fetched = send(
        Request::get(&url)
            .header(header::COOKIE, format!("forge_session={ada}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(fetched.status(), StatusCode::OK);
    assert_eq!(fetched.headers()[header::CONTENT_TYPE], "image/png");
    assert!(
        fetched.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    let bytes = axum::body::to_bytes(fetched.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"\x89PNG fake");

    // A page would run on Forge's origin, so it only downloads.
    let page =
        json(upload("x.html", "text/html", b"<script>alert(1)</script>".to_vec()).await).await;
    assert_eq!(page["inline"], false);
    let fetched = send(
        Request::get(page["url"].as_str().unwrap())
            .header(header::COOKIE, format!("forge_session={ada}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        fetched.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert!(
        fetched.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );

    // Not for other workspaces, nor signed-out people.
    let (eve, other) = app.signup("Eve", "Other").await;
    let id = url.rsplit('/').next().unwrap();
    let elsewhere = send(
        Request::get(format!("/api/w/{other}/attachments/{id}"))
            .header(header::COOKIE, format!("forge_session={eve}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(elsewhere.status(), StatusCode::NOT_FOUND);
    let anonymous = send(Request::get(&url).body(Body::empty()).unwrap()).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let huge = upload(
        "big.bin",
        "application/octet-stream",
        vec![0; forge_server::attachments::MAX_SIZE + 1],
    )
    .await;
    assert_eq!(huge.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test(migrations = "../../migrations")]
async fn admins_rename_the_workspace_and_its_url_stays(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let path = format!("/api/w/{acme}");

    let refused = app
        .call("PATCH", &path, Some(&bob), Some(json!({ "name": "Bobco" })))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let renamed = app
        .call(
            "PATCH",
            &path,
            Some(&ada),
            Some(json!({ "name": "Acme Labs" })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    let workspace = &renamed.body["workspaces"][0];
    assert_eq!(workspace["name"], "Acme Labs");
    assert_eq!(workspace["url_key"], "acme", "links keep working");
    assert_eq!(
        app.get("/api/me", &bob).await.body["workspaces"][0]["name"],
        "Acme Labs"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn many_wrong_passwords_at_once_still_hit_the_limit(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;
    let tries = (0..30).map(|_| {
        app.post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "wrong horse" }),
        )
    });
    let statuses: Vec<StatusCode> = futures::future::join_all(tries)
        .await
        .into_iter()
        .map(|r| r.status)
        .collect();
    let checked = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    assert!(checked <= 10, "{checked} passwords were checked");
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS));
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_never_sign_in_nor_become_admins(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let created = app
        .post(
            &format!("/api/w/{acme}/agents"),
            Some(&ada),
            json!({ "name": "Claude Code" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let agent_id = created.body["agent_id"].as_str().unwrap().to_string();
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1::uuid")
        .bind(&agent_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // reset-password gives agents no password ...
    assert!(accounts::reset_password(&app.pool, &email).await.is_err());
    // ... and one that got a password some other way still can't sign in.
    let hash = forge_server::auth::hash_password("correct horse".into())
        .await
        .unwrap();
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2::uuid")
        .bind(hash)
        .bind(&agent_id)
        .execute(&app.pool)
        .await
        .unwrap();
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": email, "password": "correct horse" }),
        )
        .await;
    assert_eq!(login.status, StatusCode::UNAUTHORIZED);

    let promote = app
        .call(
            "PATCH",
            &format!("/api/w/{acme}/members/{agent_id}"),
            Some(&ada),
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(promote.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(promote.body["message"], "agents can't be admins");
}
