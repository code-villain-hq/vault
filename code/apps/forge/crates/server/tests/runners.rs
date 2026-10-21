//! Agent runners: registering one, which changes wake an agent, delivering
//! signed wake-ups with retries, and holding them while the agent is busy.

mod common;

use std::sync::{Arc, Mutex};

use axum::http::{HeaderMap, StatusCode};
use common::App;
use forge_domain::Actor;
use forge_server::mcp::tools::*;
use forge_server::runners::{self, SIGNATURE_HEADER};
use serde_json::{Value, json};
use sqlx::PgPool;

struct Setup {
    app: App,
    admin: String,
    base: String,
    ada: Actor,
    agent: Actor,
}

/// Ada's workspace with the agent "Claude Code" in it.
async fn setup(pool: PgPool) -> Setup {
    let app = App::new(pool);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let base = format!("/api/w/{workspace}");
    let created = app
        .post(
            &format!("{base}/agents"),
            Some(&admin),
            json!({ "name": "Claude Code" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let workspace_id = workspace.parse().unwrap();
    let ada = sqlx::query_scalar("SELECT id FROM users WHERE email = 'ada@example.com'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let agent = created.body["agent_id"].as_str().unwrap().parse().unwrap();
    Setup {
        app,
        admin,
        base,
        ada: Actor {
            user_id: ada,
            workspace_id,
        },
        agent: Actor {
            user_id: agent,
            workspace_id,
        },
    }
}

impl Setup {
    /// Registers the agent's runner; returns its secret.
    async fn runner(&self, url: &str) -> String {
        let response = self
            .app
            .call(
                "PUT",
                &format!("{}/agents/{}/runner", self.base, self.agent.user_id),
                Some(&self.admin),
                Some(json!({ "url": url })),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response.body["secret"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    async fn agent_info(&self) -> Value {
        let agents = self
            .app
            .get(&format!("{}/agents", self.base), &self.admin)
            .await
            .body;
        agents[0].clone()
    }

    /// The agent's wake-ups, oldest first: (event, issue).
    async fn wakeups(&self) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT w.event, t.key || '-' || i.number FROM agent_wakeups w \
             JOIN issues i ON i.id = w.issue_id JOIN teams t ON t.id = i.team_id \
             WHERE w.user_id = $1 ORDER BY w.created_at, w.id",
        )
        .bind(self.agent.user_id)
        .fetch_all(&self.app.pool)
        .await
        .unwrap()
    }

    async fn create_issue(&self, by: Actor, title: &str, assignee: Option<&str>) {
        self.create_with_priority(by, title, assignee, None).await;
    }

    async fn create_with_priority(
        &self,
        by: Actor,
        title: &str,
        assignee: Option<&str>,
        priority: Option<&str>,
    ) {
        create_issue(
            &self.app.pool,
            by,
            CreateIssue {
                team: "FOR".into(),
                title: title.into(),
                description: None,
                status: Some("Todo".into()),
                priority: priority.map(Into::into),
                assignee: assignee.map(Into::into),
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
    }

    async fn update(
        &self,
        by: Actor,
        identifier: &str,
        status: Option<&str>,
        assignee: Option<&str>,
    ) {
        // An agent starts an issue only once its spec change is written.
        if by == self.agent && matches!(status, Some("In Progress" | "In Review")) {
            self.write_spec_change(identifier).await;
        }
        update_issue(
            &self.app.pool,
            by,
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

    /// The agent writes that the issue changes no spec, as before starting it.
    async fn write_spec_change(&self, identifier: &str) {
        save_spec_change(
            &self.app.pool,
            self.agent,
            SaveSpecChange {
                identifier: identifier.into(),
                spec: None,
                new_spec: None,
                service: None,
                requirements: None,
                body: None,
                drop: None,
                replaces: None,
                no_spec_change: Some("Only a test.".into()),
            },
        )
        .await
        .unwrap();
    }

    /// The agent starts or finishes work on an issue, as through MCP.
    async fn start(&self, identifier: &str) {
        self.write_spec_change(identifier).await;
        start_work(&self.app.pool, self.agent, issue_ref(identifier))
            .await
            .unwrap();
    }

    async fn finish(&self, identifier: &str) {
        finish_work(&self.app.pool, self.agent, finishing(identifier))
            .await
            .unwrap();
    }

    /// The agent's latest wake-ups as shown on the Agents page, newest
    /// first: (issue, status).
    async fn shown(&self) -> Vec<(String, String)> {
        self.agent_info().await["runner"]["wakeups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["issue"].as_str().unwrap().to_string(),
                    w["status"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    async fn comment(&self, by: Actor, identifier: &str, body: &str) {
        comment_on_issue(
            &self.app.pool,
            by,
            CommentOnIssue {
                identifier: identifier.into(),
                body: body.into(),
            },
        )
        .await
        .unwrap();
    }
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

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn admins_register_a_runner_and_see_its_secret_once(pool: PgPool) {
    let s = setup(pool).await;
    let path = format!("{}/agents/{}/runner", s.base, s.agent.user_id);

    // Forge will send requests there, so only admins choose it.
    let invite = s.app.invite(&s.admin, &s.base[7..], "member").await;
    let bob = s.app.join("Bob", &invite).await;
    let refused = s
        .app
        .call(
            "PUT",
            &path,
            Some(&bob),
            Some(json!({ "url": "http://localhost:4100/wake" })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    for bad in ["not a url", "ftp://example.com/wake", "localhost:4100"] {
        let response = s
            .app
            .call("PUT", &path, Some(&s.admin), Some(json!({ "url": bad })))
            .await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    assert!(s.agent_info().await["runner"].is_null());
    let secret = s.runner(" http://localhost:4100/wake ").await;
    assert!(secret.starts_with("forge_runner_"), "{secret}");
    let runner = &s.agent_info().await["runner"];
    assert_eq!(runner["url"], "http://localhost:4100/wake");
    assert_eq!(runner["wakeups"], json!([]));
    assert!(
        !runner.to_string().contains(&secret),
        "the secret is shown only once"
    );

    // A new URL keeps the secret; a new secret replaces it.
    assert_eq!(s.runner("http://localhost:4200/wake").await, "");
    assert_eq!(
        s.agent_info().await["runner"]["url"],
        "http://localhost:4200/wake"
    );
    let renewed = s
        .app
        .post(&format!("{path}/secret"), Some(&s.admin), json!(null))
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.body);
    assert_ne!(renewed.body["secret"], secret);

    let removed = s.app.call("DELETE", &path, Some(&s.admin), None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert!(s.agent_info().await["runner"].is_null());
    let again = s.app.call("DELETE", &path, Some(&s.admin), None).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn work_for_the_agent_wakes_it(pool: PgPool) {
    let s = setup(pool).await;

    // Without a runner, nothing is queued.
    s.create_issue(s.ada, "Before the runner", Some("Claude Code"))
        .await;
    assert!(s.wakeups().await.is_empty());
    s.runner("http://localhost:4100/wake").await;

    // Assigned to it, when created or later.
    s.create_issue(s.ada, "Build the runner", Some("Claude Code"))
        .await;
    s.create_issue(s.ada, "Write the docs", None).await;
    s.update(s.ada, "FOR-3", None, Some("Claude Code")).await;
    assert_eq!(
        s.wakeups().await,
        pairs(&[("assigned", "FOR-2"), ("assigned", "FOR-3")])
    );

    // Its own changes don't wake it; someone else's comment does.
    s.comment(s.agent, "FOR-2", "Picking this up.").await;
    s.update(s.agent, "FOR-2", Some("In Progress"), None).await;
    s.update(s.agent, "FOR-2", Some("In Review"), None).await;
    assert_eq!(s.wakeups().await.len(), 2);
    s.comment(s.ada, "FOR-2", "Looks good, one question.").await;

    // Sent back: from In Review to an earlier status. Moving on doesn't count.
    s.update(s.ada, "FOR-2", Some("In Progress"), None).await;
    s.update(s.ada, "FOR-2", Some("In Review"), None).await;
    s.update(s.ada, "FOR-2", Some("Done"), None).await;
    assert_eq!(
        s.wakeups().await[2..],
        pairs(&[("comment", "FOR-2"), ("sent_back", "FOR-2")])
    );

    // A mention wakes it on anyone's issue, and notifies it like a person.
    s.create_issue(s.ada, "Ada's own issue", Some("me")).await;
    s.comment(s.ada, "FOR-4", "Ada's note to self.").await;
    s.comment(s.ada, "FOR-4", "@Claude Code can you look at this?")
        .await;
    assert_eq!(s.wakeups().await[4..], pairs(&[("mention", "FOR-4")]));
    let mentioned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE user_id = $1 AND type = 'mention'",
    )
    .bind(s.agent.user_id)
    .fetch_one(&s.app.pool)
    .await
    .unwrap();
    assert_eq!(mentioned, 1);

    // Comments on issues it only created don't concern it.
    s.create_issue(s.agent, "Filed by the agent", Some("Ada"))
        .await;
    s.comment(s.ada, "FOR-5", "Thanks, I'll take it.").await;
    // One change, one wake-up, even with several reasons.
    s.create_issue(s.ada, "Mention and assign", None).await;
    update_issue(
        &s.app.pool,
        s.ada,
        UpdateIssue {
            identifier: "FOR-6".into(),
            description: Some("@Claude Code, yours.".into()),
            assignee: Some("Claude Code".into()),
            reviewer: None,
            status: None,
            title: None,
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
    assert_eq!(s.wakeups().await[5..], pairs(&[("mention", "FOR-6")]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn accepting_its_issue_out_of_triage_wakes_the_agent(pool: PgPool) {
    let s = setup(pool).await;
    s.runner("http://localhost:4100/wake").await;
    // The agent asks for a decision: it moves its own issue to Triage.
    s.create_issue(s.agent, "Needs a decision", Some("me"))
        .await;
    s.update(s.agent, "FOR-1", Some("Triage"), None).await;
    s.create_issue(s.agent, "Not scheduled yet", Some("me"))
        .await;
    s.update(s.agent, "FOR-2", Some("Triage"), None).await;
    assert!(s.wakeups().await.is_empty());

    // Accepted to Todo, it goes on; to Backlog, it waits.
    s.update(s.ada, "FOR-1", Some("Todo"), None).await;
    s.update(s.ada, "FOR-2", Some("Backlog"), None).await;
    assert_eq!(s.wakeups().await, pairs(&[("accepted", "FOR-1")]));
}

/// A stand-in runner: answers with the status in `status`, and keeps what
/// it was sent.
async fn fake_runner(status: Arc<Mutex<u16>>) -> (String, Arc<Mutex<Vec<(HeaderMap, String)>>>) {
    let received: Arc<Mutex<Vec<(HeaderMap, String)>>> = Arc::default();
    let kept = received.clone();
    let app = axum::Router::new().route(
        "/wake",
        axum::routing::post(move |headers: HeaderMap, body: String| {
            let kept = kept.clone();
            let status = status.clone();
            async move {
                kept.lock().unwrap().push((headers, body));
                StatusCode::from_u16(*status.lock().unwrap()).unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/wake", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, received)
}

#[sqlx::test(migrations = "../../migrations")]
async fn wakeups_are_signed_and_retried(pool: PgPool) {
    let s = setup(pool).await;
    let status = Arc::new(Mutex::new(503));
    let (url, received) = fake_runner(status.clone()).await;
    let secret = s.runner(&url).await;
    let client = reqwest::Client::new();

    s.create_issue(s.ada, "Build the runner", Some("Claude Code"))
        .await;
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 1);
    let wakeup = &s.agent_info().await["runner"]["wakeups"][0];
    assert_eq!(wakeup["status"], "pending");
    assert_eq!(wakeup["last_error"], "HTTP 503");
    assert_eq!(wakeup["attempts"], 1);
    // Not again until it's due.
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 0);

    *status.lock().unwrap() = 200;
    sqlx::query("UPDATE agent_wakeups SET next_attempt_at = now()")
        .execute(&s.app.pool)
        .await
        .unwrap();
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 1);
    let wakeup = &s.agent_info().await["runner"]["wakeups"][0];
    assert_eq!(wakeup["status"], "delivered");
    assert_eq!(wakeup["issue"], "FOR-1");
    assert_eq!(wakeup["event"], "assigned");
    assert!(wakeup["last_error"].is_null());

    let received = received.lock().unwrap().clone();
    assert_eq!(received.len(), 2);
    let (headers, body) = &received[1];
    assert_eq!(
        headers[SIGNATURE_HEADER].to_str().unwrap(),
        runners::signature(&secret, body.as_bytes())
    );
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["event"], "assigned");
    assert_eq!(body["issue"], "FOR-1");
    assert_eq!(body["workspace_id"], json!(s.agent.workspace_id));
    assert_eq!(body["agent_id"], json!(s.agent.user_id));
    assert_eq!(body["id"], wakeup["id"]);

    // Forge gives up after six attempts.
    *status.lock().unwrap() = 500;
    s.start("FOR-1").await;
    s.finish("FOR-1").await;
    s.comment(s.ada, "FOR-1", "Still there?").await;
    sqlx::query("UPDATE agent_wakeups SET attempts = 5 WHERE status = 'pending'")
        .execute(&s.app.pool)
        .await
        .unwrap();
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 1);
    let wakeup = &s.agent_info().await["runner"]["wakeups"][0];
    assert_eq!(wakeup["event"], "comment");
    assert_eq!(wakeup["status"], "failed");
    assert_eq!(wakeup["attempts"], 6);
}

#[sqlx::test(migrations = "../../migrations")]
async fn wakeups_wait_while_the_agent_is_busy(pool: PgPool) {
    let s = setup(pool).await;
    let (url, received) = fake_runner(Arc::new(Mutex::new(200))).await;
    s.runner(&url).await;
    let client = reqwest::Client::new();
    let deliver = || runners::deliver_due(&s.app.pool, &client);
    let sent = || {
        received
            .lock()
            .unwrap()
            .iter()
            .map(|(_, body)| serde_json::from_str::<Value>(body).unwrap()["issue"].clone())
            .collect::<Vec<_>>()
    };

    s.create_issue(s.ada, "Build the runner", Some("Claude Code"))
        .await;
    assert_eq!(deliver().await.unwrap(), 1);
    s.start("FOR-1").await;

    // While it works, wake-ups wait, even about the issue it's on.
    s.create_with_priority(s.ada, "Tidy the docs", Some("Claude Code"), Some("low"))
        .await;
    s.create_with_priority(s.ada, "Fix the login", Some("Claude Code"), Some("high"))
        .await;
    s.comment(s.ada, "FOR-1", "One more thing.").await;
    assert_eq!(deliver().await.unwrap(), 0);
    assert_eq!(
        s.shown().await,
        pairs(&[
            ("FOR-1", "waiting"),
            ("FOR-3", "waiting"),
            ("FOR-2", "waiting"),
            ("FOR-1", "delivered"),
        ])
    );

    // Free again: one goes out, the highest priority. The rest wait until
    // it has taken that one up and is free again.
    s.finish("FOR-1").await;
    assert_eq!(deliver().await.unwrap(), 1);
    assert_eq!(deliver().await.unwrap(), 0);
    assert_eq!(sent(), [json!("FOR-1"), json!("FOR-3")]);
    s.start("FOR-3").await;
    assert_eq!(deliver().await.unwrap(), 0);

    // The issue moving on frees it too. Low priority goes before none.
    s.update(s.ada, "FOR-3", Some("Done"), None).await;
    assert_eq!(deliver().await.unwrap(), 1);
    assert_eq!(sent()[2], json!("FOR-2"));

    // A run that never starts work gets the next one after a while.
    assert_eq!(deliver().await.unwrap(), 0);
    sqlx::query("UPDATE agent_runners SET woken_at = now() - interval '11 minutes'")
        .execute(&s.app.pool)
        .await
        .unwrap();
    assert_eq!(deliver().await.unwrap(), 1);
    assert_eq!(sent()[3], json!("FOR-1"));

    // Nor does a run that stopped (an hour without a sign of life) hold them.
    s.start("FOR-2").await;
    s.create_issue(s.ada, "Write the README", Some("Claude Code"))
        .await;
    assert_eq!(deliver().await.unwrap(), 0);
    assert_eq!(s.shown().await[0], ("FOR-4".into(), "waiting".into()));
    sqlx::query("UPDATE workspace_members SET working_seen_at = now() - interval '61 minutes'")
        .execute(&s.app.pool)
        .await
        .unwrap();
    assert_eq!(deliver().await.unwrap(), 1);
    assert_eq!(s.shown().await[0], ("FOR-4".into(), "delivered".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn waiting_wakeups_go_out_in_the_order_of_the_work_queue(pool: PgPool) {
    let s = setup(pool).await;
    let (url, received) = fake_runner(Arc::new(Mutex::new(200))).await;
    s.runner(&url).await;
    let client = reqwest::Client::new();
    s.create_issue(s.ada, "Build the runner", Some("Claude Code"))
        .await;
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 1);
    s.start("FOR-1").await;
    s.create_with_priority(s.ada, "Fix the login", Some("Claude Code"), Some("high"))
        .await;
    s.create_with_priority(s.ada, "Tidy the docs", Some("Claude Code"), Some("low"))
        .await;

    s.finish("FOR-1").await;
    assert_eq!(runners::deliver_due(&s.app.pool, &client).await.unwrap(), 1);
    let last: Value = serde_json::from_str(&received.lock().unwrap().last().unwrap().1).unwrap();
    assert_eq!(last["issue"], "FOR-2", "the high-priority one first");
}
