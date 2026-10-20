//! Drift checks agents run themselves and report through MCP (spec "Drift
//! checks from agents").

mod common;

use axum::http::StatusCode;
use common::App;
use forge_domain::Actor;
use forge_server::AppState;
use forge_server::mcp::drift_checks::*;
use forge_server::mcp::tools::*;
use serde_json::{Value, json};
use sqlx::PgPool;

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    agent: Actor,
    /// A second agent, to show a check is its own agent's.
    other: Actor,
}

async fn add_agent(app: &App, admin: &str, workspace: &str, name: &str) -> Actor {
    let created = app
        .post(
            &format!("/api/w/{workspace}/agents"),
            Some(admin),
            json!({ "name": name }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    Actor {
        user_id: created.body["agent_id"].as_str().unwrap().parse().unwrap(),
        workspace_id: workspace.parse().unwrap(),
    }
}

const SPEC: &str = "## Purpose\n\nTokens.\n\n## Requirements\n\n\
### Requirement: Tokens expire\nA token SHALL expire after a year.\n\n\
#### Scenario: Old token\n- **WHEN** a token is a year old\n- **THEN** it's refused\n\n\
### Requirement: Tokens are shown once\nA token SHALL be shown once.\n\n\
#### Scenario: Shown\n- **WHEN** an agent is added\n- **THEN** its token is shown once\n\n\
### Requirement: Agent keys\nAn agent SHALL have a key.\n\n\
#### Scenario: Key\n- **WHEN** an agent is added\n- **THEN** it has a key\n";

async fn setup(pool: PgPool) -> Setup {
    let app = App::new(pool);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let agent = add_agent(&app, &admin, &workspace, "Drift check").await;
    let other = add_agent(&app, &admin, &workspace, "Claude Code").await;
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
    save_service(
        &app.pool,
        person,
        SaveService {
            service: None,
            name: Some("MCP server".into()),
            description: None,
            team: Some("FOR".into()),
            repository: None,
            paths: None,
            current_state: None,
        },
    )
    .await
    .unwrap();
    save_spec(
        &app.pool,
        person,
        SaveSpec {
            spec: None,
            title: Some("Agents' tokens".into()),
            body: Some(SPEC.into()),
            status: Some("active".into()),
            services: Some(vec!["MCP server".into()]),
            domain: None,
            project: None,
            applying: None,
        },
    )
    .await
    .unwrap();
    Setup {
        app,
        admin,
        workspace,
        agent,
        other,
    }
}

fn planned(requirement: &str) -> PlannedRequirement {
    PlannedRequirement {
        spec: "Agents' tokens".into(),
        requirement: requirement.into(),
    }
}

fn start(key: &str, services: &[&str]) -> StartDriftCheck {
    StartDriftCheck {
        key: key.into(),
        commit_from: Some("73f0be5".into()),
        commit_to: "c02dcc0".into(),
        services: services.iter().map(|s| s.to_string()).collect(),
        checker: "vault-drift-check v2".into(),
        full_audit: false,
        plan: Some(vec![
            planned("Tokens expire"),
            planned("Tokens are shown once"),
            planned("Agent keys"),
        ]),
    }
}

fn verdict(requirement: &str, verdict: &str, why: &str) -> ReportedVerdict {
    ReportedVerdict {
        spec: "Agents' tokens".into(),
        requirement: requirement.into(),
        verdict: verdict.into(),
        why: why.into(),
        issue: None,
    }
}

fn report(key: &str, verdicts: Vec<ReportedVerdict>) -> ReportDriftCheck {
    ReportDriftCheck {
        key: key.into(),
        verdicts,
        plan: None,
    }
}

fn finish(key: &str, outcome: &str, usage: Option<Vec<ReportedUsage>>) -> FinishDriftCheck {
    FinishDriftCheck {
        key: key.into(),
        outcome: outcome.into(),
        summary: "Checked the tokens.".into(),
        error: (outcome == "failed").then(|| "a worker crashed".into()),
        usage,
    }
}

async fn run(pool: &PgPool, key: &str) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(r) FROM ai_runs r WHERE run_key = $1")
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_report_their_checks(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    let key = "73f0be5..c02dcc0";

    let unknown = start_drift_check(pool, s.agent, start(key, &["Billing"]))
        .await
        .unwrap_err();
    assert!(unknown.contains("nothing started"), "{unknown}");
    assert!(unknown.contains("MCP server"), "{unknown}");

    // A retry finds the check it started.
    let started = start_drift_check(pool, s.agent, start(key, &["MCP server"]))
        .await
        .unwrap();
    assert!(started.contains("planning 3 requirements"), "{started}");
    let again = start_drift_check(pool, s.agent, start(key, &["MCP server"]))
        .await
        .unwrap();
    assert!(again.contains("already started"), "{again}");
    let checks: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_runs WHERE run_key = $1")
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(checks, 1);
    let check = run(pool, key).await;
    assert_eq!(check["trigger"], "agent");
    assert_eq!(check["status"], "running");
    assert_eq!(check["tally"]["unjudged"], 3);

    // A requirement that doesn't exist records nothing of the report.
    let wrong = report_drift_check(
        pool,
        s.agent,
        report(
            key,
            vec![
                verdict("Tokens expire", "met", "`tokens.rs:12`"),
                verdict("Tokens rotate", "met", "-"),
            ],
        ),
    )
    .await
    .unwrap_err();
    assert!(wrong.contains("nothing recorded"), "{wrong}");
    assert!(wrong.contains("\"Agent keys\""), "{wrong}");
    assert_eq!(run(pool, key).await["tally"]["met"], 0);

    let uncovered = report_drift_check(
        pool,
        s.agent,
        report(key, vec![verdict("Agent keys", "covered", "FOR has it")]),
    )
    .await
    .unwrap_err();
    assert!(uncovered.contains("name the issue"), "{uncovered}");

    let refused = report_drift_check(
        pool,
        s.other,
        report(key, vec![verdict("Agent keys", "met", "-")]),
    )
    .await
    .unwrap_err();
    assert_eq!(
        refused,
        "only the agent that started this check can report into it"
    );

    // Reported twice, it counts once, as the last verdict.
    report_drift_check(
        pool,
        s.agent,
        report(key, vec![verdict("Agent keys", "unclear", "Can't tell.")]),
    )
    .await
    .unwrap();
    let reported = report_drift_check(
        pool,
        s.agent,
        report(
            key,
            vec![
                verdict("Agent keys", "met", "`agents.rs:40`"),
                verdict("Tokens expire", "skipped", "Not in these commits."),
            ],
        ),
    )
    .await
    .unwrap();
    assert!(
        reported.contains("1 met, 1 skipped, 1 planned without a verdict"),
        "{reported}"
    );

    // Met reaches the spec page; skipped doesn't.
    let latest: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT requirement, verdict, commit_sha FROM requirement_verdicts ORDER BY requirement",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        latest,
        vec![("Agent keys".into(), "met".into(), Some("c02dcc0".into()))]
    );

    // Forge sees the issue the agent files meanwhile.
    create_issue(
        pool,
        s.agent,
        CreateIssue {
            team: "FOR".into(),
            title: "Spec drift: \"Agents' tokens\" no longer matches the code".into(),
            status: Some("Triage".into()),
            confirm_new: Some(true),
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

    let usage = vec![
        ReportedUsage {
            name: "Orchestrator".into(),
            calls: Some(12),
            input: Some(4_000),
            cache_writes: Some(20_000),
            cache_reads: Some(300_000),
            output: Some(9_000),
            output_estimated: None,
        },
        ReportedUsage {
            name: "Worker: MCP server".into(),
            calls: None,
            input: None,
            cache_writes: None,
            cache_reads: None,
            output: Some(3_000),
            output_estimated: Some(true),
        },
    ];
    let finished = finish_drift_check(pool, s.agent, finish(key, "finished", Some(usage)))
        .await
        .unwrap();
    assert!(finished.contains("incomplete"), "{finished}");
    assert!(finished.contains("1 planned requirements"), "{finished}");
    assert!(finished.contains("Forge saw you file FOR-1"), "{finished}");
    let check = run(pool, key).await;
    assert_eq!(check["status"], "succeeded");
    assert_eq!(check["usage"][0]["cache_reads"], 300_000);
    assert_eq!(check["usage"][1]["input"], Value::Null);
    assert_eq!(check["usage"][1]["output_estimated"], true);

    // Once ended, it takes no more reports, and finishing again changes nothing.
    let late = report_drift_check(
        pool,
        s.agent,
        report(key, vec![verdict("Tokens expire", "met", "-")]),
    )
    .await
    .unwrap_err();
    assert!(late.contains("has ended"), "{late}");
    let twice = finish_drift_check(pool, s.agent, finish(key, "failed", None))
        .await
        .unwrap();
    assert!(twice.contains("nothing changed"), "{twice}");
    assert_eq!(run(pool, key).await["status"], "succeeded");

    // The check's page.
    let id = check["id"].as_str().unwrap();
    let details = s
        .app
        .get(
            &format!("/api/w/{}/drift-checks/{id}", s.workspace),
            &s.admin,
        )
        .await;
    assert_eq!(details.status, StatusCode::OK, "{}", details.body);
    let results = details.body["results"].as_array().unwrap();
    let verdicts: Vec<(&str, &Value)> = results
        .iter()
        .map(|r| (r["requirement"].as_str().unwrap(), &r["verdict"]))
        .collect();
    assert_eq!(
        verdicts,
        vec![
            ("Agent keys", &json!("met")),
            ("Tokens are shown once", &Value::Null),
            ("Tokens expire", &json!("skipped")),
        ]
    );
    assert_eq!(details.body["filed"].as_array().unwrap().len(), 1);
    assert_eq!(details.body["commented"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_check_keeps_its_verdicts(pool: PgPool) {
    let s = setup(pool).await;
    let pool = &s.app.pool;
    start_drift_check(pool, s.agent, start("audit", &["MCP server"]))
        .await
        .unwrap();
    report_drift_check(
        pool,
        s.agent,
        report("audit", vec![verdict("Agent keys", "not_met", "No key.")]),
    )
    .await
    .unwrap();
    let failed = finish_drift_check(pool, s.agent, finish("audit", "failed", None))
        .await
        .unwrap();
    assert!(failed.contains("Marked the check failed"), "{failed}");
    let check = run(pool, "audit").await;
    assert_eq!(check["status"], "failed");
    assert_eq!(check["error"], "a worker crashed");
    assert_eq!(check["usage"], Value::Null);
    assert_eq!(check["tally"]["not_met"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn restarts_leave_agents_checks_alone(pool: PgPool) {
    let s = setup(pool.clone()).await;
    start_drift_check(&pool, s.agent, start("running", &["MCP server"]))
        .await
        .unwrap();

    // A server without a model clears up left-over runs, then stops.
    forge_server::drift::worker(AppState::new(pool.clone(), false)).await;

    assert_eq!(run(&pool, "running").await["status"], "running");
    report_drift_check(
        &pool,
        s.agent,
        report("running", vec![verdict("Agent keys", "met", "-")]),
    )
    .await
    .unwrap();
    assert_eq!(run(&pool, "running").await["tally"]["met"], 1);
}
