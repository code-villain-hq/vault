use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::item_details::{self, DetailsChange, DetailsPatch, DetailsTarget};
use super::AppState;
use crate::config::item_details::ItemDetails;
use crate::error::Result;

/// Desktop-only fields must never enter the raw MCP and phone payloads.
#[derive(Debug, Serialize)]
pub struct DesktopItem<T: Serialize> {
    #[serde(flatten)]
    pub item: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ItemDetails>,
}

pub(crate) fn project<T: Serialize>(item: T, details: Option<ItemDetails>) -> DesktopItem<T> {
    DesktopItem { item, details }
}

pub(crate) fn project_waiting(state: &AppState) -> Vec<DesktopItem<super::panes_restore::WaitingPane>> {
    let panes = super::panes_restore::waiting_panes_inner(state);
    state.config.with(|c| {
        panes
            .into_iter()
            .map(|p| {
                let details = p.cwd.as_ref().and_then(|room| c.chat_details.get(room)).cloned();
                project(p, details)
            })
            .collect()
    })
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DetailsEvent {
    pub kind: &'static str,
    pub catalog_changed: bool,
}

/// Call only with the successful durable mutation result, never before persistence.
/// Event delivery is best effort; a closed webview must not turn a saved edit into an error.
pub(crate) fn notify_details<R: tauri::Runtime>(app: &AppHandle<R>, change: &DetailsChange) {
    notify(app, change.kind, change.changed, change.catalog_changed);
}

fn notify<R: tauri::Runtime>(app: &AppHandle<R>, kind: &'static str, changed: bool, catalog_changed: bool) {
    if changed {
        let _ = app.emit("item-details:changed", DetailsEvent { kind, catalog_changed });
    }
}

#[tauri::command]
pub async fn set_task_details(app: AppHandle, task_id: String, patch: DetailsPatch) -> Result<ItemDetails> {
    let handle = app.clone();
    super::blocking(app, move |state| {
        let change = item_details::edit_details(&state.config, DetailsTarget::Task(task_id), patch)?;
        notify_details(&handle, &change);
        Ok(change.details)
    })
    .await
}

#[tauri::command]
pub async fn set_chat_details(app: AppHandle, room: String, patch: DetailsPatch) -> Result<ItemDetails> {
    let handle = app.clone();
    super::blocking(app, move |state| {
        let change = item_details::edit_details(&state.config, DetailsTarget::Chat(room), patch)?;
        notify_details(&handle, &change);
        Ok(change.details)
    })
    .await
}

#[tauri::command]
pub fn list_tags(state: State<AppState>) -> Vec<String> {
    state.config.with(|c| c.tag_catalog.clone())
}

#[tauri::command]
pub async fn add_tag(app: AppHandle, tag: String) -> Result<String> {
    let handle = app.clone();
    super::blocking(app, move |state| {
        let change = item_details::add_tag(&state.config, &tag)?;
        notify(&handle, "catalog", change.changed, change.catalog_changed);
        Ok(change.tag)
    })
    .await
}

#[tauri::command]
pub async fn rename_tag(app: AppHandle, tag: String, to: String) -> Result<String> {
    let handle = app.clone();
    super::blocking(app, move |state| {
        let edit = item_details::rename_tag(&state.config, &tag, &to)?;
        notify_catalog(&handle, &edit);
        Ok(edit.tag.unwrap_or(to))
    })
    .await
}

#[tauri::command]
pub async fn delete_tag(app: AppHandle, tag: String) -> Result<()> {
    let handle = app.clone();
    super::blocking(app, move |state| {
        let edit = item_details::delete_tag(&state.config, &tag)?;
        notify_catalog(&handle, &edit);
        Ok(())
    })
    .await
}

/// One event per kind of item the edit reached, so the window reloads those.
fn notify_catalog<R: tauri::Runtime>(app: &AppHandle<R>, edit: &item_details::CatalogEdit) {
    notify(app, "task", edit.tasks_changed, true);
    notify(app, "chat", edit.chats_changed, true);
    notify(app, "catalog", edit.changed && !edit.tasks_changed && !edit.chats_changed, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tasks::TaskView;
    use crate::config::Task;
    use crate::pty::{Activity, PaneInfo, PaneKind};
    use serde_json::json;
    use tauri::Listener;

    fn details() -> ItemDetails {
        ItemDetails { display_name: Some("Alias".into()), tags: vec!["Review".into()], color: None }
    }

    #[test]
    fn desktop_projection_adds_details_without_changing_raw_task_or_mcp_task_view() {
        let task: Task = serde_json::from_value(json!({
            "id":"t", "name":"Original", "root":"/work/t", "branch":"original",
            "created_at":"2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert!(serde_json::to_value(&task).unwrap().get("details").is_none());
        let raw = TaskView { task, checkouts: vec![], pane_count: 0 };
        let before = serde_json::to_value(&raw).unwrap();
        assert!(before.get("details").is_none());
        let mut desktop = serde_json::to_value(project(raw, Some(details()))).unwrap();
        assert_eq!(desktop["details"]["display_name"], "Alias");
        desktop.as_object_mut().unwrap().remove("details");
        assert_eq!(desktop, before);
    }

    #[test]
    fn desktop_projection_preserves_raw_pane_payload_and_omits_absent_details() {
        let raw = PaneInfo {
            id: "p".into(),
            task_id: "chat".into(),
            checkout_id: None,
            kind: PaneKind::Agent,
            title: "Original".into(),
            agent_id: Some("claude".into()),
            cwd: "/room".into(),
            running: true,
            exit_code: None,
            started_at: chrono::Utc::now(),
            last_output_at: chrono::Utc::now(),
            notice: None,
            activity: Activity::Idle,
            activity_since: chrono::Utc::now(),
            topic: None,
            acp: true,
            loop_said: None,
        };
        let before = serde_json::to_value(&raw).unwrap();
        assert!(before.get("details").is_none());
        assert_eq!(serde_json::to_value(project(raw.clone(), None)).unwrap(), before);
        let desktop = serde_json::to_value(project(raw, Some(details()))).unwrap();
        assert_eq!(desktop["title"], "Original");
        assert_eq!(desktop["details"]["tags"], json!(["Review"]));
    }

    #[test]
    fn notifications_follow_durable_changes_and_skip_no_ops_and_failed_writes() {
        let root = std::env::temp_dir().join(format!("layer-details-event-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let cfg = serde_json::from_value(json!({
            "tasks": [{"id":"t", "name":"Original", "root":"/work/t", "branch":"original",
                       "created_at":"2026-01-01T00:00:00Z"}]
        }))
        .unwrap();
        let store = crate::config::ConfigStore::for_tests(root.join("config.json"), cfg);
        store.update(|_| ()).unwrap();
        let app = tauri::test::mock_app();
        let events = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let received = events.clone();
        let path = root.join("config.json");
        app.listen("item-details:changed", move |event| {
            let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(disk["task_details"]["t"]["tags"], json!(["Review"]));
            received.lock().push(serde_json::from_str::<serde_json::Value>(event.payload()).unwrap());
        });
        let patch = serde_json::from_value(json!({"tags":["Review"]})).unwrap();
        let target = DetailsTarget::Task("t".into());
        let changed = item_details::edit_details(&store, target.clone(), patch).unwrap();
        notify_details(app.handle(), &changed);
        let unchanged = item_details::edit_details(&store, target.clone(), DetailsPatch::default()).unwrap();
        notify_details(app.handle(), &unchanged);
        assert_eq!(*events.lock(), vec![json!({"kind":"task","catalog_changed":true})]);
        // Config persists via this sibling temporary path; refusal must publish no event.
        std::fs::create_dir(root.join("config.json.tmp")).unwrap();
        let patch = serde_json::from_value(json!({"display_name":"Refused"})).unwrap();
        let failed = item_details::edit_details(&store, target, patch);
        assert!(failed.is_err());
        if let Ok(change) = failed {
            notify_details(app.handle(), &change);
        }
        assert_eq!(events.lock().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
