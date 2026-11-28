#[cfg(test)]
mod cases {
    use super::super::*;
    use axum::http::HeaderMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::{test::MockRuntime, Manager};
    use tokio::{sync::Notify, task::JoinHandle, time::timeout};

    // The test build's Keychain substitute is process-wide. This pair of tests
    // uses the real connect logic with only loopback and synthetic credentials.
    static TEST_CONNECTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[derive(Default)]
    struct PausedRead {
        hold_old: AtomicBool,
        got_old_snapshot: AtomicBool,
        got_new_snapshot: AtomicBool,
        started: Notify,
        release: Notify,
    }

    impl PausedRead {
        async fn pause_old(&self) {
            if self.hold_old.load(Ordering::SeqCst) {
                self.started.notify_one();
                self.release.notified().await;
            }
        }

        async fn wait_started(&self) {
            timeout(std::time::Duration::from_secs(10), self.started.notified()).await.unwrap();
        }
    }

    struct Fixture {
        app: tauri::App<MockRuntime>,
        api: String,
        dir: PathBuf,
        server: JoinHandle<()>,
        paused: Arc<PausedRead>,
    }

    impl Fixture {
        async fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("vl-native-github-session-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            git::run_for_tests(&dir, &["init", "-q", "-b", "task"]).unwrap();
            git::run_for_tests(&dir, &["remote", "add", "origin", "git@gh.example:acme/api.git"]).unwrap();
            let cfg = serde_json::from_value(json!({
            "projects": [{"id": "p", "name": "api", "path": dir, "default_branch": "main"}],
            "tasks": [{"id": "t", "name": "task", "root": dir, "branch": "task", "created_at": "2026-11-01T12:00:00Z"}],
            "checkouts": [{"id": "c", "task_id": "t", "project_id": "p", "path": dir, "base": "main"}]
        }))
        .unwrap();
            let app = tauri::test::mock_builder()
                .invoke_handler(tauri::generate_handler![github_all_prs])
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap();
            app.manage(crate::commands::repos::tests::state(&dir, cfg));
            let paused = Arc::new(PausedRead::default());
            let pulls = paused.clone();
            let queue = paused.clone();
            let router = Router::new()
            .route("/user", get(|| async { Json(json!({"login": "test-user"})) }))
            .route(
                "/repos/acme/api/pulls",
                get(move |headers: HeaderMap| {
                    let paused = pulls.clone();
                    async move {
                        let old = is_old(headers);
                        if old {
                            paused.pause_old().await;
                        }
                        Json(if old {
                            json!([{
                                "number": 1, "title": "Merged on the old connection", "state": "closed",
                                "draft": false, "merged_at": "2026-11-01T12:00:00Z", "user": {"login": "ana"},
                                "head": {"ref": "task", "sha": ""}, "base": {"ref": "main"},
                                "html_url": "https://gh.example/acme/api/pull/1"
                            }])
                        } else {
                            json!([])
                        })
                    }
                }),
            )
            .route(
                "/graphql",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let paused = queue.clone();
                    async move {
                        let query = body["variables"]["q"].as_str().unwrap();
                        let numbers = if query.contains("team-review-requested:acme/devs") {
                            vec![4]
                        } else if is_old(headers) {
                            paused.pause_old().await;
                            if paused.got_old_snapshot.swap(true, Ordering::SeqCst) { vec![1, 5] } else { vec![1] }
                        } else if paused.got_new_snapshot.swap(true, Ordering::SeqCst) {
                            vec![2, 3]
                        } else {
                            vec![2]
                        };
                        let nodes: Vec<_> = numbers
                            .iter()
                            .map(|n| json!({
                                "number": n, "title": format!("t{n}"), "url": format!("https://gh.example/acme/api/pull/{n}"),
                                "isDraft": false, "createdAt": "2026-11-01T12:00:00Z", "updatedAt": "2026-11-02T12:00:00Z",
                                "headRefName": "task", "baseRefName": "main", "headRefOid": LIST_HEAD, "isCrossRepository": false,
                                "additions": 1, "deletions": 0, "changedFiles": 1, "author": {"login": "ana"},
                                "repository": {"nameWithOwner": "acme/api"}, "commits": {"nodes": []}, "viewerLatestReview": null
                            }))
                            .collect();
                        Json(json!({"data": {"search": {"issueCount": nodes.len(), "nodes": nodes}}}))
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let api = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            Self { app, api, dir, server, paused }
        }

        async fn connect(&self, token: &str) {
            self.save(token, None).await;
        }

        async fn save(&self, token: &str, team: Option<&str>) {
            let state = self.app.state::<AppState>();
            let source = state.config.with(|c| c.github_source("gh.example")).map(|g| g.key);
            github_connect(
                self.app.handle().clone(),
                source,
                self.api.clone(),
                "https://gh.example".into(),
                token.into(),
                team.map(str::to_owned),
            )
            .await
            .unwrap();
        }

        async fn review(&self) -> Result<ReviewQueue> {
            let state = self.app.state::<AppState>();
            let connected = github_gate(&state).await;
            let queue = review_queue(&state, &connected, false).await?;
            crate::news::saw_reviews(self.app.handle().clone(), connected.reviews_generation, queue).await
        }

        async fn deferred_banner(&self) -> (crate::commands::SubmitBanner, String) {
            let state = self.app.state::<AppState>();
            let connected = github_gate(&state).await;
            let queue = review_queue(&state, &connected, false).await.unwrap();
            let (tx, rx) = tokio::sync::oneshot::channel();
            crate::news::saw_reviews_with(
                self.app.handle().clone(),
                connected.reviews_generation,
                queue,
                move |_, submit, news| tx.send((submit, news[0].target.clone().unwrap())).ok().unwrap(),
            )
            .await
            .unwrap();
            rx.await.unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.server.abort();
            let credential =
                self.app.state::<AppState>().config.with(|c| c.github_sources.credentials.get("gh.example").cloned());
            if let Some(credential) = credential {
                let _ = credential.delete("gh.example");
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn is_old(headers: HeaderMap) -> bool {
        match headers["authorization"].to_str().unwrap() {
            "Bearer synthetic-old" => true,
            "Bearer synthetic-new" => false,
            _ => panic!("unexpected test authorization"),
        }
    }

    #[tokio::test]
    async fn native_github_reconnect_rejects_an_old_queue_after_the_new_snapshot() {
        let _held = TEST_CONNECTION.lock().await;
        let fx = Fixture::new().await;
        fx.connect("synthetic-old").await;
        fx.review().await.unwrap();
        fx.paused.hold_old.store(true, Ordering::SeqCst);

        let (late, ()) = tokio::join!(fx.review(), async {
            fx.paused.wait_started().await;
            fx.connect("synthetic-new").await;
            fx.review().await.unwrap();
            fx.paused.release.notify_one();
        });
        assert!(late.is_err(), "the superseded queue should not be used");
        fx.review().await.unwrap();

        let items = fx.app.state::<AppState>().messages.list();
        let targets: Vec<_> = items.iter().filter_map(|item| item.target.as_deref()).collect();
        assert_eq!(targets, ["review:gh.example/acme/api#3"], "only a later arrival on the new connection is news");
    }

    #[tokio::test]
    async fn native_github_reconnect_does_not_skip_recently_merged_tasks_or_keep_late_rows() {
        let _held = TEST_CONNECTION.lock().await;
        let fx = Fixture::new().await;
        let state = fx.app.state::<AppState>();
        fx.connect("synthetic-old").await;
        let old = github_gate(&state).await;
        let rows = task_prs(&state, "t").await.unwrap();
        assert!(rows[0].pr.as_ref().unwrap().merged);
        assert!(sweep_ids(&state, &old, None).await.is_empty(), "same-connection merge keeps the normal pacing");

        fx.paused.hold_old.store(true, Ordering::SeqCst);
        let (late, (first_sweep, new)) = tokio::join!(task_prs(&state, "t"), async {
            fx.paused.wait_started().await;
            fx.connect("synthetic-new").await;
            let new = github_gate(&state).await;
            let due = sweep_ids(&state, &new, None).await;
            assert!(task_prs(&state, "t").await.unwrap()[0].pr.is_none());
            fx.paused.release.notify_one();
            (due, new)
        });
        assert!(late.unwrap()[0].pr.as_ref().unwrap().merged, "the old server really returned the merged PR");
        assert_eq!(
            (first_sweep, sweep_ids(&state, &new, None).await, state.prs_seen.lock()["t"][0].pr.is_none()),
            (vec!["t".to_string()], vec!["t".to_string()], true),
            "the new sweep must be due, and the old completion must not replace the new observation"
        );
    }

    #[tokio::test]
    async fn native_github_window_reads_a_merged_task_already_settled_by_a_native_read() {
        let _held = TEST_CONNECTION.lock().await;
        let fx = Fixture::new().await;
        let state = fx.app.state::<AppState>();
        fx.connect("synthetic-old").await;
        let connected = github_gate(&state).await;
        let rows = task_prs(&state, "t").await.unwrap();
        assert!(rows[0].pr.as_ref().unwrap().merged);
        assert!(
            sweep_ids(&state, &connected, None).await.is_empty(),
            "the Henchman still skips a recently settled task"
        );

        let webview = tauri::WebviewWindowBuilder::new(&fx.app, "main", Default::default()).build().unwrap();
        let sweep = |known_ids: &[&str]| {
            let webview = webview.clone();
            let body = tauri::ipc::InvokeBody::Json(json!({ "knownTaskIds": known_ids }));
            // The mock IPC client blocks; keep the loopback fixture's runtime free to serve it.
            tokio::task::spawn_blocking(move || {
                tauri::test::get_ipc_response(
                    &webview,
                    tauri::webview::InvokeRequest {
                        cmd: "github_all_prs".into(),
                        callback: tauri::ipc::CallbackFn(0),
                        error: tauri::ipc::CallbackFn(1),
                        url: "tauri://localhost".parse().unwrap(),
                        body,
                        headers: Default::default(),
                        invoke_key: tauri::test::INVOKE_KEY.into(),
                    },
                )
                .unwrap()
                .deserialize::<Value>()
                .unwrap()
            })
        };
        let first = sweep(&[]).await.unwrap();
        assert_eq!(first[0]["task_id"], "t", "the window does not yet have the recently settled task");
        assert_eq!(first[0]["rows"][0]["pr"]["merged"], true);
        assert_eq!(
            sweep(&["t"]).await.unwrap(),
            json!([]),
            "the same task can be skipped once the window holds its rows"
        );
    }

    #[tokio::test]
    async fn native_github_a_review_team_preference_keeps_the_personal_baseline() {
        let _held = TEST_CONNECTION.lock().await;
        let fx = Fixture::new().await;
        fx.connect("synthetic-new").await;
        fx.review().await.unwrap();

        fx.save("", Some("acme/devs")).await;
        let team = fx.review().await.unwrap().sources.remove(0).team.unwrap();
        assert_eq!(team.slug, "acme/devs");
        assert_eq!(team.prs[0].number, 4);
        let items = fx.app.state::<AppState>().messages.list();
        let targets: Vec<_> = items.iter().filter_map(|item| item.target.as_deref()).collect();
        assert_eq!(
            targets,
            ["review:gh.example/acme/api#3"],
            "the new team primes quietly without forgetting personal requests"
        );
    }

    #[tokio::test]
    async fn native_github_delayed_review_banner_does_not_post_after_reconnect() {
        let _held = TEST_CONNECTION.lock().await;
        let fx = Fixture::new().await;
        fx.connect("synthetic-old").await;
        fx.review().await.unwrap();
        let (old, old_target) = fx.deferred_banner().await;
        assert_eq!(old_target, "review:gh.example/acme/api#5");

        fx.connect("synthetic-new").await;
        fx.review().await.unwrap();
        let (current, current_target) = fx.deferred_banner().await;
        let posted = Arc::new(Mutex::new(Vec::new()));
        for (submit, target) in [(old, old_target), (current, current_target)] {
            let posted = posted.clone();
            submit(Box::new(move || posted.lock().unwrap().push(target))).await.unwrap();
        }

        assert_eq!(
            *posted.lock().unwrap(),
            ["review:gh.example/acme/api#3"],
            "only the active generation reaches native delivery"
        );
    }
}
