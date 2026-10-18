//! Agents and the MCP server: tokens, the tools acting as the agent, and a
//! round trip over HTTP.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::App;
use forge_domain::Actor;
use forge_server::mcp::tools::{self, *};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    agent: Actor,
    /// Ada, who edits specs in the app (agents can't).
    person: Actor,
    token: String,
}

async fn setup(pool: PgPool) -> Setup {
    let app = App::new(pool);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let created = app
        .post(
            &format!("/api/w/{workspace}/agents"),
            Some(&admin),
            json!({ "name": "Claude Code" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let token = created.body["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("forge_"));
    let agent = Actor {
        user_id: created.body["agent_id"].as_str().unwrap().parse().unwrap(),
        workspace_id: workspace.parse().unwrap(),
    };
    let person = Actor {
        user_id: sqlx::query_scalar(
            "SELECT id FROM workspace_users WHERE workspace_id = $1 AND kind = 'person'",
        )
        .bind(agent.workspace_id)
        .fetch_one(&app.pool)
        .await
        .unwrap(),
        workspace_id: agent.workspace_id,
    };
    Setup {
        app,
        admin,
        workspace,
        agent,
        person,
        token,
    }
}

fn parse(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_are_members_with_tokens(pool: PgPool) {
    let s = setup(pool).await;

    let agents = s
        .app
        .get(&format!("/api/w/{}/agents", s.workspace), &s.admin)
        .await
        .body;
    assert_eq!(agents[0]["name"], "Claude Code");
    assert_eq!(agents[0]["tokens"].as_array().unwrap().len(), 1);

    // Forge's own agents are part of Forge: not listed, nor given keys.
    let mut db = s.app.pool.acquire().await.unwrap();
    let github = forge_server::github::github_agent(&mut db, s.workspace.parse().unwrap())
        .await
        .unwrap();
    drop(db);
    let agents = s
        .app
        .get(&format!("/api/w/{}/agents", s.workspace), &s.admin)
        .await
        .body;
    assert_eq!(agents.as_array().unwrap().len(), 1, "{agents}");
    let key = s
        .app
        .post(
            &format!("/api/w/{}/agents/{}/tokens", s.workspace, github.user_id),
            Some(&s.admin),
            json!({}),
        )
        .await;
    assert_eq!(key.status, StatusCode::NOT_FOUND);

    let records = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let kinds: Vec<(&str, &str)> = records["records"]["User"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["name"].as_str().unwrap(), u["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Ada", "person"),
            ("Claude Code", "agent"),
            ("GitHub", "agent")
        ]
    );

    // Agents can't sign in with a password.
    let login = s
        .app
        .post("/api/auth/login", None, json!({ "email": format!("agent-{}@agents.forge.invalid", s.agent.user_id), "password": "" }))
        .await;
    assert_eq!(login.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_handle_agents_keys_and_runners(pool: PgPool) {
    let s = setup(pool).await;
    let base = format!("/api/w/{}", s.workspace);
    let agent = s.agent.user_id;
    let invite = s.app.invite(&s.admin, &s.workspace, "member").await;
    let max = s.app.join("Max", &invite).await;
    let token_id = s.app.get(&format!("{base}/agents"), &s.admin).await.body[0]["tokens"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let runner = json!({ "url": "http://localhost:4100/wake" });

    // Members see the agents, keys and runners, but don't touch them.
    let listed = s.app.get(&format!("{base}/agents"), &max).await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body[0]["tokens"].as_array().unwrap().len(), 1);
    for (method, path, body) in [
        (
            "POST",
            format!("{base}/agents"),
            Some(json!({ "name": "Mine" })),
        ),
        (
            "POST",
            format!("{base}/agents/{agent}/tokens"),
            Some(json!({})),
        ),
        ("DELETE", format!("{base}/tokens/{token_id}"), None),
        (
            "PUT",
            format!("{base}/agents/{agent}/runner"),
            Some(runner.clone()),
        ),
        (
            "POST",
            format!("{base}/agents/{agent}/runner/secret"),
            Some(json!({})),
        ),
        ("DELETE", format!("{base}/agents/{agent}/runner"), None),
    ] {
        let refused = s.app.call(method, &path, Some(&max), body).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    let after = s.app.get(&format!("{base}/agents"), &s.admin).await.body;
    assert_eq!(after.as_array().unwrap().len(), 1, "{after}");
    assert_eq!(after[0]["tokens"].as_array().unwrap().len(), 1, "{after}");
    assert!(after[0]["runner"].is_null());

    // Admins do.
    for (method, path, body) in [
        (
            "POST",
            format!("{base}/agents/{agent}/tokens"),
            Some(json!({})),
        ),
        ("PUT", format!("{base}/agents/{agent}/runner"), Some(runner)),
        (
            "POST",
            format!("{base}/agents/{agent}/runner/secret"),
            Some(json!({})),
        ),
        ("DELETE", format!("{base}/agents/{agent}/runner"), None),
        ("DELETE", format!("{base}/tokens/{token_id}"), None),
    ] {
        let done = s.app.call(method, &path, Some(&s.admin), body).await;
        assert!(done.status.is_success(), "{method} {path}: {}", done.body);
    }
    let (status, _) = rpc(
        &s.app,
        Some(&s.token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the revoked key stopped working"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_and_their_tokens_can_be_told_apart(pool: PgPool) {
    let s = setup(pool).await;
    let base = format!("/api/w/{}", s.workspace);

    // Forge's own AI is part of Forge, not an agent to manage here.
    let mut db = s.app.pool.acquire().await.unwrap();
    let forge_ai = forge_server::drift::ai_agent(&mut db, s.agent.workspace_id)
        .await
        .unwrap();
    drop(db);

    // Each token shows its last four characters, to match it to a config.
    let agents = s.app.get(&format!("{base}/agents"), &s.admin).await.body;
    assert_eq!(agents.as_array().unwrap().len(), 1, "{agents}");
    assert_eq!(agents[0]["tokens"][0]["hint"], s.token[s.token.len() - 4..]);

    // A second agent can't take a name in use, whatever the case.
    let same = s
        .app
        .post(
            &format!("{base}/agents"),
            Some(&s.admin),
            json!({ "name": "claude code" }),
        )
        .await;
    assert_eq!(same.status, StatusCode::CONFLICT, "{}", same.body);
    let laptop = s
        .app
        .post(
            &format!("{base}/agents"),
            Some(&s.admin),
            json!({ "name": "Claude Code (laptop)" }),
        )
        .await;
    assert_eq!(laptop.status, StatusCode::OK, "{}", laptop.body);
    let laptop = laptop.body["agent_id"].as_str().unwrap().to_string();

    let ai_key = s
        .app
        .post(
            &format!("{base}/agents/{}/tokens", forge_ai.user_id),
            Some(&s.admin),
            json!({}),
        )
        .await;
    assert_eq!(ai_key.status, StatusCode::NOT_FOUND);
    let as_ai = s
        .app
        .post(
            &format!("{base}/agents"),
            Some(&s.admin),
            json!({ "name": "Forge AI" }),
        )
        .await;
    assert_eq!(as_ai.status, StatusCode::CONFLICT);

    // Only admins rename, and renaming can't make a duplicate either.
    let invite = s.app.invite(&s.admin, &s.workspace, "member").await;
    let max = s.app.join("Max", &invite).await;
    let rename = |name: &str| json!({ "name": name });
    let path = format!("{base}/agents/{laptop}");
    let by_member = s
        .app
        .call("PATCH", &path, Some(&max), Some(rename("Mine")))
        .await;
    assert_eq!(by_member.status, StatusCode::FORBIDDEN);
    let to_taken = s
        .app
        .call("PATCH", &path, Some(&s.admin), Some(rename("Claude Code")))
        .await;
    assert_eq!(to_taken.status, StatusCode::CONFLICT);
    let renamed = s
        .app
        .call("PATCH", &path, Some(&s.admin), Some(rename("Review bot")))
        .await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT, "{}", renamed.body);
    let users = s
        .app
        .get(&format!("{base}/sync/bootstrap"), &max)
        .await
        .body;
    let names: Vec<&str> = users["records"]["User"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Review bot"), "{names:?}");

    // Removing an agent (as a member) takes its tokens with it.
    let removed = s
        .app
        .call(
            "DELETE",
            &format!("{base}/members/{}", s.agent.user_id),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.body);
    let (status, _) = rpc(
        &s.app,
        Some(&s.token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let agents = s.app.get(&format!("{base}/agents"), &s.admin).await.body;
    assert_eq!(agents.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tools_act_as_the_agent(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;

    let overview = parse(workspace_overview(pool, s.agent).await.unwrap());
    assert_eq!(overview["you"], "Claude Code");
    assert_eq!(overview["teams"][0]["key"], "FOR");

    let created = parse(
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                team: "for".into(),
                title: "Write the MCP server".into(),
                description: Some("Agents need **full CRUD**.".into()),
                status: Some("todo".into()),
                priority: Some("High".into()),
                assignee: Some("me".into()),
                reviewer: None,
                labels: None,
                project: None,
                service: None,
                parent: None,
                due_date: None,
                confirm_new: None,
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(created["created"]["identifier"], "FOR-1");
    assert_eq!(created["created"]["status"], "Todo");
    assert_eq!(created["created"]["assignee"], "Claude Code");

    let creator: Uuid = sqlx::query_scalar("SELECT created_by FROM issues")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(creator, s.agent.user_id, "the agent is the author");

    no_spec_change(pool, s.agent, "FOR-1").await;
    update_issue(
        pool,
        s.agent,
        UpdateIssue {
            identifier: "FOR-1".into(),
            status: Some("In Progress".into()),
            assignee: Some("Ada".into()),
            reviewer: None,
            title: None,
            description: None,
            priority: None,
            labels: None,
            project: None,
            service: None,
            parent: None,
            due_date: None,
        },
    )
    .await
    .unwrap();
    comment_on_issue(
        pool,
        s.agent,
        CommentOnIssue {
            identifier: "FOR-1".into(),
            body: "On it.".into(),
        },
    )
    .await
    .unwrap();

    let found = parse(
        search_issues(
            pool,
            s.agent,
            SearchIssues {
                query: Some("mcp".into()),
                team: None,
                status: None,
                assignee: Some("ada@example.com".into()),
                project: None,
                service: None,
                include_closed: None,
                needs_triage: None,
                limit: None,
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(found["total"], 1);
    assert_eq!(found["issues"][0]["status"], "In Progress");

    let issue = parse(
        get_issue(
            pool,
            s.agent,
            IssueRef {
                identifier: "for-1".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(issue["comments"][0]["author"], "Claude Code");
    assert_eq!(issue["description"], "Agents need **full CRUD**.");
    let history: Vec<String> = issue["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}: {}",
                e["who"].as_str().unwrap(),
                e["change"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        history,
        [
            "Claude Code: created it",
            "Claude Code: moved it from Todo to In Progress",
            "Claude Code: assigned it to Ada",
        ]
    );

    // Ada was assigned by the agent, so she has a notification from it.
    let notified: Uuid =
        sqlx::query_scalar("SELECT actor_id FROM notifications WHERE type = 'assigned'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(notified, s.agent.user_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tools_keep_specs_and_decisions(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;

    save_service(
        pool,
        s.agent,
        SaveService {
            service: None,
            name: Some("Sync engine".into()),
            description: Some("Keeps clients in step.".into()),
            team: Some("FOR".into()),
            repository: Some("github:villain/forge".into()),
            paths: Some(vec!["crates/sync/**".into()]),
            current_state: None,
        },
    )
    .await
    .unwrap();
    save_spec(
        pool,
        s.person,
        SaveSpec {
            spec: None,
            title: Some("Sync protocol".into()),
            body: Some("# Transactions\nAll or nothing.".into()),
            status: Some("active".into()),
            services: Some(vec!["sync".into()]),
            domain: None,
            project: None,
            applying: None,
        },
    )
    .await
    .unwrap();
    save_decision(
        pool,
        s.agent,
        SaveDecision {
            decision_ref: None,
            title: Some("Build our own sync engine".into()),
            context: None,
            decision: Some("Linear-style, in Rust.".into()),
            consequences: None,
            status: None,
            service: Some("Sync engine".into()),
            project: None,
            superseded_by: None,
            decided_on: Some("2026-10-13".into()),
        },
    )
    .await
    .unwrap();
    save_service(
        pool,
        s.agent,
        SaveService {
            service: Some("Sync engine".into()),
            current_state: Some("Applies transactions and serves a change log.".into()),
            name: None,
            description: None,
            team: None,
            repository: None,
            paths: None,
        },
    )
    .await
    .unwrap();

    let service = parse(
        get_service(
            pool,
            s.agent,
            ServiceRef {
                service: "sync engine".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(service["paths"], json!(["crates/sync/**"]));
    assert_eq!(
        service["current_state"],
        "Applies transactions and serves a change log."
    );
    assert!(service["current_state_updated_at"].is_string());
    assert_eq!(
        service["specs"][0]["body"],
        "# Transactions\nAll or nothing."
    );
    assert_eq!(
        service["decisions"][0]["decision"],
        "Linear-style, in Rust."
    );

    // An issue in the service carries the service's context.
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            team: "FOR".into(),
            title: "Partial sync".into(),
            service: Some("Sync engine".into()),
            description: None,
            status: None,
            priority: None,
            assignee: None,
            reviewer: None,
            labels: None,
            project: None,
            parent: None,
            due_date: None,
            confirm_new: None,
        },
    )
    .await
    .unwrap();
    let issue = parse(
        get_issue(
            pool,
            s.agent,
            IssueRef {
                identifier: "FOR-1".into(),
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        issue["service_context"]["current_state"],
        "Applies transactions and serves a change log."
    );
    assert_eq!(issue["specs"][0]["title"], "Sync protocol");
    assert_eq!(issue["specs"][0]["via"], "service");
}

#[sqlx::test(migrations = "../../migrations")]
async fn save_spec_points_out_where_a_behaviour_spec_breaks_the_convention(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let body = |requirement: &str| {
        format!("## Purpose\n\nRate limits.\n\n## Requirements\n\n{requirement}")
    };

    // Prose isn't a behaviour spec: no warnings.
    let mut input = spec("Overview", "active", None, None);
    input.body = Some("# Overview\n\nWhat it is.".into());
    let prose = save_spec(pool, s.person, input).await.unwrap();
    assert!(!prose.contains("convention"), "{prose}");

    // Problems come back as warnings, naming the requirement; it's saved anyway.
    let mut input = spec("Rate limits", "draft", None, None);
    input.body = Some(body(
        "### Requirement: Tokens are limited\nThe API limits tokens.\n",
    ));
    let created = save_spec(pool, s.person, input).await.unwrap();
    assert!(created.starts_with("Created Spec"), "{created}");
    assert!(
        created.contains("- Requirement \"Tokens are limited\" doesn't say SHALL or MUST."),
        "{created}"
    );
    assert!(
        created.contains("- Requirement \"Tokens are limited\" has no scenario"),
        "{created}"
    );
    let saved = get_spec(
        pool,
        s.agent,
        SpecRef {
            spec: "Rate limits".into(),
        },
    )
    .await
    .unwrap();
    assert!(saved.contains("The API limits tokens."), "{saved}");

    // Changing only the title checks the body as it stands.
    let renamed = save_spec(
        pool,
        s.person,
        SaveSpec {
            spec: Some("Rate limits".into()),
            title: Some("Token limits".into()),
            body: None,
            status: None,
            services: None,
            domain: None,
            project: None,
            applying: None,
        },
    )
    .await
    .unwrap();
    assert!(renamed.contains("has no scenario"), "{renamed}");

    // Fixed: no warnings.
    let fixed = save_spec(
        pool,
        s.person,
        SaveSpec {
            spec: Some("Token limits".into()),
            title: None,
            body: Some(body(
                "### Requirement: Tokens are limited\nThe API SHALL limit tokens.\n\n#### Scenario: Too many\n- **WHEN** a token sends too many\n- **THEN** it gets 429\n",
            )),
            status: None,
            services: None,
            domain: None,
            project: None,
            applying: None,
        },
    )
    .await
    .unwrap();
    assert!(
        fixed.starts_with("Updated Spec") && !fixed.contains("convention"),
        "{fixed}"
    );
}

fn spec(title: &str, status: &str, service: Option<&str>, project: Option<&str>) -> SaveSpec {
    SaveSpec {
        spec: None,
        title: Some(title.into()),
        body: None,
        status: Some(status.into()),
        services: service.map(|sv| vec![sv.into()]),
        domain: None,
        project: project.map(Into::into),
        applying: None,
    }
}

fn decision(title: &str, status: &str, service: Option<&str>, decided_on: &str) -> SaveDecision {
    SaveDecision {
        decision_ref: None,
        title: Some(title.into()),
        context: None,
        decision: None,
        consequences: None,
        status: Some(status.into()),
        service: service.map(Into::into),
        project: None,
        superseded_by: None,
        decided_on: Some(decided_on.into()),
    }
}

/// `get_issue` gives the same specs and decisions as the issue page: the
/// service's, the project's and the workspace-wide ones still in force.
#[sqlx::test(migrations = "../../migrations")]
async fn get_issue_includes_project_and_workspace_knowledge(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;

    for name in ["Sync engine", "Web app"] {
        save_service(
            pool,
            s.agent,
            SaveService {
                service: None,
                name: Some(name.into()),
                description: None,
                team: Some("FOR".into()),
                repository: None,
                paths: None,
                current_state: None,
            },
        )
        .await
        .unwrap();
    }
    save_project(
        pool,
        s.agent,
        SaveProject {
            project: None,
            name: Some("Milestone 1".into()),
            description: None,
            status: None,
            lead: None,
            initiative: None,
            target_date: None,
        },
    )
    .await
    .unwrap();

    for input in [
        spec("Sync protocol", "active", Some("Sync engine"), None),
        spec("Milestone scope", "draft", None, Some("Milestone 1")),
        spec("Old protocol", "deprecated", Some("Sync engine"), None),
        spec("Theme", "active", Some("Web app"), None),
        spec("Glossary", "active", None, None),
    ] {
        save_spec(pool, s.person, input).await.unwrap();
    }
    for input in [
        decision(
            "Own sync engine",
            "accepted",
            Some("Sync engine"),
            "2026-10-01",
        ),
        decision("Committed work only", "accepted", None, "2026-10-14"),
        decision("Specs in Forge", "proposed", None, "2026-10-14"),
        decision("Specs in the repo", "overridden", None, "2026-09-01"),
        decision("Tailwind", "accepted", Some("Web app"), "2026-10-02"),
        SaveDecision {
            project: Some("Milestone 1".into()),
            ..decision("Ship the tracker first", "accepted", None, "2026-10-12")
        },
    ] {
        save_decision(pool, s.agent, input).await.unwrap();
    }

    create_issue(
        pool,
        s.agent,
        CreateIssue {
            team: "FOR".into(),
            title: "Partial sync".into(),
            service: Some("Sync engine".into()),
            project: Some("Milestone 1".into()),
            description: None,
            status: None,
            priority: None,
            assignee: None,
            reviewer: None,
            labels: None,
            parent: None,
            due_date: None,
            confirm_new: None,
        },
    )
    .await
    .unwrap();
    let issue = parse(
        get_issue(
            pool,
            s.agent,
            IssueRef {
                identifier: "FOR-1".into(),
            },
        )
        .await
        .unwrap(),
    );
    let titles_and_via = |key: &str| -> Vec<(String, String)> {
        issue[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let text = |f: &str| r[f].as_str().unwrap().to_string();
                (text("title"), text("via"))
            })
            .collect()
    };
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    // A spec with neither service nor project isn't about this issue's work.
    assert_eq!(
        titles_and_via("specs"),
        pairs(&[("Milestone scope", "project"), ("Sync protocol", "service")])
    );
    assert_eq!(
        titles_and_via("decisions"),
        pairs(&[
            ("Committed work only", "workspace"),
            ("Specs in Forge", "workspace"),
            ("Ship the tracker first", "project"),
            ("Own sync engine", "service"),
        ])
    );
}

/// An agent can put a decision in a project, see which project a decision
/// belongs to, and take it out again.
#[sqlx::test(migrations = "../../migrations")]
async fn decisions_can_belong_to_a_project(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;

    save_project(
        pool,
        s.agent,
        SaveProject {
            project: None,
            name: Some("Milestone 1".into()),
            description: None,
            status: None,
            lead: None,
            initiative: None,
            target_date: None,
        },
    )
    .await
    .unwrap();
    save_decision(
        pool,
        s.agent,
        SaveDecision {
            project: Some("milestone 1".into()),
            ..decision("Ship the tracker first", "accepted", None, "2026-10-12")
        },
    )
    .await
    .unwrap();
    save_decision(
        pool,
        s.agent,
        decision("Committed work only", "accepted", None, "2026-10-14"),
    )
    .await
    .unwrap();

    let projects = || async {
        let list = parse(
            list_decisions(pool, s.agent, ListKnowledge { service: None })
                .await
                .unwrap(),
        );
        let mut found: Vec<(String, Value)> = list["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["title"].as_str().unwrap().to_string(),
                    d["project"].clone(),
                )
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    };
    assert_eq!(
        projects().await,
        vec![
            ("Committed work only".to_string(), Value::Null),
            ("Ship the tracker first".to_string(), json!("Milestone 1")),
        ]
    );
    let get = || async {
        parse(
            get_decision(
                pool,
                s.agent,
                DecisionRef {
                    decision: "Ship the tracker first".into(),
                },
            )
            .await
            .unwrap(),
        )
    };
    assert_eq!(get().await["project"], "Milestone 1");

    save_decision(
        pool,
        s.agent,
        SaveDecision {
            decision_ref: Some("Ship the tracker first".into()),
            title: None,
            context: None,
            decision: None,
            consequences: None,
            status: None,
            service: None,
            project: Some("none".into()),
            superseded_by: None,
            decided_on: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(get().await["project"], Value::Null);

    let error = save_decision(
        pool,
        s.agent,
        SaveDecision {
            project: Some("Milestone 9".into()),
            ..decision("Nowhere", "accepted", None, "2026-10-14")
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("no project"), "{error}");
}

/// `list_specs` says which project each spec belongs to, and `get_project`
/// lists the project's decisions as well as its specs.
#[sqlx::test(migrations = "../../migrations")]
async fn project_knowledge_shows_in_lists(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;

    save_project(
        pool,
        s.agent,
        SaveProject {
            project: None,
            name: Some("Milestone 1".into()),
            description: None,
            status: None,
            lead: None,
            initiative: None,
            target_date: None,
        },
    )
    .await
    .unwrap();
    for input in [
        spec("Milestone scope", "active", None, Some("Milestone 1")),
        spec("Loose notes", "draft", None, None),
    ] {
        save_spec(pool, s.person, input).await.unwrap();
    }
    for input in [
        SaveDecision {
            project: Some("Milestone 1".into()),
            ..decision("Ship the tracker first", "accepted", None, "2026-10-12")
        },
        decision("Committed work only", "accepted", None, "2026-10-14"),
    ] {
        save_decision(pool, s.agent, input).await.unwrap();
    }

    let list = parse(
        list_specs(pool, s.agent, ListKnowledge { service: None })
            .await
            .unwrap(),
    );
    let mut found: Vec<(String, Value)> = list["specs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sp| {
            (
                sp["title"].as_str().unwrap().to_string(),
                sp["project"].clone(),
            )
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        found,
        vec![
            ("Loose notes".to_string(), Value::Null),
            ("Milestone scope".to_string(), json!("Milestone 1")),
        ]
    );

    let project = parse(
        get_project(
            pool,
            s.agent,
            ProjectRef {
                project: "Milestone 1".into(),
            },
        )
        .await
        .unwrap(),
    );
    let titles = |key: &str| -> Vec<String> {
        project[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["title"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(titles("specs"), vec!["Milestone scope"]);
    assert_eq!(titles("decisions"), vec!["Ship the tracker first"]);
    assert_eq!(project["decisions"][0]["status"], "accepted");
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_propose_descriptions_and_people_decide(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            team: "FOR".into(),
            title: "Ask before removing".into(),
            description: Some("Remove asks first.".into()),
            status: None,
            priority: None,
            assignee: Some("me".into()),
            reviewer: None,
            labels: None,
            project: None,
            service: None,
            parent: None,
            due_date: None,
            confirm_new: None,
        },
    )
    .await
    .unwrap();
    let propose = |description: &str, why: &str| ProposeDescription {
        identifier: "FOR-1".into(),
        description: description.into(),
        why: why.into(),
    };

    let error = propose_description(pool, s.agent, propose("Remove asks first.", "Same"))
        .await
        .unwrap_err();
    assert!(error.contains("already"), "{error}");
    let error = propose_description(pool, s.agent, propose("Revoke too.", " "))
        .await
        .unwrap_err();
    assert!(error.contains("why"), "{error}");

    propose_description(
        pool,
        s.agent,
        propose("Remove and revoke ask.", "Revoke too."),
    )
    .await
    .unwrap();
    propose_description(
        pool,
        s.agent,
        propose("Remove, revoke and delete ask first.", "Delete too."),
    )
    .await
    .unwrap();
    let suggestions: Vec<(Uuid, String, String, Uuid)> =
        sqlx::query_as("SELECT id, kind, status, created_by FROM suggestions ORDER BY created_at")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(suggestions.len(), 2);
    assert_eq!(
        suggestions[0].2, "dismissed",
        "the newer proposal replaces it"
    );
    let (id, kind, status, by) = &suggestions[1];
    assert_eq!((kind.as_str(), status.as_str()), ("description", "open"));
    assert_eq!(*by, s.agent.user_id);

    // Nobody but the agent is on the issue, so the admins hear about it.
    let notified: Vec<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM notifications WHERE suggestion_id = $1")
            .bind(id)
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(notified.len(), 1);
    assert_ne!(notified[0], s.agent.user_id);

    let description = || async {
        sqlx::query_scalar::<_, String>("SELECT description FROM issues")
            .fetch_one(pool)
            .await
            .unwrap()
    };
    assert_eq!(
        description().await,
        "Remove asks first.",
        "not until accepted"
    );

    let accepted = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/suggestions/{id}/accept", s.workspace),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::NO_CONTENT, "{}", accepted.body);
    assert_eq!(description().await, "Remove, revoke and delete ask first.");
    let changed_by: Uuid = sqlx::query_scalar(
        "SELECT actor_id FROM sync_log WHERE model = 'Issue' ORDER BY sync_id DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        changed_by, notified[0],
        "the change is the accepting admin's"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn tools_explain_what_went_wrong(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let bad_team = CreateIssue {
        team: "NOPE".into(),
        title: "x".into(),
        description: None,
        status: None,
        priority: None,
        assignee: None,
        reviewer: None,
        labels: None,
        project: None,
        service: None,
        parent: None,
        due_date: None,
        confirm_new: None,
    };
    let error = create_issue(pool, s.agent, bad_team).await.unwrap_err();
    assert!(error.contains("teams: FOR"), "{error}");

    let error = tools::get_issue(
        pool,
        s.agent,
        IssueRef {
            identifier: "FOR-99".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("no issue"), "{error}");
}

/// Sends one JSON-RPC message to `/mcp` and returns the response message.
async fn rpc(app: &App, token: Option<&str>, message: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::from(message.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes);
    // JSON, or a server-sent event stream with one message.
    let body = serde_json::from_str(&text).unwrap_or_else(|_| {
        text.lines()
            .rev()
            .filter_map(|line| line.strip_prefix("data:"))
            .find_map(|data| serde_json::from_str(data.trim()).ok())
            .unwrap_or(Value::String(text.to_string()))
    });
    (status, body)
}

fn new_issue(title: &str, status: &str, confirm_new: Option<bool>) -> CreateIssue {
    CreateIssue {
        team: "FOR".into(),
        title: title.into(),
        status: Some(status.into()),
        confirm_new,
        description: None,
        priority: None,
        assignee: None,
        reviewer: None,
        labels: None,
        project: None,
        service: None,
        parent: None,
        due_date: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_delete_only_issues_they_created(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    // FOR-1 by Ada, FOR-2 by the agent.
    create_issue(pool, s.person, new_issue("Ada's", "Todo", Some(true)))
        .await
        .unwrap();
    create_issue(pool, s.agent, new_issue("The agent's", "Todo", Some(true)))
        .await
        .unwrap();

    let refused = delete_issue(pool, s.agent, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    assert_eq!(
        refused,
        "Only admins and whoever created an issue can delete it."
    );
    delete_issue(pool, s.agent, issue_ref("FOR-2"))
        .await
        .unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT title FROM issues")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(left, ["Ada's"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_see_similar_open_issues_before_filing(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let count = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM issues")
            .fetch_one(pool)
            .await
            .unwrap()
    };
    create_issue(
        pool,
        s.agent,
        new_issue(
            "A test sometimes fails, turning pull requests red at random",
            "Todo",
            None,
        ),
    )
    .await
    .unwrap();

    // The same flaky test again, in other words (FOR-39 and FOR-40): nothing
    // is created, and the agent sees the open issue that may cover it.
    let duplicate = "The draft pull request test fails now and then";
    let error = create_issue(pool, s.agent, new_issue(duplicate, "Triage", None))
        .await
        .unwrap_err();
    assert!(error.contains("Not created yet"), "{error}");
    assert!(
        error.contains(
            "FOR-1: A test sometimes fails, turning pull requests red at random (Todo, unassigned)"
        ),
        "{error}"
    );
    assert!(error.contains("confirm_new: true"), "{error}");
    assert_eq!(count().await, 1);

    // Saying it's really new files it.
    let created = parse(
        create_issue(pool, s.agent, new_issue(duplicate, "Triage", Some(true)))
            .await
            .unwrap(),
    );
    assert_eq!(created["created"]["identifier"], "FOR-2");

    // Titles about something else, or sharing a single word, go straight in.
    for title in [
        "Tidy the docs",
        "Write the docs",
        "Make the test suite quicker",
    ] {
        create_issue(pool, s.agent, new_issue(title, "Todo", None))
            .await
            .unwrap_or_else(|e| panic!("{title}: {e}"));
    }

    // Closed issues don't count: once both flaky-test issues are done, the
    // next one is filed without asking.
    for issue in ["FOR-1", "FOR-2"] {
        update_issue(
            pool,
            s.agent,
            UpdateIssue {
                identifier: issue.into(),
                status: Some("Done".into()),
                title: None,
                description: None,
                priority: None,
                assignee: None,
                reviewer: None,
                labels: None,
                project: None,
                service: None,
                parent: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
    }
    create_issue(pool, s.agent, new_issue(duplicate, "Todo", None))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn mcp_over_http_needs_a_valid_token(pool: PgPool) {
    let s = setup(pool).await;
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });

    let (status, _) = rpc(&s.app, None, list.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = rpc(&s.app, Some("forge_not-a-real-token"), list.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": { "name": "test", "version": "1" },
    }});
    let (status, body) = rpc(&s.app, Some(&s.token), init).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["serverInfo"]["name"], "forge");

    let (status, body) = rpc(&s.app, Some(&s.token), list).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names: Vec<&str> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"get_issue") && names.contains(&"save_decision"),
        "{names:?}"
    );

    let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "create_issue",
        "arguments": { "team": "FOR", "title": "Made over MCP" },
    }});
    let (status, body) = rpc(&s.app, Some(&s.token), call).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("FOR-1"), "{text}");

    // Revoked tokens stop working.
    let agents = s
        .app
        .get(&format!("/api/w/{}/agents", s.workspace), &s.admin)
        .await
        .body;
    let token_id = agents[0]["tokens"][0]["id"].as_str().unwrap();
    let revoked = s
        .app
        .call(
            "DELETE",
            &format!("/api/w/{}/tokens/{token_id}", s.workspace),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let (status, _) = rpc(
        &s.app,
        Some(&s.token),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// The agent's work as clients see it: issue, and whether it's still alive.
async fn working_on(pool: &PgPool, agent: Actor) -> Option<String> {
    sqlx::query_scalar(
        "SELECT t.key || '-' || i.number FROM workspace_users u \
         JOIN issues i ON i.id = u.working_on_id JOIN teams t ON t.id = i.team_id \
         WHERE u.id = $1 AND u.workspace_id = $2",
    )
    .bind(agent.user_id)
    .bind(agent.workspace_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn move_issue(
    pool: &PgPool,
    actor: Actor,
    identifier: &str,
    status: Option<&str>,
    assignee: Option<&str>,
) {
    update_issue(
        pool,
        actor,
        UpdateIssue {
            identifier: identifier.into(),
            status: status.map(Into::into),
            assignee: assignee.map(Into::into),
            reviewer: None,
            title: None,
            description: None,
            priority: None,
            labels: None,
            project: None,
            service: None,
            parent: None,
            due_date: None,
        },
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_say_what_they_are_working_on(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for title in ["First", "Second"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                team: "FOR".into(),
                title: title.into(),
                status: Some("Todo".into()),
                assignee: Some("me".into()),
                reviewer: None,
                description: None,
                priority: None,
                labels: None,
                project: None,
                service: None,
                parent: None,
                due_date: None,
                confirm_new: None,
            },
        )
        .await
        .unwrap();
    }
    no_spec_change(pool, s.agent, "FOR-1").await;
    no_spec_change(pool, s.agent, "FOR-2").await;
    // Starting shows on the agent's record, which clients sync.
    let started = start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    assert!(started.contains("FOR-1"), "{started}");
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    let users = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let record = users["records"]["User"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["kind"] == "agent")
        .unwrap()
        .clone();
    assert!(record["working_since"].is_string(), "{record}");
    assert!(record["working_seen_at"].is_string(), "{record}");

    // Finishing one it isn't on changes nothing.
    let not_on = finish_work(pool, s.agent, finishing("FOR-2"))
        .await
        .unwrap();
    assert!(not_on.contains("weren't"), "{not_on}");
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    finish_work(pool, s.agent, finishing("FOR-1"))
        .await
        .unwrap();
    assert_eq!(working_on(pool, s.agent).await, None);

    // Work that changes no code is handed over with the No code mark.
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    let marked = finish_work(
        pool,
        s.agent,
        FinishWork {
            identifier: "FOR-1".into(),
            no_code: Some(true),
        },
    )
    .await
    .unwrap();
    assert!(marked.contains("no code"), "{marked}");
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert_eq!(issue["no_code"], true, "{issue}");
    assert!(
        issue["history"]
            .to_string()
            .contains("marked it as needing no code"),
        "{issue}"
    );

    // Moving it along keeps the work going; into review ends it.
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    move_issue(pool, s.agent, "FOR-1", Some("In Progress"), None).await;
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    move_issue(pool, s.agent, "FOR-1", Some("In Review"), None).await;
    assert_eq!(working_on(pool, s.agent).await, None);

    // So does handing it to someone else, or anyone moving it out of the
    // started statuses (Triage, for a decision).
    start_work(pool, s.agent, issue_ref("FOR-2")).await.unwrap();
    move_issue(pool, s.agent, "FOR-2", None, Some("Ada")).await;
    assert_eq!(working_on(pool, s.agent).await, None);
    start_work(pool, s.agent, issue_ref("FOR-2")).await.unwrap();
    move_issue(pool, s.agent, "FOR-2", Some("Triage"), None).await;
    assert_eq!(working_on(pool, s.agent).await, None);

    // And deleting it.
    start_work(pool, s.agent, issue_ref("FOR-2")).await.unwrap();
    delete_issue(pool, s.agent, issue_ref("FOR-2"))
        .await
        .unwrap();
    assert_eq!(working_on(pool, s.agent).await, None);

    // Each change reached the log clients follow.
    let logged: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sync_log WHERE model = 'User' AND model_id = $1")
            .bind(s.agent.user_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(logged >= 10, "{logged}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_work_is_not_done_twice(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let created = s
        .app
        .post(
            &format!("/api/w/{}/agents", s.workspace),
            Some(&s.admin),
            json!({ "name": "Claude Code (chat)" }),
        )
        .await;
    let chat = Actor {
        user_id: created.body["agent_id"].as_str().unwrap().parse().unwrap(),
        workspace_id: s.agent.workspace_id,
    };
    for title in ["First", "Second"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                team: "FOR".into(),
                title: title.into(),
                status: Some("Todo".into()),
                assignee: Some("me".into()),
                reviewer: None,
                description: None,
                priority: None,
                labels: None,
                project: None,
                service: None,
                parent: None,
                due_date: None,
                confirm_new: None,
            },
        )
        .await
        .unwrap();
    }

    for identifier in ["FOR-1", "FOR-2"] {
        no_spec_change(pool, s.agent, identifier).await;
        move_issue(pool, s.agent, identifier, Some("In Progress"), None).await;
    }

    // One issue per run: still on one, it can't start another, and the
    // first stays as it was. Starting the same one again is fine.
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    let busy = start_work(pool, s.agent, issue_ref("FOR-2"))
        .await
        .unwrap_err();
    assert!(busy.contains("still working on FOR-1"), "{busy}");
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();

    // One worker per issue: the refusal says who has it.
    let taken = start_work(pool, chat, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    assert!(
        taken.contains("Claude Code is already working on FOR-1"),
        "{taken}"
    );
    assert_eq!(working_on(pool, chat).await, None);

    // Once the other finishes, it's free.
    start_work(pool, chat, issue_ref("FOR-2")).await.unwrap();
    finish_work(pool, chat, finishing("FOR-2")).await.unwrap();
    start_work(pool, chat, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    finish_work(pool, s.agent, finishing("FOR-1"))
        .await
        .unwrap();
    start_work(pool, chat, issue_ref("FOR-1")).await.unwrap();

    // And after an hour without a sign of life, a run counts as stopped:
    // its work holds neither the issue nor the agent.
    start_work(pool, s.agent, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    sqlx::query(
        "UPDATE workspace_members SET working_seen_at = now() - interval '61 minutes' \
         WHERE user_id = $1",
    )
    .bind(chat.user_id)
    .execute(pool)
    .await
    .unwrap();
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    start_work(pool, chat, issue_ref("FOR-2")).await.unwrap();
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    assert_eq!(working_on(pool, chat).await.as_deref(), Some("FOR-2"));
}

fn finishing(identifier: &str) -> FinishWork {
    FinishWork {
        identifier: identifier.into(),
        no_code: None,
    }
}

fn issue_ref(identifier: &str) -> IssueRef {
    IssueRef {
        identifier: identifier.into(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn any_call_shows_the_agent_is_still_working(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "start_work",
        "arguments": { "identifier": "FOR-1" },
    }});
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            team: "FOR".into(),
            title: "Long build".into(),
            status: None,
            assignee: Some("me".into()),
            reviewer: None,
            description: None,
            priority: None,
            labels: None,
            project: None,
            service: None,
            parent: None,
            due_date: None,
            confirm_new: None,
        },
    )
    .await
    .unwrap();
    no_spec_change(pool, s.agent, "FOR-1").await;
    let (status, body) = rpc(&s.app, Some(&s.token), call.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["isError"], false, "{body}");

    let set_seen = async |minutes: i32| {
        sqlx::query(
            "UPDATE workspace_members SET working_seen_at = now() - make_interval(mins => $1), \
             working_since = now() - make_interval(mins => $1) WHERE user_id = $2",
        )
        .bind(minutes)
        .bind(s.agent.user_id)
        .execute(pool)
        .await
        .unwrap();
    };
    let minutes_since = async |field: &str| -> f64 {
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM now() - {field})::float8 / 60 FROM workspace_members WHERE user_id = $1"
        );
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(s.agent.user_id)
            .fetch_one(pool)
            .await
            .unwrap()
    };

    // A call during the work renews it.
    set_seen(30).await;
    rpc(&s.app, Some(&s.token), list.clone()).await;
    assert!(minutes_since("working_seen_at").await < 1.0);
    assert!(
        minutes_since("working_since").await > 29.0,
        "still the same start"
    );

    // After an hour without one, the run has stopped, and a call doesn't
    // bring it back; starting again does, from now.
    set_seen(61).await;
    rpc(&s.app, Some(&s.token), list).await;
    assert!(minutes_since("working_seen_at").await > 60.0);
    assert_eq!(working_on(pool, s.agent).await.as_deref(), Some("FOR-1"));
    rpc(&s.app, Some(&s.token), call).await;
    assert!(minutes_since("working_seen_at").await < 1.0);
    assert!(minutes_since("working_since").await < 1.0);
}

fn update(identifier: &str) -> UpdateIssue {
    UpdateIssue {
        identifier: identifier.into(),
        status: None,
        assignee: None,
        reviewer: None,
        title: None,
        description: None,
        priority: None,
        labels: None,
        project: None,
        service: None,
        parent: None,
        due_date: None,
    }
}

/// Writes that the issue changes no spec, so the agent may start it.
async fn no_spec_change(pool: &PgPool, actor: Actor, identifier: &str) {
    save_spec_change(
        pool,
        actor,
        SaveSpecChange {
            no_spec_change: Some("Only a test.".into()),
            ..spec_change(identifier)
        },
    )
    .await
    .unwrap();
}

fn spec_change(identifier: &str) -> SaveSpecChange {
    SaveSpecChange {
        identifier: identifier.into(),
        spec: None,
        new_spec: None,
        service: None,
        requirements: None,
        body: None,
        drop: None,
        replaces: None,
        no_spec_change: None,
    }
}

const RATE_LIMITS: &str = "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429
";

/// Decision "Specs change only when the work is done" (FOR-71): agents write
/// an issue's spec change instead of editing specs, and apply it only once
/// the issue is done.
#[sqlx::test(migrations = "../../migrations")]
async fn agents_write_spec_changes_instead_of_editing_specs(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for (title, body) in [
        ("Rate limits", RATE_LIMITS),
        ("Overview", "# Overview\n\nWhat it is."),
    ] {
        let mut input = spec(title, "active", None, None);
        input.body = Some(body.into());
        save_spec(pool, s.person, input).await.unwrap();
    }
    save_decision(
        pool,
        s.person,
        decision("Old limits", "accepted", None, "2026-10-01"),
    )
    .await
    .unwrap();
    create_issue(pool, s.agent, new_issue("Raise the limit", "todo", None))
        .await
        .unwrap();

    // Agents can't edit a spec directly.
    let edit = SaveSpec {
        spec: Some("Rate limits".into()),
        title: None,
        body: Some(RATE_LIMITS.replace("100 requests", "200 requests")),
        status: None,
        services: None,
        domain: None,
        project: None,
        applying: None,
    };
    let refused = save_spec(pool, s.agent, edit).await.unwrap_err();
    assert!(refused.contains("save_spec_change"), "{refused}");

    // They write the change on the issue, in OpenSpec's delta format.
    let saved = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            spec: Some("Rate limits".into()),
            requirements: Some(
                "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow 200 requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends a 201st request\n- **THEN** it gets 429\n\n## ADDED Requirements\n\n### Requirement: Limits reset\nThe limit SHALL reset every minute.\n".into(),
            ),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    assert!(saved.contains("changes \"Rate limits\""), "{saved}");
    assert!(
        saved.contains("- Requirement \"Limits reset\" has no scenario"),
        "{saved}"
    );

    // A change has to fit the spec as it is.
    for (requirements, problem) in [
        (
            "## MODIFIED Requirements\n### Requirement: Missing\nIt SHALL.\n",
            "\"Rate limits\" has no requirement \"Missing\" to change.",
        ),
        (
            "## ADDED Requirements\n### Requirement: Tokens are limited\nIt SHALL.\n",
            "\"Rate limits\" already has a requirement \"Tokens are limited\"",
        ),
        ("## Purpose\nOther.", "isn't a delta section"),
    ] {
        let error = save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some("Rate limits".into()),
                requirements: Some(requirements.into()),
                ..spec_change("FOR-1")
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains(problem), "{error}");
    }
    let prose = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            spec: Some("Overview".into()),
            requirements: Some("## ADDED Requirements\n### Requirement: A\nIt SHALL.\n".into()),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap_err();
    assert!(prose.contains("rewrite it whole"), "{prose}");

    // A new spec is written whole; replaced decisions are named.
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            new_spec: Some("Quotas".into()),
            body: Some("## Purpose\n\nQuotas.".into()),
            replaces: Some(vec!["Old limits".into()]),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    let both = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            no_spec_change: Some("Nothing changes.".into()),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap_err();
    assert!(both.contains("changes specs"), "{both}");

    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    let change = &issue["spec_change"];
    assert_eq!(change["specs"][0]["spec"], "Rate limits");
    assert!(
        change["specs"][0]["requirements"]
            .as_str()
            .unwrap()
            .starts_with("## MODIFIED Requirements")
    );
    assert_eq!(change["specs"][1]["new_spec"], "Quotas");
    assert_eq!(change["specs"][1]["body"], "## Purpose\n\nQuotas.");
    assert_eq!(change["replaces"], json!(["Old limits"]));
    assert_eq!(change["no_spec_change"], Value::Null);

    // Dropping a spec takes only its part out.
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            new_spec: Some("Quotas".into()),
            drop: Some(true),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert_eq!(issue["spec_change"]["specs"].as_array().unwrap().len(), 1);

    // Applying it waits until the issue is done.
    let applying = |spec: &str| SaveSpec {
        spec: Some(spec.into()),
        title: None,
        body: Some(RATE_LIMITS.replace("100 requests", "200 requests")),
        status: None,
        services: None,
        domain: None,
        project: None,
        applying: Some("FOR-1".into()),
    };
    let early = save_spec(pool, s.agent, applying("Rate limits"))
        .await
        .unwrap_err();
    assert!(early.contains("isn't done"), "{early}");
    move_issue(pool, s.person, "FOR-1", Some("Done"), None).await;

    // Done: Forge has applied it (FOR-74), and the spec's history says so.
    let applied = parse(
        get_spec(
            pool,
            s.agent,
            SpecRef {
                spec: "Rate limits".into(),
            },
        )
        .await
        .unwrap(),
    );
    let body = applied["body"].as_str().unwrap();
    assert!(body.contains("200 requests a minute"), "{body}");
    assert!(body.contains("### Requirement: Limits reset"), "{body}");
    assert_eq!(applied["history"][0]["issue"], "FOR-1");
    assert_eq!(
        applied["history"][0]["change"],
        "added \"Limits reset\"; changed \"Tokens are limited\""
    );
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert!(issue["spec_change"]["applied_at"].is_string(), "{issue}");
    assert_eq!(issue["spec_change"]["not_applied"], json!([]));

    let closed = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            replaces: Some(vec![]),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap_err();
    assert!(closed.contains("is closed"), "{closed}");
    save_spec(pool, s.agent, applying("Rate limits"))
        .await
        .unwrap();
    let other = save_spec(pool, s.agent, applying("Overview"))
        .await
        .unwrap_err();
    assert!(other.contains("doesn't change this spec"), "{other}");
    let applied = get_spec(
        pool,
        s.agent,
        SpecRef {
            spec: "Rate limits".into(),
        },
    )
    .await
    .unwrap();
    assert!(applied.contains("200 requests"), "{applied}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_change_follows_what_it_names(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    save_decision(
        pool,
        s.person,
        decision("Old limits", "accepted", None, "2026-10-01"),
    )
    .await
    .unwrap();
    for title in ["Fix a typo", "Raise the limit"] {
        create_issue(pool, s.agent, new_issue(title, "todo", Some(true)))
            .await
            .unwrap();
    }

    // Saying nothing changes, then changing a spec after all.
    let none = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            no_spec_change: Some("Only a typo in a message.".into()),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    assert!(none.contains("says no spec changes"), "{none}");
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert_eq!(
        issue["spec_change"]["no_spec_change"],
        "Only a typo in a message."
    );
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-2")).await.unwrap());
    assert_eq!(issue["spec_change"], Value::Null, "not written yet");

    let rewrite = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            spec: Some("Rate limits".into()),
            body: Some(RATE_LIMITS.replace("100", "200")),
            replaces: Some(vec!["Old limits".into()]),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    assert!(
        rewrite.contains("no longer says no spec changes"),
        "{rewrite}"
    );

    // Deleting what it names takes it out of the change; deleting the
    // issue deletes the change.
    let delete = async |model: forge_domain::ModelKind, sql: &'static str| {
        let id: Uuid = sqlx::query_scalar(sql).fetch_one(pool).await.unwrap();
        let transaction = forge_domain::Transaction {
            id: Uuid::now_v7(),
            operations: vec![forge_domain::Operation::Delete { model, id }],
        };
        forge_sync::apply_transaction(pool, s.person, &transaction)
            .await
            .unwrap();
    };
    delete(
        forge_domain::ModelKind::Decision,
        "SELECT id FROM decisions WHERE title = 'Old limits'",
    )
    .await;
    delete(
        forge_domain::ModelKind::Spec,
        "SELECT id FROM specs WHERE title = 'Rate limits'",
    )
    .await;
    let (specs, decisions): (Value, Vec<Uuid>) =
        sqlx::query_as("SELECT specs, replaces_decision_ids FROM spec_changes")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(specs, json!([]));
    assert!(decisions.is_empty());
    delete(
        forge_domain::ModelKind::Issue,
        "SELECT id FROM issues WHERE number = 1",
    )
    .await;
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM spec_changes")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

fn search_assigned_to(assignee: &str) -> SearchIssues {
    SearchIssues {
        query: None,
        team: None,
        status: None,
        assignee: Some(assignee.into()),
        project: None,
        service: None,
        include_closed: None,
        needs_triage: None,
        limit: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_take_their_issues_in_queue_order(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for (title, status, priority) in [
        ("Low", "Todo", "low"),
        ("Urgent", "Todo", "urgent"),
        ("No priority", "Todo", "none"),
        ("Started", "In Progress", "medium"),
        ("Waiting for review", "In Review", "urgent"),
        ("Someday", "Backlog", "urgent"),
    ] {
        // Work starts once its spec change is written.
        let started = matches!(status, "In Progress" | "In Review");
        let created = create_issue(
            pool,
            s.agent,
            CreateIssue {
                priority: Some(priority.into()),
                assignee: Some("me".into()),
                reviewer: None,
                ..new_issue(title, if started { "Todo" } else { status }, Some(true))
            },
        )
        .await
        .unwrap();
        if started {
            let identifier = parse(created)["created"]["identifier"]
                .as_str()
                .unwrap()
                .to_string();
            no_spec_change(pool, s.agent, &identifier).await;
            move_issue(pool, s.agent, &identifier, Some(status), None).await;
        }
    }
    // The queue first, by priority. What waits for review or isn't planned
    // yet has no place in it.
    let found = parse(
        search_issues(pool, s.agent, search_assigned_to("me"))
            .await
            .unwrap(),
    );
    let order: Vec<(&str, Option<u64>)> = found["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["title"].as_str().unwrap(), i["queue_position"].as_u64()))
        .collect();
    assert_eq!(
        order,
        [
            ("Urgent", Some(1)),
            ("Started", Some(2)),
            ("Low", Some(3)),
            ("No priority", Some(4)),
            ("Waiting for review", None),
            ("Someday", None),
        ]
    );

    // The issue itself says where it is in the queue.
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-4")).await.unwrap());
    assert_eq!(issue["title"], "Started");
    assert_eq!(issue["queue_position"], 2);

    // People have no queue: their issues have no place in one.
    move_issue(pool, s.agent, "FOR-1", None, Some("Ada")).await;
    let ada = parse(
        search_issues(pool, s.agent, search_assigned_to("Ada"))
            .await
            .unwrap(),
    );
    assert_eq!(ada["issues"][0]["title"], "Low");
    assert!(ada["issues"][0].get("queue_position").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_has_a_domain(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let save = |domain: &str| SaveSpec {
        spec: Some("Invite links".into()),
        title: None,
        body: None,
        status: None,
        services: None,
        domain: Some(domain.into()),
        project: None,
        applying: None,
    };
    save_spec(pool, s.person, spec("Invite links", "active", None, None))
        .await
        .unwrap();
    // Domains are made in the app.
    let transaction = forge_domain::Transaction {
        id: Uuid::now_v7(),
        operations: vec![forge_domain::Operation::Create {
            model: forge_domain::ModelKind::Domain,
            id: Uuid::now_v7(),
            data: serde_json::from_value(json!({ "name": "Accounts" })).unwrap(),
        }],
    };
    forge_sync::apply_transaction(pool, s.person, &transaction)
        .await
        .unwrap();
    assert!(save_spec(pool, s.person, save("Billing")).await.is_err());

    save_spec(pool, s.person, save(" Accounts ")).await.unwrap();
    let read = |text: String| serde_json::from_str::<Value>(&text).unwrap();
    let spec_ref = || SpecRef {
        spec: "Invite links".into(),
    };
    let saved = read(get_spec(pool, s.person, spec_ref()).await.unwrap());
    assert_eq!(saved["domain"], "Accounts");
    let listed = read(
        list_specs(pool, s.person, ListKnowledge { service: None })
            .await
            .unwrap(),
    );
    assert_eq!(listed["specs"][0]["domain"], "Accounts");

    save_spec(pool, s.person, save("none")).await.unwrap();
    let cleared = read(get_spec(pool, s.person, spec_ref()).await.unwrap());
    assert_eq!(cleared["domain"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn superseding_a_decision_marks_it_superseded(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for title in ["D1: 100 a minute", "D2: 200 a minute"] {
        save_decision(
            pool,
            s.agent,
            decision(title, "accepted", None, "2026-10-01"),
        )
        .await
        .unwrap();
    }
    save_decision(
        pool,
        s.agent,
        SaveDecision {
            decision_ref: Some("D1: 100 a minute".into()),
            title: None,
            status: None,
            decided_on: None,
            superseded_by: Some("D2: 200 a minute".into()),
            ..decision("", "", None, "")
        },
    )
    .await
    .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM decisions WHERE title = $1")
        .bind("D1: 100 a minute")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "superseded", "as when it's set in the app");
}

#[sqlx::test(migrations = "../../migrations")]
async fn work_starts_only_once_the_spec_change_is_written(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for title in ["First", "Second"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("me".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }

    // Neither starting nor moving it into a started status, until then.
    let refused = start_work(pool, s.agent, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    assert!(refused.contains("save_spec_change"), "{refused}");
    for status in ["In Progress", "In Review"] {
        let moved = update_issue(
            pool,
            s.agent,
            UpdateIssue {
                status: Some(status.into()),
                ..update("FOR-1")
            },
        )
        .await
        .unwrap_err();
        assert!(moved.contains("no spec change yet"), "{moved}");
    }
    let created = create_issue(pool, s.agent, new_issue("Third", "In Progress", Some(true)))
        .await
        .unwrap_err();
    assert!(created.contains("save_spec_change"), "{created}");
    // Only naming decisions it replaces isn't a spec change yet.
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            replaces: Some(vec![]),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    assert!(start_work(pool, s.agent, issue_ref("FOR-1")).await.is_err());
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert_eq!(issue["spec_change"]["written"], false);

    no_spec_change(pool, s.agent, "FOR-1").await;
    let issue = parse(get_issue(pool, s.agent, issue_ref("FOR-1")).await.unwrap());
    assert_eq!(issue["spec_change"]["written"], true);
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
    move_issue(pool, s.agent, "FOR-1", Some("In Progress"), None).await;

    // While it works on it, the spec change is its to write.
    let created = s
        .app
        .post(
            &format!("/api/w/{}/agents", s.workspace),
            Some(&s.admin),
            json!({ "name": "Claude Code (chat)" }),
        )
        .await;
    let chat = Actor {
        user_id: created.body["agent_id"].as_str().unwrap().parse().unwrap(),
        workspace_id: s.agent.workspace_id,
    };
    let theirs = save_spec_change(
        pool,
        chat,
        SaveSpecChange {
            no_spec_change: Some("Mine now.".into()),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap_err();
    assert!(
        theirs.contains("Claude Code is working on FOR-1"),
        "{theirs}"
    );

    // People aren't held back: they can't write spec changes in the app yet.
    let issue: Uuid =
        sqlx::query_scalar("SELECT id FROM issues WHERE number = 2 AND workspace_id = $1")
            .bind(s.person.workspace_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let state: Uuid = sqlx::query_scalar(
        "SELECT s.id FROM workflow_states s JOIN issues i ON i.team_id = s.team_id \
         WHERE i.id = $1 AND s.name = 'In Progress'",
    )
    .bind(issue)
    .fetch_one(pool)
    .await
    .unwrap();
    let moved = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            json!({ "id": Uuid::now_v7(), "operations": [
                { "action": "update", "model": "Issue", "id": issue, "data": { "state_id": state } }
            ] }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
}

/// Two open issues changing the same requirement: the one that wrote it
/// second can't start until it's settled, and the first one's people hear
/// about it. Once the first is done, the second can't overwrite it.
#[sqlx::test(migrations = "../../migrations")]
async fn only_one_issue_changes_a_requirement_at_a_time(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    for title in ["Allow 200", "Allow 300", "Bursts"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("Ada".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }
    let modifies = |n: &str| {
        format!(
            "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow {n} requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends one more within a minute\n- **THEN** it gets 429\n"
        )
    };
    let write = async |identifier: &str, requirements: String| {
        save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some("Rate limits".into()),
                requirements: Some(requirements),
                ..spec_change(identifier)
            },
        )
        .await
        .unwrap()
    };
    let first = write("FOR-1", modifies("200")).await;
    assert!(!first.contains("can't start"), "{first}");
    let second = write("FOR-2", modifies("300")).await;
    assert!(second.contains("FOR-2 can't start yet"), "{second}");
    assert!(second.contains("FOR-1 (Ada) already changes"), "{second}");
    // Adding another requirement doesn't get in anyone's way.
    write(
        "FOR-3",
        "## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts of 10.\n\n#### Scenario: Burst\n- **WHEN** a token sends 10 at once\n- **THEN** all pass\n".into(),
    )
    .await;
    // Two issues writing a new spec of the same name: only one at a time.
    let new_spec = |identifier: &'static str| SaveSpecChange {
        new_spec: Some("Quotas".into()),
        body: Some("## Purpose\n\nQuotas.".into()),
        ..spec_change(identifier)
    };
    save_spec_change(pool, s.agent, new_spec("FOR-1"))
        .await
        .unwrap();
    let taken = save_spec_change(pool, s.agent, new_spec("FOR-3"))
        .await
        .unwrap_err();
    assert!(
        taken.contains("FOR-1's spec change writes a new spec"),
        "{taken}"
    );
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            new_spec: Some("Quotas".into()),
            drop: Some(true),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();

    // Ada, assigned to FOR-1, hears about FOR-2.
    let heard: Vec<(String, i32)> = sqlx::query_as(
        "SELECT n.type, i.number FROM notifications n JOIN issues i ON i.id = n.issue_id \
         WHERE n.user_id = $1 AND n.type = 'spec_conflict'",
    )
    .bind(s.person.user_id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(heard, [("spec_conflict".to_string(), 2)]);

    let standing = |identifier: &'static str| async move {
        parse(
            get_issue(pool, s.agent, issue_ref(identifier))
                .await
                .unwrap(),
        )["spec_change"]
            .clone()
    };
    assert_eq!(standing("FOR-1").await["held_back"], json!([]));
    assert!(
        standing("FOR-1").await["also_wanted"][0]
            .as_str()
            .unwrap()
            .contains("FOR-2 (Ada) also wants to change \"Tokens are limited\"")
    );
    assert_eq!(standing("FOR-3").await["held_back"], json!([]));
    let refused = start_work(pool, s.agent, issue_ref("FOR-2"))
        .await
        .unwrap_err();
    assert!(refused.contains("Settle it"), "{refused}");
    let moved = update_issue(
        pool,
        s.agent,
        UpdateIssue {
            status: Some("In Progress".into()),
            ..update("FOR-2")
        },
    )
    .await
    .unwrap_err();
    assert!(moved.contains("FOR-1 (Ada) already changes"), "{moved}");
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();

    // FOR-1 is done: FOR-2 no longer conflicts, but it was written against
    // the text FOR-1 changed, so it would undo it.
    move_issue(pool, s.person, "FOR-1", Some("Done"), None).await;
    let held = standing("FOR-2").await["held_back"].clone();
    assert!(held[0].as_str().unwrap().contains("older text"), "{held}");
    assert!(start_work(pool, s.agent, issue_ref("FOR-2")).await.is_err());

    // Done anyway, by a person: Forge leaves that part out rather than undo FOR-1.
    move_issue(pool, s.person, "FOR-2", Some("Done"), None).await;
    let body: String = sqlx::query_scalar("SELECT body FROM specs WHERE title = 'Rate limits'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(body.contains("allow 200 requests"), "{body}");
    let not_applied = standing("FOR-2").await["not_applied"].clone();
    assert!(
        not_applied[0]["why"]
            .as_str()
            .unwrap()
            .contains("\"Tokens are limited\" changed since this was written"),
        "{not_applied}"
    );
}

/// Two issues never hold each other back: the write that would make them
/// is refused. A new spec is written by one open issue at a time, even when
/// one of them is reopened.
#[sqlx::test(migrations = "../../migrations")]
async fn issues_never_hold_each_other_back(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    for title in ["Rate limits", "Search limits"] {
        let mut input = spec(title, "active", None, None);
        input.body = Some(RATE_LIMITS.into());
        save_spec(pool, s.person, input).await.unwrap();
    }
    for title in ["Allow 200", "Allow 300"] {
        create_issue(pool, s.agent, new_issue(title, "Todo", Some(true)))
            .await
            .unwrap();
    }
    let modifies = "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow 200 requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends one more within a minute\n- **THEN** it gets 429\n";
    let write = async |identifier: &str, spec: &str| {
        save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some(spec.into()),
                requirements: Some(modifies.into()),
                ..spec_change(identifier)
            },
        )
        .await
    };
    // FOR-1 is first on Rate limits, FOR-2 on Search limits; FOR-1 then
    // comes second on Search limits.
    write("FOR-1", "Rate limits").await.unwrap();
    write("FOR-2", "Search limits").await.unwrap();
    write("FOR-1", "Search limits").await.unwrap();
    // FOR-2 coming second on Rate limits would leave neither able to start.
    let looped = write("FOR-2", "Rate limits").await.unwrap_err();
    assert!(
        looped.contains("FOR-2 would come second to FOR-1"),
        "{looped}"
    );
    start_work(pool, s.agent, issue_ref("FOR-2")).await.unwrap();

    // FOR-1 writes a new spec, is canceled, and FOR-2 writes it meanwhile:
    // reopened, FOR-1 comes after FOR-2 for it.
    finish_work(pool, s.agent, finishing("FOR-2"))
        .await
        .unwrap();
    let quotas = |identifier: &'static str| SaveSpecChange {
        new_spec: Some("Quotas".into()),
        body: Some("## Purpose\n\nQuotas.".into()),
        ..spec_change(identifier)
    };
    save_spec_change(pool, s.agent, quotas("FOR-1"))
        .await
        .unwrap();
    move_issue(pool, s.person, "FOR-1", Some("Canceled"), None).await;
    save_spec_change(pool, s.agent, quotas("FOR-2"))
        .await
        .unwrap();
    move_issue(pool, s.person, "FOR-1", Some("Todo"), None).await;
    let held = |identifier: &'static str| async move {
        parse(
            get_issue(pool, s.agent, issue_ref(identifier))
                .await
                .unwrap(),
        )["spec_change"]["held_back"]
            .clone()
    };
    let first = held("FOR-1").await;
    assert!(
        first.as_array().unwrap().iter().any(|h| h
            .as_str()
            .unwrap()
            .contains("FOR-2 already writes a new spec \"Quotas\"")),
        "{first}"
    );
    assert_eq!(held("FOR-2").await, json!([]));
    // FOR-2, there first, can still write its new spec again.
    save_spec_change(pool, s.agent, quotas("FOR-2"))
        .await
        .unwrap();
    // FOR-2 is done: the spec exists now, so FOR-1's new one can't be created.
    move_issue(pool, s.person, "FOR-2", Some("Done"), None).await;
    let first = held("FOR-1").await;
    assert!(
        first[0]
            .as_str()
            .unwrap()
            .contains("There's a spec called \"Quotas\" now"),
        "{first}"
    );
    assert!(start_work(pool, s.agent, issue_ref("FOR-1")).await.is_err());
}

/// A spec change changed after work on its issue started shows as such,
/// next to what it said then.
#[sqlx::test(migrations = "../../migrations")]
async fn a_spec_change_changed_after_work_started_shows(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    for title in ["Bursts", "Late"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("me".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }
    let flag = |identifier: &'static str| async move {
        parse(
            get_issue(pool, s.agent, issue_ref(identifier))
                .await
                .unwrap(),
        )["spec_change"]["changed_after_start"]
            .clone()
    };
    no_spec_change(pool, s.agent, "FOR-1").await;
    move_issue(pool, s.agent, "FOR-1", Some("In Progress"), None).await;
    assert_eq!(flag("FOR-1").await, json!(false));

    let saved = save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            spec: Some("Rate limits".into()),
            requirements: Some("## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts of 10.\n\n#### Scenario: Burst\n- **WHEN** a token sends 10 at once\n- **THEN** all pass\n".into()),
            no_spec_change: None,
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    assert!(saved.contains("Say why in a comment"), "{saved}");
    assert_eq!(flag("FOR-1").await, json!(true));

    // Back to Todo and started again: what it says then is the new start.
    move_issue(pool, s.agent, "FOR-1", Some("Todo"), None).await;
    move_issue(pool, s.agent, "FOR-1", Some("In Progress"), None).await;
    assert_eq!(flag("FOR-1").await, json!(false));

    // Also by way of Canceled, edited while not started.
    move_issue(pool, s.person, "FOR-1", Some("Canceled"), None).await;
    move_issue(pool, s.person, "FOR-1", Some("Todo"), None).await;
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            spec: Some("Rate limits".into()),
            drop: Some(true),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    no_spec_change(pool, s.agent, "FOR-1").await;
    move_issue(pool, s.agent, "FOR-1", Some("In Progress"), None).await;
    assert_eq!(flag("FOR-1").await, json!(false));

    // Started by a person without one: written afterwards, it shows.
    move_issue(pool, s.person, "FOR-2", Some("In Progress"), None).await;
    no_spec_change(pool, s.agent, "FOR-2").await;
    assert_eq!(flag("FOR-2").await, json!(true));
}

/// A spec change part widened later to a requirement another issue already
/// changes comes after that issue for it, however old the part is.
#[sqlx::test(migrations = "../../migrations")]
async fn widening_a_part_later_never_jumps_the_queue(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    for title in ["Bursts", "Allow 200"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("Ada".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }
    let bursts = "## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts of 10.\n\n#### Scenario: Burst\n- **WHEN** a token sends 10 at once\n- **THEN** all pass\n";
    let limit = "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow 200 requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends a 201st request within a minute\n- **THEN** it gets 429\n";
    let write = async |identifier: &str, requirements: String| {
        save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some("Rate limits".into()),
                requirements: Some(requirements),
                ..spec_change(identifier)
            },
        )
        .await
        .unwrap()
    };
    write("FOR-1", bursts.into()).await;
    write("FOR-2", limit.into()).await;
    let widened = write(
        "FOR-1",
        format!(
            "{bursts}
{limit}"
        ),
    )
    .await;
    assert!(widened.contains("FOR-1 can't start yet"), "{widened}");
    assert!(widened.contains("FOR-2 (Ada) already changes"), "{widened}");
    let held = |identifier: &'static str| async move {
        parse(
            get_issue(pool, s.agent, issue_ref(identifier))
                .await
                .unwrap(),
        )["spec_change"]["held_back"]
            .clone()
    };
    assert_eq!(held("FOR-2").await, json!([]));
}

/// Turning a part into a rewrite touches the whole spec from then on: it
/// comes after issues that already change something in it.
#[sqlx::test(migrations = "../../migrations")]
async fn becoming_a_rewrite_later_never_jumps_the_queue(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    for title in ["Bursts", "Allow 200"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("Ada".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }
    let save = async |identifier: &str, requirements: Option<String>, body: Option<String>| {
        save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some("Rate limits".into()),
                requirements,
                body,
                ..spec_change(identifier)
            },
        )
        .await
        .unwrap()
    };
    save("FOR-1", Some("## ADDED Requirements\n\n### Requirement: Bursts\nThe API SHALL allow bursts of 10.\n\n#### Scenario: Burst\n- **WHEN** a token sends 10 at once\n- **THEN** all pass\n".into()), None).await;
    save("FOR-2", Some("## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow 200 requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends a 201st request within a minute\n- **THEN** it gets 429\n".into()), None).await;
    let rewritten = save(
        "FOR-1",
        None,
        Some(RATE_LIMITS.replace("Rate limits.", "Limits per token.")),
    )
    .await;
    assert!(rewritten.contains("FOR-1 can't start yet"), "{rewritten}");
    let held = parse(get_issue(pool, s.agent, issue_ref("FOR-2")).await.unwrap())["spec_change"]
        ["held_back"]
        .clone();
    assert_eq!(held, json!([]));
}

/// An issue reopened from Canceled joins the queue anew, behind issues that
/// changed the same requirement while it was canceled.
#[sqlx::test(migrations = "../../migrations")]
async fn a_reopened_issue_joins_the_queue_anew(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let mut input = spec("Rate limits", "active", None, None);
    input.body = Some(RATE_LIMITS.into());
    save_spec(pool, s.person, input).await.unwrap();
    for title in ["Allow 200", "Allow 300"] {
        create_issue(
            pool,
            s.agent,
            CreateIssue {
                assignee: Some("Ada".into()),
                reviewer: None,
                ..new_issue(title, "Todo", Some(true))
            },
        )
        .await
        .unwrap();
    }
    let modifies = |n: &str| {
        format!(
            "## MODIFIED Requirements\n\n### Requirement: Tokens are limited\nThe API SHALL allow {n} requests a minute per token.\n\n#### Scenario: Too many\n- **WHEN** a token sends one more within a minute\n- **THEN** it gets 429\n"
        )
    };
    let write = async |identifier: &str, requirements: String| {
        save_spec_change(
            pool,
            s.agent,
            SaveSpecChange {
                spec: Some("Rate limits".into()),
                requirements: Some(requirements),
                ..spec_change(identifier)
            },
        )
        .await
        .unwrap()
    };
    write("FOR-1", modifies("200")).await;
    move_issue(pool, s.person, "FOR-1", Some("Canceled"), None).await;
    write("FOR-2", modifies("300")).await;
    start_work(pool, s.agent, issue_ref("FOR-2")).await.unwrap();

    // Not straight from Canceled into work: reopened first.
    let refused = start_work(pool, s.agent, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    assert!(refused.contains("is closed"), "{refused}");
    let moved = update_issue(
        pool,
        s.agent,
        UpdateIssue {
            status: Some("In Progress".into()),
            ..update("FOR-1")
        },
    )
    .await
    .unwrap_err();
    assert!(moved.contains("is closed"), "{moved}");

    move_issue(pool, s.person, "FOR-1", Some("Todo"), None).await;
    let held = |identifier: &'static str| async move {
        parse(
            get_issue(pool, s.agent, issue_ref(identifier))
                .await
                .unwrap(),
        )["spec_change"]["held_back"]
            .clone()
    };
    assert_eq!(held("FOR-2").await, json!([]));
    let first = held("FOR-1").await;
    assert!(
        first[0]
            .as_str()
            .unwrap()
            .contains("FOR-2 (Ada) already changes"),
        "{first}"
    );
}

/// A reopened issue whose spec change took effect can be started again: it
/// can't change any more, so there's nothing left to write.
#[sqlx::test(migrations = "../../migrations")]
async fn a_reopened_issue_whose_spec_change_took_effect_can_start(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            assignee: Some("me".into()),
            reviewer: None,
            ..new_issue("Replace a decision", "Todo", Some(true))
        },
    )
    .await
    .unwrap();
    // Only the decisions it replaces: not written yet, as far as starting goes.
    save_spec_change(
        pool,
        s.agent,
        SaveSpecChange {
            replaces: Some(vec![]),
            ..spec_change("FOR-1")
        },
    )
    .await
    .unwrap();
    move_issue(pool, s.person, "FOR-1", Some("Done"), None).await;
    move_issue(pool, s.person, "FOR-1", Some("Todo"), None).await;
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
}

/// A done issue without a spec change, picked up again: reopened first,
/// where its spec change can be written.
#[sqlx::test(migrations = "../../migrations")]
async fn a_done_issue_is_reopened_before_it_starts_again(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            assignee: Some("me".into()),
            reviewer: None,
            ..new_issue("Old work", "Todo", Some(true))
        },
    )
    .await
    .unwrap();
    move_issue(pool, s.person, "FOR-1", Some("Done"), None).await;
    let refused = start_work(pool, s.agent, issue_ref("FOR-1"))
        .await
        .unwrap_err();
    assert!(refused.contains("Reopen it to Todo"), "{refused}");
    move_issue(pool, s.agent, "FOR-1", Some("Todo"), None).await;
    no_spec_change(pool, s.agent, "FOR-1").await;
    start_work(pool, s.agent, issue_ref("FOR-1")).await.unwrap();
}
