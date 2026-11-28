#[cfg(test)]
mod cases {
    use super::super::*;
    use crate::config::{AppConfig, SavedPane, Task};
    use serde_json::json;

    struct Fixture {
        dir: std::path::PathBuf,
        store: ConfigStore,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("layer-item-details-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let cfg: AppConfig = serde_json::from_value(json!({
            "version": 4,
            "tasks": [{"id":"t","name":"Source","root":"/work/t","branch":"original","created_at":"2026-01-01T00:00:00Z"}],
            "saved_panes": [{"id":"p","task_id":"chat","kind":"agent","cwd":"/old/_chat/abcd1234"}]
        })).unwrap();
            let store = ConfigStore::for_tests(dir.join("config.json"), cfg);
            store.update(|_| ()).unwrap();
            Self { dir, store }
        }

        fn disk(&self) -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(self.dir.join("config.json")).unwrap()).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn task() -> DetailsTarget {
        DetailsTarget::Task("t".into())
    }

    fn chat() -> DetailsTarget {
        DetailsTarget::Chat("/old/_chat/abcd1234".into())
    }

    fn patch(value: serde_json::Value) -> DetailsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn omitted_patch_fields_preserve_and_explicit_null_clears() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"display_name":" Custom ","color":"data-3","tags":["Review"]})))
            .unwrap();
        let changed = edit_details(&f.store, task(), patch(json!({"display_name":null}))).unwrap();
        assert_eq!(changed.details.display_name, None);
        assert_eq!(changed.details.color, Some(ItemColor::Data3));
        assert_eq!(changed.details.tags, ["Review"]);
        assert!(changed.changed);
        assert!(!changed.catalog_changed);
        let cleared = edit_details(&f.store, task(), patch(json!({"color":null,"tags":[]}))).unwrap();
        assert_eq!(cleared.details, ItemDetails::default());
        assert!(f.disk()["task_details"].as_object().unwrap().is_empty());
        assert_eq!(f.disk()["tag_catalog"], json!(["Review"]));
        assert!(serde_json::from_value::<DetailsPatch>(json!({"tags":null})).is_ok());
    }

    #[test]
    fn old_configs_load_empty_details_and_edits_survive_reload_without_touching_records() {
        let f = Fixture::new();
        let before = f.disk();
        assert!(before.get("task_details").is_none_or(|v| v.as_object().unwrap().is_empty()));
        let result = edit_details(&f.store, task(), patch(json!({"display_name":" Alias "}))).unwrap();
        assert_eq!(result.details.display_name.as_deref(), Some("Alias"));
        let disk = f.disk();
        assert_eq!(disk["task_details"]["t"]["display_name"], "Alias");
        for key in ["tasks", "saved_panes", "shared", "checkouts", "ui"] {
            assert_eq!(before[key], disk[key], "{key}");
        }
        let loaded: AppConfig = serde_json::from_value(disk).unwrap();
        let reloaded = ConfigStore::for_tests(f.dir.join("config.json"), loaded);
        let unchanged = edit_details(&reloaded, task(), DetailsPatch::default()).unwrap();
        assert_eq!(unchanged.details.display_name.as_deref(), Some("Alias"));
        assert!(!unchanged.changed);
    }

    #[test]
    fn catalog_reuses_first_saved_spelling_across_tasks_and_chats_but_not_profiles() {
        let f = Fixture::new();
        let other = Fixture::new();
        let added = add_tag(&f.store, " Review ").unwrap();
        assert_eq!(added.tag, "Review");
        assert!(added.changed && added.catalog_changed);
        let reused = add_tag(&f.store, "REVIEW").unwrap();
        assert_eq!(reused.tag, "Review");
        assert!(!reused.changed && !reused.catalog_changed);
        let task_result = edit_details(&f.store, task(), patch(json!({"tags":["review","Fresh","REVIEW"]}))).unwrap();
        assert_eq!(task_result.details.tags, ["Review", "Fresh"]);
        assert!(task_result.catalog_changed);
        let chat_result = edit_details(&f.store, chat(), patch(json!({"tags":["FRESH","review"]}))).unwrap();
        assert_eq!(chat_result.kind, "chat");
        assert_eq!(chat_result.details.tags, ["Fresh", "Review"]);
        assert!(!chat_result.catalog_changed);
        assert_eq!(f.disk()["tag_catalog"], json!(["Review", "Fresh"]));
        assert!(other.disk().get("tag_catalog").is_none_or(|v| v.as_array().unwrap().is_empty()));
    }

    #[test]
    fn concurrent_field_edits_merge_the_latest_details_under_the_disk_lock() {
        let f = Fixture::new();
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            for value in [json!({"display_name":"Alias"}), json!({"color":"data-6"}), json!({"tags":["Focus"]})] {
                let store = &f.store;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    edit_details(store, task(), patch(value)).unwrap();
                });
            }
        });
        assert_eq!(f.disk()["task_details"]["t"], json!({"display_name":"Alias","color":"data-6","tags":["Focus"]}));
    }

    #[test]
    fn concurrent_catalog_and_assignment_writes_keep_one_canonical_spelling() {
        let f = Fixture::new();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                add_tag(&f.store, "REVIEW").unwrap();
            });
            scope.spawn(|| {
                barrier.wait();
                set_tag(&f.store, task(), "Review", true).unwrap();
            });
        });
        let disk = f.disk();
        let catalog = disk["tag_catalog"].as_array().unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(disk["task_details"]["t"]["tags"], json!(catalog));
    }

    #[test]
    fn single_tag_assignment_preserves_fields_and_removal_does_not_create_catalog_entries() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"display_name":"Alias","color":"data-2"}))).unwrap();
        let added = set_tag(&f.store, task(), " First ", true).unwrap();
        assert_eq!(added.details.tags, ["First"]);
        assert!(added.catalog_changed);
        let duplicate = set_tag(&f.store, task(), "FIRST", true).unwrap();
        assert!(!duplicate.changed && !duplicate.catalog_changed);
        let absent = set_tag(&f.store, task(), "Unused", false).unwrap();
        assert!(!absent.changed && !absent.catalog_changed);
        let removed = set_tag(&f.store, task(), "first", false).unwrap();
        assert!(removed.details.tags.is_empty());
        assert_eq!(removed.details.display_name.as_deref(), Some("Alias"));
        assert_eq!(removed.details.color, Some(ItemColor::Data2));
        assert_eq!(f.disk()["tag_catalog"], json!(["First"]));
        assert!(set_tag(&f.store, task(), " ", true).is_err());
        assert!(set_tag(&f.store, task(), "\nFirst", false).is_err());
        assert!(add_tag(&f.store, " ").is_err());
    }

    #[test]
    fn invalid_assignment_rolls_back_the_whole_item_and_catalog() {
        let f = Fixture::new();
        let before = f.disk();
        assert!(edit_details(&f.store, task(), patch(json!({"display_name":"Alias","tags":["New","bad\n"]}))).is_err());
        assert_eq!(f.disk(), before);
        let tags: Vec<_> = (1..=12).map(|i| format!("Tag {i}")).collect();
        edit_details(&f.store, task(), DetailsPatch { tags: Some(tags), ..Default::default() }).unwrap();
        let before = f.disk();
        assert!(set_tag(&f.store, task(), "Thirteenth", true).is_err());
        assert_eq!(f.disk(), before);
    }

    #[test]
    fn missing_or_ineligible_targets_are_not_recreated_even_with_an_empty_patch() {
        let f = Fixture::new();
        for (room, task_id, kind, checkout) in [
            ("/old/_chat/henchman", "chat", "agent", None),
            ("/old/_chat/routines/job/run", "chat", "agent", None),
            ("/work/t", "t", "agent", None),
            ("/old/_chat/shell", "chat", "shell", None),
            ("/old/_chat/owned", "chat", "agent", Some("checkout")),
        ] {
            let pane: SavedPane = serde_json::from_value(json!({
                "id":room,"task_id":task_id,"kind":kind,"cwd":room,"checkout_id":checkout
            }))
            .unwrap();
            f.store.update(|cfg| cfg.saved_panes.push(pane)).unwrap();
            assert!(
                edit_details(&f.store, DetailsTarget::Chat(room.into()), DetailsPatch::default()).is_err(),
                "{room}"
            );
        }
        for target in [DetailsTarget::Task("missing".into()), DetailsTarget::Chat("/not/saved".into())] {
            assert!(edit_details(&f.store, target, patch(json!({"tags":["New"]}))).is_err());
        }
        assert!(set_tag(&f.store, DetailsTarget::Task("missing".into()), "New", false).is_err());
        f.store.update(|cfg| cfg.tasks.clear()).unwrap();
        assert!(edit_details(&f.store, task(), DetailsPatch::default()).is_err());
        assert!(f.disk().get("tag_catalog").is_none_or(|v| v.as_array().unwrap().is_empty()));
    }

    #[test]
    fn waiting_chat_details_use_the_saved_room_not_the_pane_id() {
        let f = Fixture::new();
        edit_details(&f.store, chat(), patch(json!({"display_name":"Chat alias"}))).unwrap();
        f.store
            .update(|cfg| {
                cfg.saved_panes[0].id = "new-pane".into();
                cfg.saved_panes[0].waiting = true;
            })
            .unwrap();
        let result = edit_details(&f.store, chat(), DetailsPatch::default()).unwrap();
        assert_eq!(result.details.display_name.as_deref(), Some("Chat alias"));
        assert!(!result.changed);
        assert_eq!(f.disk()["chat_details"]["/old/_chat/abcd1234"]["display_name"], "Chat alias");
    }

    #[test]
    fn refused_disk_writes_publish_neither_item_details_nor_catalog_changes() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"display_name":"Before"}))).unwrap();
        let before = serde_json::to_value(f.store.read()).unwrap();
        std::fs::create_dir(f.dir.join("config.json.tmp")).unwrap();
        assert!(edit_details(&f.store, task(), patch(json!({"display_name":"After","tags":["New"]}))).is_err());
        assert!(add_tag(&f.store, "Catalog only").is_err());
        assert_eq!(serde_json::to_value(f.store.read()).unwrap(), before);
        assert_eq!(f.disk(), before);
    }

    #[test]
    fn unchanged_edits_do_not_require_a_disk_write_or_report_a_change() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"display_name":"Alias","tags":["Review"]}))).unwrap();
        std::fs::create_dir(f.dir.join("config.json.tmp")).unwrap();
        let result =
            edit_details(&f.store, task(), patch(json!({"display_name":" Alias ","tags":["review"]}))).unwrap();
        assert_eq!(result.details.display_name.as_deref(), Some("Alias"));
        assert_eq!(result.kind, "task");
        assert!(!result.changed && !result.catalog_changed);
        assert!(!add_tag(&f.store, "REVIEW").unwrap().changed);
    }

    #[test]
    fn stored_epics_and_subtasks_are_eligible_without_modifying_source_names() {
        let f = Fixture::new();
        f.store
            .update(|cfg| {
                let mut epic: Task = cfg.tasks[0].clone();
                epic.id = "epic".into();
                epic.epic = true;
                cfg.tasks.push(epic);
                cfg.tasks[0].sub_task =
                    Some(crate::config::SubTaskOf { epic: "epic".into(), title: "Spec title".into() });
            })
            .unwrap();
        for id in ["t", "epic"] {
            assert!(
                edit_details(&f.store, DetailsTarget::Task(id.into()), patch(json!({"display_name":"Custom"})))
                    .unwrap()
                    .changed
            );
        }
        assert_eq!(f.disk()["tasks"][0]["name"], "Source");
        assert_eq!(f.disk()["tasks"][0]["sub_task"]["title"], "Spec title");
    }

    #[test]
    fn renaming_a_tag_renames_it_on_every_chat_and_task_and_merges_onto_an_existing_spelling() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"tags":["Review","Bug"]}))).unwrap();
        edit_details(&f.store, chat(), patch(json!({"tags":["review"]}))).unwrap();
        let renamed = rename_tag(&f.store, "REVIEW", " Needs review ").unwrap();
        assert_eq!(renamed.tag.as_deref(), Some("Needs review"));
        assert!(renamed.changed && renamed.tasks_changed && renamed.chats_changed);
        let disk = f.disk();
        assert_eq!(disk["tag_catalog"], json!(["Needs review", "Bug"]));
        assert_eq!(disk["task_details"]["t"]["tags"], json!(["Needs review", "Bug"]));
        assert_eq!(disk["chat_details"]["/old/_chat/abcd1234"]["tags"], json!(["Needs review"]));
        let merged = rename_tag(&f.store, "Bug", "needs REVIEW").unwrap();
        assert_eq!(merged.tag.as_deref(), Some("Needs review"));
        assert!(merged.tasks_changed && !merged.chats_changed);
        let disk = f.disk();
        assert_eq!(disk["tag_catalog"], json!(["Needs review"]));
        assert_eq!(disk["task_details"]["t"]["tags"], json!(["Needs review"]));
        let cased = rename_tag(&f.store, "needs review", "NEEDS REVIEW").unwrap();
        assert!(cased.changed);
        assert_eq!(f.disk()["tag_catalog"], json!(["NEEDS REVIEW"]));
        let same = rename_tag(&f.store, "needs review", "NEEDS REVIEW").unwrap();
        assert!(!same.changed);
        assert!(rename_tag(&f.store, "Missing", "Other").is_err());
        assert!(rename_tag(&f.store, "NEEDS REVIEW", " ").is_err());
    }

    #[test]
    fn deleting_a_tag_takes_it_off_every_item_and_forgets_items_left_with_nothing() {
        let f = Fixture::new();
        edit_details(&f.store, task(), patch(json!({"tags":["Review"],"display_name":"Alias"}))).unwrap();
        edit_details(&f.store, chat(), patch(json!({"tags":["Review"]}))).unwrap();
        add_tag(&f.store, "Unused").unwrap();
        let deleted = delete_tag(&f.store, "review").unwrap();
        assert!(deleted.changed && deleted.tasks_changed && deleted.chats_changed);
        let disk = f.disk();
        assert_eq!(disk["tag_catalog"], json!(["Unused"]));
        assert_eq!(disk["task_details"]["t"]["display_name"], "Alias");
        assert_eq!(disk["task_details"]["t"]["tags"], json!([]));
        assert!(disk["chat_details"].get("/old/_chat/abcd1234").is_none());
        let unused = delete_tag(&f.store, "Unused").unwrap();
        assert!(unused.changed && !unused.tasks_changed && !unused.chats_changed);
        assert!(delete_tag(&f.store, "Unused").is_err());
    }
}
