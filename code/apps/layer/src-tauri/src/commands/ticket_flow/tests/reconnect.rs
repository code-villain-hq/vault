#[cfg(test)]
mod cases {
    use super::super::*;
    use crate::config::GithubConfig;
    use crate::integrations::github::GitHub;
    use crate::{commands::github, git};
    use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};
    use parking_lot::Mutex;
    use serde_json::{json, Value};
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        task::Poll,
        time::Duration,
    };
    use tokio::{sync::Notify, task::JoinHandle, time::timeout};

    const LIMIT: Duration = Duration::from_secs(5);

    struct JiraFixture {
        phase: Mutex<String>,
        requests: Mutex<Vec<(String, String)>>,
        first_review: AtomicBool,
        review_posted: Notify,
        reply: Notify,
    }

    struct Fixture {
        state: Arc<AppState>,
        jira: Jira,
        remote: Arc<JiraFixture>,
        server: JoinHandle<()>,
        dir: PathBuf,
    }

    impl Fixture {
        async fn new() -> Self {
            let remote = Arc::new(JiraFixture {
                phase: Mutex::new("progress".into()),
                requests: Mutex::default(),
                first_review: AtomicBool::new(true),
                review_posted: Notify::new(),
                reply: Notify::new(),
            });
            let read = remote.clone();
            let write = remote.clone();
            let router = Router::new()
                .route(
                    "/rest/api/3/issue/{key}",
                    get(move |Path(key): Path<String>| {
                        let read = read.clone();
                        async move {
                            read.requests.lock().push((key.clone(), "read".into()));
                            let status = read.phase.lock().clone();
                            Json(json!({ "key": key, "fields": { "status": {
                                "id": status, "name": status, "statusCategory": { "key": "indeterminate" }
                            }}}))
                        }
                    }),
                )
                .route(
                    "/rest/api/3/issue/{key}/transitions",
                    get(|| async {
                        Json(json!({ "transitions": [
                            { "id": "20", "name": "Review", "to": { "id": "review", "name": "Review" } },
                            { "id": "30", "name": "Merged", "to": { "id": "merged", "name": "Merged" } }
                        ]}))
                    })
                    .post(move |Path(key): Path<String>, Json(body): Json<Value>| {
                        let write = write.clone();
                        async move {
                            let id = body["transition"]["id"].as_str().unwrap();
                            let stage = match id {
                                "20" => "review",
                                "30" => "merged",
                                _ => panic!("unexpected transition {id}"),
                            };
                            write.requests.lock().push((key, stage.into()));
                            *write.phase.lock() = stage.into();
                            if stage == "review" && write.first_review.swap(false, Ordering::SeqCst) {
                                write.review_posted.notify_one();
                                write.reply.notified().await;
                            }
                            StatusCode::NO_CONTENT
                        }
                    }),
                )
                .route("/replacement/repos/acme/api/pulls", get(|| async { Json(json!([])) }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let dir = std::env::temp_dir().join(format!("vl-ticket-sweep-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            git::run_for_tests(&dir, &["init", "-q", "-b", "ACME-1"]).unwrap();
            git::run_for_tests(&dir, &["remote", "add", "origin", "git@gh.example:acme/api.git"]).unwrap();
            let cfg = serde_json::from_value(json!({
                "github": { "api_url": url, "web_url": url },
                "jira": { "base_url": url, "email": "synthetic@example.test", "flow": { "ACME": {
                    "review": { "id":"review", "name":"Review" }, "merged": { "id":"merged", "name":"Merged" }
                }}},
                "projects": [{ "id":"p", "name":"api", "path":dir, "default_branch":"main" }],
                "tasks": [task(None)],
                "checkouts": [{ "id":"c", "task_id":"t", "project_id":"p", "path":dir, "base":"main" }]
            }))
            .unwrap();
            let state = Arc::new(crate::commands::repos::tests::state(&dir, cfg));
            let jira = Jira::new(&state.config.read().jira.unwrap(), "synthetic").unwrap();
            Self { state, jira, remote, server, dir }
        }

        async fn observation(&self) -> github::ConnectedGitHub {
            let session = self.state.github_session.lock().await;
            let config = self.state.config.read().github.unwrap();
            github::ConnectedGitHub {
                client: GitHub::new(&config, "synthetic").unwrap(),
                config,
                generation: session.generation,
                reviews_generation: session.reviews_generation,
            }
        }

        async fn reconnect(&self) -> github::ConnectedGitHub {
            let state = self.state.clone();
            timeout(
                LIMIT,
                tokio::task::spawn_blocking(move || {
                    github::apply_github_settings(&state, || {
                        state.config.update(|c| {
                            let cfg: &mut GithubConfig = c.github.as_mut().unwrap();
                            cfg.api_url.push_str("/replacement");
                        })
                    })
                    .unwrap();
                }),
            )
            .await
            .expect("GitHub can reconnect while a Jira reply is delayed")
            .unwrap();
            self.observation().await
        }

        async fn follow(&self, observation: &github::ConnectedGitHub, task: &Task, stage: Stage) -> Option<TicketMove> {
            let rows = [match stage {
                Stage::Review => row("open", false, false, 1),
                Stage::Merged => row("closed", false, true, 0),
            }];
            follow(&self.state, observation, &self.jira, task, &rows).await
        }

        async fn pause_at_review(&self) {
            timeout(LIMIT, self.remote.review_posted.notified()).await.unwrap();
        }

        fn saved(&self) -> Task {
            self.state.config.task("t").unwrap()
        }

        fn manually_move_to_progress(&self) {
            *self.remote.phase.lock() = "progress".into();
        }

        fn requests(&self) -> Vec<(String, String)> {
            self.remote.requests.lock().clone()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.server.abort();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn jira_sweep_reconnect_does_not_downgrade_stage_or_repeat_a_transition() {
        let fx = Fixture::new().await;
        let old = fx.observation().await;
        let snapshot = fx.saved();
        let (review, (finished_early, merged)) = tokio::join!(fx.follow(&old, &snapshot, Stage::Review), async {
            fx.pause_at_review().await;
            let current = fx.reconnect().await;
            // An actual PR read and its publication gate are independent of the waiting Jira reply.
            let rows = timeout(LIMIT, github::task_prs(&fx.state, &current, "t"))
                .await
                .expect("new PR read does not wait on Jira")
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert!(rows[0].error.is_none(), "{:?}", rows[0].error);
            let mut move_merged = std::pin::pin!(fx.follow(&current, &snapshot, Stage::Merged));
            let early = timeout(Duration::from_millis(400), &mut move_merged).await.ok();
            fx.remote.reply.notify_one();
            let finished_early = early.is_some();
            let merged = match early {
                Some(out) => out,
                None => timeout(LIMIT, move_merged).await.unwrap(),
            };
            (finished_early, merged)
        });
        assert_eq!(review.unwrap().moved_to.as_deref(), Some("Review"));
        assert_eq!(merged.unwrap().moved_to.as_deref(), Some("Merged"));
        let saved = fx.saved();
        fx.manually_move_to_progress();
        fx.follow(&fx.observation().await, &saved, Stage::Merged).await;
        let requests = fx.requests();
        let posts: Vec<_> = requests.iter().filter(|(_, step)| step != "read").map(|(_, step)| step.as_str()).collect();
        assert_eq!(
            (finished_early, saved.ticket_stage.as_deref(), posts),
            (false, Some("merged"), vec!["review", "merged"])
        );
    }

    #[tokio::test]
    async fn jira_sweep_reconnect_discards_a_waiting_old_observation_but_marks_an_in_flight_post() {
        let fx = Fixture::new().await;
        let old = fx.observation().await;
        let snapshot = fx.saved();
        let (review, (obsolete, later_review)) = tokio::join!(fx.follow(&old, &snapshot, Stage::Review), async {
            fx.pause_at_review().await;
            let mut waiting = std::pin::pin!(fx.follow(&old, &snapshot, Stage::Merged));
            assert!(matches!(futures_util::poll!(&mut waiting), Poll::Pending));
            let new = fx.reconnect().await;
            // A user can move it by hand as soon as Jira applies the Review POST, before the reply.
            fx.manually_move_to_progress();
            fx.remote.reply.notify_one();
            tokio::join!(waiting, fx.follow(&new, &snapshot, Stage::Review))
        });
        assert_eq!(review.unwrap().moved_to.as_deref(), Some("Review"));
        let requests = fx.requests();
        let seen: Vec<_> = requests.iter().map(|(key, kind)| (key.as_str(), kind.as_str())).collect();
        assert_eq!(
            (obsolete.is_none(), later_review.is_none(), fx.saved().ticket_stage.as_deref(), seen),
            (true, true, Some("review"), vec![("ACME-1", "read"), ("ACME-1", "review")])
        );
    }

    #[tokio::test]
    async fn jira_sweep_does_not_mark_a_relinked_task_or_act_on_its_old_key_after_waiting() {
        let fx = Fixture::new().await;
        let old = fx.observation().await;
        let snapshot = fx.saved();
        let (_, queued) = tokio::join!(fx.follow(&old, &snapshot, Stage::Review), async {
            fx.pause_at_review().await;
            let mut waiting = std::pin::pin!(fx.follow(&old, &snapshot, Stage::Merged));
            assert!(matches!(futures_util::poll!(&mut waiting), Poll::Pending));
            fx.state.config.update(|c| c.tasks[0].issue_key = Some("ACME-2".into())).unwrap();
            fx.remote.reply.notify_one();
            timeout(LIMIT, waiting).await.unwrap()
        });
        let saved = fx.saved();
        let requests = fx.requests();
        let seen: Vec<_> = requests.iter().map(|(key, kind)| (key.as_str(), kind.as_str())).collect();
        assert_eq!(saved.issue_key.as_deref(), Some("ACME-2"));
        assert_eq!(
            (queued.is_none(), saved.ticket_stage, seen),
            (true, None, vec![("ACME-1", "read"), ("ACME-1", "review")])
        );
    }
}
