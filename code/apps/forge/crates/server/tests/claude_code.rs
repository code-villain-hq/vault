//! What a person asks Forge AI goes to Claude Code on their Mac first, and
//! the server's key runs it only when Claude Code can't
//! (`studio/design/behaviour.md`, "Claude"). The night's runs stay on the
//! key, in a lane of their own.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use code_villain_claude::{Adapter, Kind};
use common::App;
use forge_ai::ScriptedProvider;
use forge_server::AppState;
use forge_server::drift::{self, DriftSettings, Lane};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    app: App,
    pool: PgPool,
    admin: String,
    workspace: String,
    service: Uuid,
    settings: DriftSettings,
    _root: tempfile::TempDir,
}

/// A workspace with one service, in a repository where rate limiting is
/// implemented, on a server with the key `provider`, or none.
async fn setup(pool: PgPool, provider: Option<Arc<ScriptedProvider>>) -> Setup {
    let root = tempfile::tempdir().unwrap();
    let root_path = std::fs::canonicalize(root.path()).unwrap();
    let repo = root_path.join("api");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/limits.rs"),
        "/// Allows 100 requests a minute per token.\npub fn rate_limit(token: &str) -> bool {\n    true\n}\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "Add rate limiting",
    ]);

    let settings = DriftSettings {
        repos_root: Some(root_path),
        check_every: None,
        max_turns: 20,
        github_clones: None,
        triage_new_issues: false,
    };
    let mut state = AppState::new(pool.clone(), false);
    match provider {
        Some(provider) => state = state.with_ai(provider, settings.clone()),
        None => state.drift = settings.clone(),
    }
    let app = App::with_state(state);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let snapshot = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let service = Uuid::now_v7();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Service", "id": service, "data": {
            "name": "API", "repository": repo.to_str().unwrap(), "paths": ["src/**"], "team_id": team,
        }},
    ]});
    let created = app
        .post(
            &format!("/api/w/{workspace}/sync/transactions"),
            Some(&admin),
            tx,
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    Setup {
        app,
        pool,
        admin,
        workspace,
        service,
        settings,
        _root: root,
    }
}

impl Setup {
    /// Asks for a check, from the desktop app (`here`) or the browser.
    async fn check(&self, here: bool) -> common::Response {
        let body = if here {
            json!({ "here": true })
        } else {
            json!({})
        };
        self.app
            .post(
                &format!("/api/w/{}/services/{}/check", self.workspace, self.service),
                Some(&self.admin),
                body,
            )
            .await
    }

