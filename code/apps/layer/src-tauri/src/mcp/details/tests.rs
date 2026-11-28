#[cfg(test)]
mod cases {
    use super::super::*;
    use crate::commands::AppState;
    use crate::config::{AppConfig, ConfigStore};
    use crate::pty::{PaneKind, SpawnOptions};
    use serde_json::json;
    use tauri::{Listener, Manager};

    struct Fixture {
        app: tauri::App<tauri::test::MockRuntime>,
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("layer-mcp-details-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let cfg: AppConfig = serde_json::from_value(json!({
                "tasks":[{"id":"t","name":"Source","root":"/work/t","branch":"source",
                          "created_at":"2026-01-01T00:00:00Z"}]
            }))
            .unwrap();
            let app = tauri::test::mock_app();
            app.manage(AppState {
                config: ConfigStore::for_tests(root.join("config.json"), cfg),
                ptys: Default::default(),
                ticket_types: Default::default(),
                epic_field_missing: Default::default(),
                pending_notices: Default::default(),
                status_cache: Default::default(),
                github_session: Default::default(),
                prs_seen: Default::default(),
                news: Default::default(),
                messages: crate::messages::Messages::for_tests(root.join("messages.json")),
                notes: crate::notes::Notes::load(&root),
                browser: Default::default(),
                loops: Default::default(),
                runs: crate::runs::Runs::for_tests(root.join("runs.json")),
                slack_bot: Default::default(),
            });
            app.state::<AppState>().config.update(|_| ()).unwrap();
            Self { app, root }
        }

        fn disk(&self) -> Value {
            serde_json::from_slice(&std::fs::read(self.root.join("config.json")).unwrap()).unwrap()
        }

        async fn call(&self, name: &str, args: Value, caller: Option<&str>) -> Result<Value> {
            call(self.app.handle(), name, args, caller).await
        }

        fn pane(&self, task: &str, kind: PaneKind, checkout: Option<&str>, room: &str) -> String {
            std::fs::create_dir_all(room).unwrap();
            let state = self.app.state::<AppState>();
            let pane = state
                .ptys
                .spawn(
                    self.app.handle(),
                    SpawnOptions {
                        task_id: task.into(),
                        checkout_id: checkout.map(str::to_string),
                        cwd: room.to_owned(),
                        kind,
                        title: "Agent".into(),
                        program: "/bin/sleep".into(),
                        args: vec!["60".into()],
                        agent_id: Some("claude".into()),
                        rows: None,
                        cols: None,
                        initial_input: None,
                        prompted: true,
                        env: vec![],
                        title_activity: None,
                        title_topic: None,
                    },
                )
                .unwrap();
            let saved = serde_json::from_value(json!({
                "id":pane.id,"task_id":task,"kind":if kind == PaneKind::Agent {"agent"} else {"shell"},
                "checkout_id":checkout,"cwd":room
            }))
            .unwrap();
            state.config.update(|c| c.saved_panes.push(saved)).unwrap();
            pane.id
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for pane in self.app.state::<AppState>().ptys.list(None) {
                let _ = self.app.state::<AppState>().ptys.close(&pane.id);
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn listed_tools_dispatch_only_the_three_local_edits_with_required_fields() {
        let listed = crate::mcp::tests::served();
        for (name, field) in [("task_rename", "name"), ("task_set_tag", "assigned"), ("task_set_color", "color")] {
            assert!(owns(name), "{name} must route to the focused handler");
            let schema = &listed.iter().find(|t| t["name"] == name).unwrap()["inputSchema"];
            assert!(schema["required"].as_array().unwrap().contains(&json!(field)));
            assert!(!schema["required"].as_array().unwrap().contains(&json!("task_id")));
            assert!(!crate::mcp::tool_needs_confirm(name, &json!({})));
        }
        assert!(!owns("list_tasks"));
    }

    #[tokio::test]
    async fn explicit_task_edits_return_canonical_details_preserve_fields_and_clear() {
        let f = Fixture::new();
        let original = f.disk();
        assert_eq!(
            f.call("task_rename", json!({"task_id":"t","name":" Alias "}), None).await.unwrap(),
            json!({"display_name":"Alias","tags":[],"color":null})
        );
        f.call("task_set_tag", json!({"task_id":"t","tag":" Review ","assigned":true}), None).await.unwrap();
        assert_eq!(
            f.call("task_set_color", json!({"task_id":"t","color":"data-3"}), None).await.unwrap(),
            json!({"display_name":"Alias","tags":["Review"],"color":"data-3"})
        );
        let cleared = f.call("task_rename", json!({"task_id":"t","name":null}), None).await.unwrap();
        assert_eq!(cleared, json!({"display_name":null,"tags":["Review"],"color":"data-3"}));
        let cleared = f.call("task_set_color", json!({"task_id":"t","color":null}), None).await.unwrap();
        assert_eq!(cleared, json!({"display_name":null,"tags":["Review"],"color":null}));
        f.call("task_set_tag", json!({"task_id":"t","tag":"review","assigned":false}), None).await.unwrap();
        assert_eq!(f.disk()["task_details"], json!({}));
        assert_eq!(f.disk()["tag_catalog"], json!(["Review"]));
        assert_eq!(f.disk()["tasks"], original["tasks"]);
        assert_eq!(f.disk()["saved_panes"], original["saved_panes"]);
    }

    #[tokio::test]
    async fn omitted_target_resolves_live_work_and_saved_standalone_chat() {
        for task in ["t", "chat"] {
            let f = Fixture::new();
            let pane = f.pane(task, PaneKind::Agent, None, f.root.to_str().unwrap());
            let result = f.call("task_rename", json!({"name":"Caller"}), Some(&pane)).await.unwrap();
            assert_eq!(result["display_name"], "Caller");
            let disk = f.disk();
            if task == "chat" {
                assert_eq!(disk["chat_details"][f.root.to_str().unwrap()]["display_name"], "Caller");
                assert_eq!(disk["task_details"], json!({}));
            } else {
                assert_eq!(disk["task_details"]["t"]["display_name"], "Caller");
                assert_eq!(disk["chat_details"], json!({}));
            }
        }
    }

    #[tokio::test]
    async fn malformed_explicit_targets_never_fall_back_to_the_caller() {
        let f = Fixture::new();
        let pane = f.pane("t", PaneKind::Agent, None, f.root.to_str().unwrap());
        let before = f.disk();
        for id in
            [json!(null), json!(""), json!(" "), json!(3), json!(false), json!({}), json!("missing"), json!("chat")]
        {
            assert!(f.call("task_rename", json!({"task_id":id,"name":"Wrong"}), Some(&pane)).await.is_err(), "{id}");
            assert_eq!(f.disk(), before);
        }
        for caller in [None, Some("missing")] {
            assert!(f.call("task_rename", json!({"name":"Wrong"}), caller).await.is_err());
        }
    }

    #[tokio::test]
    async fn ineligible_callers_and_disappeared_saved_chats_are_refused() {
        for (task, kind, checkout) in [
            ("chat", PaneKind::Shell, None),
            ("chat", PaneKind::Agent, Some("checkout")),
            ("missing", PaneKind::Agent, None),
        ] {
            let f = Fixture::new();
            let pane = f.pane(task, kind, checkout, f.root.to_str().unwrap());
            let before = f.disk();
            assert!(f.call("task_rename", json!({"name":"Wrong"}), Some(&pane)).await.is_err());
            assert_eq!(f.disk(), before);
        }
        let f = Fixture::new();
        let pane = f.pane("chat", PaneKind::Agent, None, f.root.to_str().unwrap());
        f.app.state::<AppState>().config.update(|c| c.saved_panes.clear()).unwrap();
        assert!(f.call("task_rename", json!({"name":"Wrong"}), Some(&pane)).await.is_err());
        assert_eq!(f.disk()["chat_details"], json!({}));
    }

    #[tokio::test]
    async fn protected_core_excludes_henchman_routines_and_embedded_epic_chat() {
        for (task, suffix) in
            [("chat", "_chat/henchman"), ("chat", "_chat/routines/routine/run"), ("epic-chat", "_chat/embedded")]
        {
            let f = Fixture::new();
            let room = f.root.join(suffix);
            let pane = f.pane(task, PaneKind::Agent, None, room.to_str().unwrap());
            let before = f.disk();
            assert!(f.call("task_set_color", json!({"color":"data-1"}), Some(&pane)).await.is_err());
            assert_eq!(f.disk(), before);
        }
    }

    #[tokio::test]
    async fn explicit_work_target_ignores_caller_and_unspecified_payload_fields() {
        let f = Fixture::new();
        f.call("task_set_tag", json!({"task_id":"t","tag":"Keep","assigned":true}), None).await.unwrap();
        let result = f
            .call(
                "task_rename",
                json!({
                    "task_id":"t","name":"Only name","tags":["Overwrite"],"color":"data-6"
                }),
                Some("missing"),
            )
            .await
            .unwrap();
        assert_eq!(result, json!({"display_name":"Only name","tags":["Keep"],"color":null}));
        let result = f
            .call(
                "task_set_color",
                json!({
                    "task_id":"t","color":"data-2","name":"Overwrite","tags":[]
                }),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result, json!({"display_name":"Only name","tags":["Keep"],"color":"data-2"}));
    }

    #[tokio::test]
    async fn chat_tag_uses_the_profile_catalog_and_publishes_chat_notification() {
        let f = Fixture::new();
        f.call("task_set_tag", json!({"task_id":"t","tag":"Review","assigned":true}), None).await.unwrap();
        let pane = f.pane("chat", PaneKind::Agent, None, f.root.to_str().unwrap());
        let events = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let received = events.clone();
        f.app.listen("item-details:changed", move |event| {
            received.lock().push(serde_json::from_str::<Value>(event.payload()).unwrap());
        });
        let details = f.call("task_set_tag", json!({"tag":"review","assigned":true}), Some(&pane)).await.unwrap();
        assert_eq!(details, json!({"display_name":null,"tags":["Review"],"color":null}));
        assert_eq!(*events.lock(), vec![json!({"kind":"chat","catalog_changed":false})]);
        assert_eq!(f.disk()["tag_catalog"], json!(["Review"]));
    }

    #[tokio::test]
    async fn existing_mcp_pane_list_never_includes_desktop_details() {
        let f = Fixture::new();
        let pane = f.pane("chat", PaneKind::Agent, None, f.root.to_str().unwrap());
        f.call("task_rename", json!({"name":"Private alias"}), Some(&pane)).await.unwrap();
        let state = f.app.state::<AppState>();
        let raw = crate::mcp::listed_panes(&state, &json!({"task_id":"chat"})).unwrap();
        assert_eq!(raw[0]["id"], pane);
        assert_eq!(raw[0]["title"], "Agent");
        assert!(raw[0].get("details").is_none());
        assert_eq!(raw, serde_json::to_value(state.ptys.list(Some("chat"))).unwrap());
        assert_eq!(crate::mcp::listed_panes(&state, &json!({"task_id":"t"})).unwrap(), json!([]));
    }

    #[tokio::test]
    async fn supplied_fields_and_editor_validation_are_enforced_without_partial_writes() {
        let f = Fixture::new();
        let before = f.disk();
        let cases = [
            ("task_rename", json!({})),
            ("task_rename", json!({"name":3})),
            ("task_rename", json!({"name":"line\nbreak"})),
            ("task_rename", json!({"name":"line\u{2028}break"})),
            ("task_rename", json!({"name":"a".repeat(121)})),
            ("task_set_color", json!({})),
            ("task_set_color", json!({"color":"red"})),
            ("task_set_tag", json!({"tag":"Tag"})),
            ("task_set_tag", json!({"tag":"Tag","assigned":null})),
            ("task_set_tag", json!({"tag":"Tag","assigned":"true"})),
            ("task_set_tag", json!({"assigned":true})),
            ("task_set_tag", json!({"tag":null,"assigned":true})),
            ("task_set_tag", json!({"tag":" ","assigned":true})),
            ("task_set_tag", json!({"tag":"a".repeat(41),"assigned":true})),
            ("task_set_tag", json!({"tag":"bad\u{0000}","assigned":false})),
        ];
        for (tool, mut args) in cases {
            args["task_id"] = json!("t");
            assert!(f.call(tool, args.clone(), None).await.is_err(), "{tool} {args}");
            assert_eq!(f.disk(), before);
        }
    }

    #[tokio::test]
    async fn thirteenth_tag_is_refused_without_leaking_a_catalog_entry() {
        let f = Fixture::new();
        for i in 1..=12 {
            f.call("task_set_tag", json!({"task_id":"t","tag":format!("Tag {i}"),"assigned":true}), None)
                .await
                .unwrap();
        }
        let before = f.disk();
        assert!(f.call("task_set_tag", json!({"task_id":"t","tag":"Excess","assigned":true}), None).await.is_err());
        assert_eq!(f.disk(), before);
        assert_eq!(f.app.state::<AppState>().config.read().tag_catalog.len(), 12);
    }

    #[tokio::test]
    async fn notifications_observe_durable_writes_and_skip_failures_and_idempotent_edits() {
        let f = Fixture::new();
        let events = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let received = events.clone();
        let path = f.root.join("config.json");
        f.app.listen("item-details:changed", move |event| {
            let disk: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            received.lock().push((serde_json::from_str::<Value>(event.payload()).unwrap(), disk));
        });
        f.call("task_set_tag", json!({"task_id":"t","tag":"Review","assigned":true}), None).await.unwrap();
        assert_eq!(events.lock()[0].0, json!({"kind":"task","catalog_changed":true}));
        assert_eq!(events.lock()[0].1["task_details"]["t"]["tags"], json!(["Review"]));
        assert_eq!(events.lock()[0].1["tag_catalog"], json!(["Review"]));
        std::fs::create_dir(f.root.join("config.json.tmp")).unwrap();
        for (tag, assigned) in [("review", true), ("absent", false)] {
            f.call("task_set_tag", json!({"task_id":"t","tag":tag,"assigned":assigned}), None).await.unwrap();
        }
        let before = f.disk();
        assert!(f.call("task_set_tag", json!({"task_id":"t","tag":"Refused","assigned":true}), None).await.is_err());
        assert!(f.call("task_rename", json!({"task_id":"t","name":"Refused"}), None).await.is_err());
        assert_eq!(events.lock().len(), 1);
        assert_eq!(f.disk(), before);
        assert_eq!(f.app.state::<AppState>().config.read().tag_catalog, ["Review"]);
        std::fs::remove_dir(f.root.join("config.json.tmp")).unwrap();
        f.call("task_set_tag", json!({"task_id":"t","tag":"review","assigned":false}), None).await.unwrap();
        assert_eq!(events.lock().len(), 2);
        assert_eq!(events.lock()[1].0, json!({"kind":"task","catalog_changed":false}));
        assert_eq!(events.lock()[1].1["task_details"], json!({}));
    }
}
