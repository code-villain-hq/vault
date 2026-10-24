//! Durable handoff between the backend's files and the window's IndexedDB.
use serde::Serialize;
use serde_json::Value;

use crate::commands::AppState;
use crate::error::{Error, Result};

const ID: &str = "_handoff_id";

#[derive(Debug, Serialize, PartialEq)]
pub struct Write {
    pub id: String,
    pub operation: Value,
}

/// Upgrade old queued operations when they are first read, without changing
/// their wire representation. The id is saved before returning it to IPC.
pub fn identify(queue: &mut [Value]) -> bool {
    let mut changed = false;
    for op in queue {
        if op.get(ID).and_then(Value::as_str).is_none() {
            if let Some(object) = op.as_object_mut() {
                object.insert(ID.into(), Value::String(uuid::Uuid::now_v7().to_string()));
                changed = true;
            }
        }
    }
    changed
}

pub fn writes(queue: &[Value]) -> Result<Vec<Write>> {
    queue.iter().map(|op| {
        let mut operation = op.clone();
        let id = operation.as_object_mut().and_then(|o| o.remove(ID))
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| Error::Other("A queued change could not be read. Its saved copy is kept.".into()))?;
        Ok(Write { id, operation })
    }).collect()
}

pub fn acknowledge(queue: &mut Vec<Value>, ids: &[String]) -> bool {
    let ids: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    let before = queue.len();
    queue.retain(|op| !op.get(ID).and_then(Value::as_str).is_some_and(|id| ids.contains(id)));
    queue.len() != before
}

pub fn read(state: &AppState) -> Result<Vec<Write>> {
    state.config.mirror(|c| identify(&mut c.shared.outbox))?;
    let mut queued = state.config.with(|c| writes(&c.shared.outbox))?;
    queued.extend(state.notes.read_writes()?);
    Ok(queued)
}

pub fn ack(state: &AppState, ids: &[String]) -> Result<()> {
    state.config.mirror(|c| acknowledge(&mut c.shared.outbox, ids))?;
    state.notes.ack_writes(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::moving::{move_in, tests::{temp_dir, v2_profile}};

    #[test]
    fn reads_survive_restart_and_acks_leave_newer_changes_queued() {
        let root = temp_dir();
        let state = v2_profile(&root);
        move_in(&state, uuid::Uuid::now_v7()).unwrap();
        let first = read(&state).unwrap();
        assert!(!first.is_empty());
        assert_eq!(read(&state).unwrap(), first);
        // Reload the durable files as a restarted process would.
        let cfg: crate::config::AppConfig = serde_json::from_slice(&std::fs::read(root.join("config.json")).unwrap()).unwrap();
        let stored = writes(&cfg.shared.outbox).unwrap();
        assert_eq!(&first[..stored.len()], stored);
        assert_eq!(crate::notes::Notes::load(&root).read_writes().unwrap(), state.notes.read_writes().unwrap());
        state.config.update(|c| c.shared.outbox.push(serde_json::json!({"action":"delete","model":"Note","id":"later"}))).unwrap();
        ack(&state, &first.iter().map(|w| w.id.clone()).collect::<Vec<_>>()).unwrap();
        let remaining = read(&state).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].operation["id"], "later");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_failed_notes_handoff_keeps_the_configs_queue() {
        let root = temp_dir();
        let state = v2_profile(&root);
        move_in(&state, uuid::Uuid::now_v7()).unwrap();
        let count = state.config.read().shared.outbox.len();
        let notes = root.join("notes.json");
        std::fs::rename(&notes, root.join("notes.saved")).unwrap();
        std::fs::create_dir(&notes).unwrap();
        assert!(read(&state).is_err());
        assert_eq!(state.config.read().shared.outbox.len(), count);
        std::fs::remove_dir(&notes).unwrap();
        std::fs::rename(root.join("notes.saved"), &notes).unwrap();
        assert!(read(&state).unwrap().len() > count);
        std::fs::remove_dir_all(root).unwrap();
    }
}