    async fn run(&self, id: Uuid) -> Run {
        sqlx::query_as(
            "SELECT status, model, error, summary, cost_usd, input_tokens FROM ai_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// The run once it's `status`: Claude Code's answer is settled after the
    /// reply, on a task of its own.
    async fn run_once(&self, id: Uuid, status: &str) -> Run {
        for _ in 0..100 {
            let run = self.run(id).await;
            if run.status == status {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the run never became {status}: {:?}", self.run(id).await);
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Run {
    status: String,
    model: String,
    error: Option<String>,
    summary: String,
    cost_usd: f64,
    input_tokens: i64,
}

/// A request from Claude Code on the asker's Mac, with the run's token.
async fn with_token(app: &App, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .extension(ConnectInfo(SocketAddr::from(([10, 9, 9, 9], 40000))))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn tool(app: &App, token: &str, name: &str, arguments: Value) -> Value {
    let (status, reply) = with_token(
        app,
        "/api/ai/mcp",
        token,
        json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
                "params": { "name": name, "arguments": arguments } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply["result"].clone()
}

fn id_of(queued: &common::Response) -> Uuid {
    queued.body["id"].as_str().unwrap().parse().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_check_asked_from_the_desktop_app_is_handed_to_claude_code_there(pool: PgPool) {
    let s = setup(pool, None).await;

    // In the browser, a server without a key can't run it.
    assert_eq!(
        s.check(false).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let queued = s.check(true).await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    let run = id_of(&queued);
    let question = &queued.body["question"];
    let token = question["token"].as_str().unwrap();
    assert!(question["prompt"].as_str().unwrap().contains("API"));
    assert!(
        question["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("finish"))
    );
    assert_eq!(question["web"], false);
    // Claude Code has it now, so no worker takes it.
    let started = s.run(run).await;
    assert_eq!(
        (started.status.as_str(), started.model.as_str()),
        ("running", "claude-code")
    );

    // Claude Code there calls the run's tools on the server.
    let (status, started) = with_token(
        &s.app,
        "/api/ai/mcp",
        token,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(started["result"]["serverInfo"]["name"], "forge");
    let (status, _) = with_token(
        &s.app,
        "/api/ai/mcp",
        token,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (_, listed) = with_token(
        &s.app,
        "/api/ai/mcp",
        token,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"read_file") && names.contains(&"finish"),
        "{names:?}"
    );
    let read = tool(
        &s.app,
        token,
        "read_file",
        json!({ "path": "src/limits.rs" }),
    )
    .await;
    assert!(
        read["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("rate_limit")
    );
    let finished = tool(
        &s.app,
        token,
        "finish",
        json!({ "summary": "Rate limiting matches the specs." }),
    )
    .await;
    assert_eq!(finished["isError"], false, "{finished}");

    // And the desktop app says what came of it.
    let (status, _) = with_token(
        &s.app,
        "/api/answered",
        token,
        json!({ "text": "Checked.", "stop": "end_turn" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = s.run_once(run, "succeeded").await;
    assert_eq!(done.model, "claude-code");
    assert_eq!(done.summary, "Rate limiting matches the specs.");
    assert_eq!((done.cost_usd, done.input_tokens), (0.0, 0));
    assert_eq!(done.error, None);

    // The token ends with the run.
    let (status, over) = with_token(
        &s.app,
        "/api/ai/mcp",
        token,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(over["message"], "This run is over.", "{over}");
    let (status, _) = with_token(&s.app, "/api/answered", token, json!({ "text": "Again." })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = with_token(&s.app, "/api/answered", "guessed", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_key_runs_it_when_claude_code_isnt_installed(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedProvider::tool_call(
        "t1",
        "finish",
        json!({ "summary": "The key checked it." }),
    )]));
    let s = setup(pool, Some(provider.clone())).await;

    let queued = s.check(true).await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    let run = id_of(&queued);
    let token = queued.body["question"]["token"].as_str().unwrap();
    let (status, _) = with_token(
        &s.app,
        "/api/answered",
        token,
        json!({ "not_answered": "not_installed" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Back on the queue for what people ask, for the key.
    let waiting = s.run_once(run, "queued").await;
    assert_eq!(waiting.model, "scripted");
    let night = drift::run_next_in(&s.pool, provider.as_ref(), &s.settings, Some(Lane::Night));
    assert!(!night.await.unwrap());
    let asked = drift::run_next_in(&s.pool, provider.as_ref(), &s.settings, Some(Lane::Asked));
    assert!(asked.await.unwrap());
    let done = s.run(run).await;
    assert_eq!(done.status, "succeeded", "{done:?}");
    assert_eq!(done.summary, "The key checked it.");
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_key_claude_code_signed_out_ends_the_run(pool: PgPool) {
    let s = setup(pool, None).await;
    let queued = s.check(true).await;
    let run = id_of(&queued);
    let token = queued.body["question"]["token"].as_str().unwrap();
    with_token(
        &s.app,
        "/api/answered",
        token,
        json!({ "not_answered": "signed_out" }),
    )
    .await;
    let failed = s.run_once(run, "failed").await;
    assert!(
        failed.error.as_deref().unwrap().contains("isn't signed in"),
        "{failed:?}"
    );

    // Trouble on the Mac ends it in Claude Code's words.
    let queued = s.check(true).await;
    let run = id_of(&queued);
    let token = queued.body["question"]["token"].as_str().unwrap();
    with_token(
        &s.app,
        "/api/answered",
        token,
        json!({ "not_answered": "trouble", "said": "The adapter crashed." }),
    )
    .await;
    let failed = s.run_once(run, "failed").await;
    assert_eq!(failed.error.as_deref(), Some("The adapter crashed."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_nights_runs_and_what_people_ask_wait_in_lanes_of_their_own(pool: PgPool) {
    let finish = |id| ScriptedProvider::tool_call(id, "finish", json!({ "summary": "Fine." }));
    let provider = Arc::new(ScriptedProvider::new(vec![finish("t1"), finish("t2")]));
    let s = setup(pool, Some(provider.clone())).await;
    let next = async |lane| {
        drift::run_next_in(&s.pool, provider.as_ref(), &s.settings, Some(lane))
            .await
            .unwrap()
    };

    // A scheduled check is the night's.
    let workspace: Uuid = s.workspace.parse().unwrap();
    let scheduled = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ai_runs (id, workspace_id, service_id, trigger, status, kind, model, created_by) \
         SELECT $1, $2, $3, 'schedule', 'queued', 'drift', 'scripted', id FROM users LIMIT 1",
    )
    .bind(scheduled)
    .bind(workspace)
    .bind(s.service)
    .execute(&s.pool)
    .await
    .unwrap();
    assert!(!next(Lane::Asked).await);
    assert!(next(Lane::Night).await);
    assert_eq!(s.run(scheduled).await.status, "succeeded");

    // A check someone asks for in the browser is theirs.
    let queued = s.check(false).await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    assert!(queued.body.get("question").is_none(), "{}", queued.body);
    let asked = id_of(&queued);
    assert!(!next(Lane::Night).await);
    assert!(next(Lane::Asked).await);
    assert_eq!(s.run(asked).await.status, "succeeded");
}

#[sqlx::test(migrations = "../../migrations")]
async fn triage_asked_from_the_desktop_app_is_handed_over_too(pool: PgPool) {
    let s = setup(pool, None).await;
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let triage = snapshot["records"]["WorkflowState"]
        .as_array()
        .unwrap()
        .iter()
        .find(|state| state["name"] == "Triage")
        .unwrap()["id"]
        .clone();
    let issue = Uuid::now_v7();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": issue, "data": {
            "team_id": team, "title": "Logins fail after a wrong password", "state_id": triage,
        }},
    ]});
    s.app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;

    let url = format!("/api/w/{}/issues/{issue}/triage", s.workspace);
    assert_eq!(
        s.app.post(&url, Some(&s.admin), json!({})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let queued = s
        .app
        .post(&url, Some(&s.admin), json!({ "here": true }))
        .await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    let run = id_of(&queued);
    let question = &queued.body["question"];
    let token = question["token"].as_str().unwrap();
    assert!(question["prompt"].as_str().unwrap().contains("Logins fail"));
    assert!(
        question["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("suggest"))
    );

    let finished = tool(
        &s.app,
        token,
        "finish",
        json!({ "summary": "Too little to go on." }),
    )
    .await;
    assert_eq!(finished["isError"], false, "{finished}");
    with_token(&s.app, "/api/answered", token, json!({ "text": "Done." })).await;
    let done = s.run_once(run, "succeeded").await;
    assert_eq!(
        (done.model.as_str(), done.summary.as_str()),
        ("claude-code", "Too little to go on.")
    );
}

/// A stand-in for Claude Code's ACP adapter (`claude-code.ts`, run by bun)
/// that calls Forge AI's tools over MCP the way Claude Code does. Returns
/// it and the file its sessions are logged to.
fn claude_code_stand_in(dir: &Path) -> (PathBuf, PathBuf) {
    let bun = std::env::split_paths(&std::env::var("PATH").unwrap())
        .map(|dir| dir.join("bun"))
        .find(|bun| bun.is_file())
        .expect("bun on the PATH");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/claude-code.ts");
    let log = dir.join("sessions.jsonl");
    let adapter = dir.join("claude-agent-acp");
    std::fs::write(
        &adapter,
        format!(
            "#!/bin/sh\nexec '{}' '{}' '{}'\n",
            bun.display(),
            script.display(),
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        &adapter,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    (adapter, log)
}

/// The test's server, stopped when the test ends, so it gives the test's
/// database connections back.
struct Serving(tokio::task::AbortHandle);

impl Drop for Serving {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_desktop_apps_own_forge_asks_claude_code_on_this_mac(pool: PgPool) {
    let s = setup(pool.clone(), None).await;
    let off = s
        .app
        .get(&format!("/api/w/{}/ai", s.workspace), &s.admin)
        .await;
    assert_eq!(off.body["configured"], false, "{}", off.body);

    // This Mac's own Forge, on a real port for Claude Code to call.
    let dir = tempfile::tempdir().unwrap();
    let (program, log) = claude_code_stand_in(dir.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let adapter = Adapter::new(
        drift::here::FORGE,
        Kind::ClaudeCode,
        dir.path().join("claude"),
        format!("http://{address}{}", drift::here::MCP_PATH),
    )
    .program(&program)
    .start_timeout(Duration::from_secs(10));
    let mut state = AppState::new(pool, false)
        .on_this_mac()
        .with_claude_code(adapter);
    state.drift = s.settings.clone();
    let service =
        forge_server::router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, service).await });
    let _serving = Serving(server.abort_handle());
    let app = App::with_state(state);

    let on = app
        .get(&format!("/api/w/{}/ai", s.workspace), &s.admin)
        .await;
    assert_eq!(on.body["configured"], true, "{}", on.body);

    // The app asks as the browser would; Claude Code here runs it.
    let queued = app
        .post(
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            json!({}),
        )
        .await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    assert!(queued.body.get("question").is_none(), "{}", queued.body);
    let run = id_of(&queued);
    let done = s.run_once(run, "succeeded").await;
    assert_eq!(done.model, "claude-code");
    assert_eq!(done.summary, "Checked on this Mac: the API is limited.");
    assert_eq!(done.cost_usd, 0.0);

    // With only the run's tools, and none of its own.
    let sessions: Vec<Value> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(sessions.len(), 1);
    let allowed = sessions[0]["allowed"].as_array().unwrap();
    assert!(
        allowed.contains(&json!("mcp__forge__finish")),
        "{allowed:?}"
    );
    assert!(
        allowed
            .iter()
            .all(|tool| tool.as_str().unwrap().starts_with("mcp__forge__")),
        "{allowed:?}"
    );
    assert_eq!(sessions[0]["tools"], json!([]));
    assert!(
        sessions[0]["system"].as_str().unwrap().contains("drift"),
        "{}",
        sessions[0]["system"]
    );
}
