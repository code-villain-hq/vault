use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

use crate::config::item_details::{normalize_tag, tag_identity, ItemColor, ItemDetails};
use crate::config::{AppConfig, ConfigStore};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DetailsPatch {
    #[serde(default, deserialize_with = "present")]
    pub display_name: Option<Option<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub color: Option<Option<ItemColor>>,
}

fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone)]
pub enum DetailsTarget {
    Task(String),
    Chat(String),
}

#[derive(Debug)]
pub struct DetailsChange {
    pub details: ItemDetails,
    pub kind: &'static str,
    pub changed: bool,
    pub catalog_changed: bool,
}

#[derive(Debug)]
pub struct TagChange {
    pub tag: String,
    pub changed: bool,
    pub catalog_changed: bool,
}

/// Synchronous core for desktop and MCP callers, invoked through `blocking`.
pub fn edit_details(store: &ConfigStore, target: DetailsTarget, patch: DetailsPatch) -> Result<DetailsChange> {
    mutate(store, target, move |details| {
        if let Some(name) = patch.display_name {
            details.display_name = name;
        }
        if let Some(tags) = patch.tags {
            details.tags = tags;
        }
        if let Some(color) = patch.color {
            details.color = color;
        }
        Ok(())
    })
}

pub fn set_tag(store: &ConfigStore, target: DetailsTarget, tag: &str, assigned: bool) -> Result<DetailsChange> {
    mutate(store, target, |details| {
        let tag = required_tag(tag)?;
        let identity = tag_identity(&tag);
        if assigned {
            if !details.tags.iter().any(|old| tag_identity(old) == identity) {
                details.tags.push(tag);
            }
        } else {
            details.tags.retain(|old| tag_identity(old) != identity);
        }
        Ok(())
    })
}

pub fn add_tag(store: &ConfigStore, tag: &str) -> Result<TagChange> {
    store.edit_item_details(|cfg| {
        let tag = required_tag(tag)?;
        let before = cfg.tag_catalog.len();
        let tag = catalog_tag(&mut cfg.tag_catalog, tag);
        let changed = before != cfg.tag_catalog.len();
        Ok((TagChange { tag, changed, catalog_changed: changed }, changed))
    })
}

/// A rename or delete in Settings → Tags. It reaches every chat and work task
/// that has the tag, so the window refreshes the kinds it touched.
#[derive(Debug, Default)]
pub struct CatalogEdit {
    /// The tag's spelling after a rename; none after a delete.
    pub tag: Option<String>,
    pub changed: bool,
    pub tasks_changed: bool,
    pub chats_changed: bool,
}

/// Renaming onto another catalog tag merges the two under that tag's
/// spelling: two tags that differ only in case cannot both exist (SET-15).
pub fn rename_tag(store: &ConfigStore, from: &str, to: &str) -> Result<CatalogEdit> {
    store.edit_item_details(|cfg| {
        let index = catalog_index(&cfg.tag_catalog, from)?;
        let to = required_tag(to)?;
        let catalog_before = cfg.tag_catalog.clone();
        let old = tag_identity(&cfg.tag_catalog[index]);
        let target = tag_identity(&to);
        let tag = match cfg.tag_catalog.iter().position(|saved| tag_identity(saved) == target) {
            Some(other) if other != index => {
                let kept = cfg.tag_catalog[other].clone();
                cfg.tag_catalog.remove(index);
                kept
            }
            _ => {
                cfg.tag_catalog[index] = to.clone();
                to
            }
        };
        let tasks_changed = retag(&mut cfg.task_details, &old, Some(&tag));
        let chats_changed = retag(&mut cfg.chat_details, &old, Some(&tag));
        let changed = cfg.tag_catalog != catalog_before || tasks_changed || chats_changed;
        Ok((CatalogEdit { tag: Some(tag), changed, tasks_changed, chats_changed }, changed))
    })
}

/// Takes the tag off every chat and work task too: an assignment with no
/// catalog entry behind it could be neither offered nor renamed.
pub fn delete_tag(store: &ConfigStore, tag: &str) -> Result<CatalogEdit> {
    store.edit_item_details(|cfg| {
        let index = catalog_index(&cfg.tag_catalog, tag)?;
        let old = tag_identity(&cfg.tag_catalog.remove(index));
        let tasks_changed = retag(&mut cfg.task_details, &old, None);
        let chats_changed = retag(&mut cfg.chat_details, &old, None);
        Ok((CatalogEdit { tag: None, changed: true, tasks_changed, chats_changed }, true))
    })
}

fn catalog_index(catalog: &[String], raw: &str) -> Result<usize> {
    let identity = tag_identity(&required_tag(raw)?);
    catalog
        .iter()
        .position(|saved| tag_identity(saved) == identity)
        .ok_or_else(|| Error::NotFound(format!("Tag \"{}\". Refresh Settings and try again.", raw.trim())))
}

/// Swaps (or drops) one tag on every item, keeping each item's order and one
/// copy of each tag; an item left with no details at all is forgotten.
fn retag(map: &mut HashMap<String, ItemDetails>, identity: &str, to: Option<&str>) -> bool {
    let mut changed = false;
    map.retain(|_, details| {
        let mut next: Vec<String> = Vec::with_capacity(details.tags.len());
        for tag in &details.tags {
            let tag = match (tag_identity(tag) == identity, to) {
                (false, _) => tag.clone(),
                (true, Some(to)) => to.to_owned(),
                (true, None) => continue,
            };
            if !next.iter().any(|kept| tag_identity(kept) == tag_identity(&tag)) {
                next.push(tag);
            }
        }
        if next != details.tags {
            details.tags = next;
            changed = true;
        }
        *details != ItemDetails::default()
    });
    changed
}

fn required_tag(raw: &str) -> Result<String> {
    normalize_tag(raw)?.ok_or_else(|| Error::Other("Tag is blank. Enter a tag name.".into()))
}

fn catalog_tag(catalog: &mut Vec<String>, tag: String) -> String {
    let identity = tag_identity(&tag);
    if let Some(saved) = catalog.iter().find(|saved| tag_identity(saved) == identity) {
        return saved.clone();
    }
    catalog.push(tag.clone());
    tag
}

impl DetailsTarget {
    fn checked<'a>(&'a self, cfg: &AppConfig) -> Result<(&'static str, &'a str)> {
        match self {
            Self::Task(id) if cfg.tasks.iter().any(|task| task.id == *id) => Ok(("task", id)),
            Self::Chat(room)
                if cfg.saved_panes.iter().any(|pane| {
                    pane.task_id == super::CHAT_TASK_ID
                        && pane.kind == "agent"
                        && pane.checkout_id.is_none()
                        && pane.cwd.as_deref() == Some(room.as_str())
                        && !super::henchman::is_saved(pane)
                        && !routine_room(room)
                }) =>
            {
                Ok(("chat", room))
            }
            Self::Task(id) => Err(Error::NotFound(format!("Work task {id}. Refresh Work and try again."))),
            Self::Chat(_) => Err(Error::NotFound("Chat room. Refresh Chat and try again.".into())),
        }
    }
}

fn routine_room(room: &str) -> bool {
    // Check old roots too. The room is only a lookup key, never opened.
    std::path::Path::new(room).ancestors().any(|ancestor| {
        ancestor.file_name().is_some_and(|name| name == "routines")
            && ancestor.parent().and_then(std::path::Path::file_name).is_some_and(|name| name == "_chat")
    })
}

fn mutate(
    store: &ConfigStore,
    target: DetailsTarget,
    edit: impl FnOnce(&mut ItemDetails) -> Result<()>,
) -> Result<DetailsChange> {
    store.edit_item_details(|cfg| {
        let (kind, key) = target.checked(cfg)?;
        let before = match &target {
            DetailsTarget::Task(_) => cfg.task_details.get(key),
            DetailsTarget::Chat(_) => cfg.chat_details.get(key),
        }
        .cloned()
        .unwrap_or_default();
        let mut details = before.clone();
        edit(&mut details)?;
        let mut details = details.normalize()?;
        let catalog_before = cfg.tag_catalog.len();
        for tag in &mut details.tags {
            *tag = catalog_tag(&mut cfg.tag_catalog, std::mem::take(tag));
        }
        let catalog_changed = cfg.tag_catalog.len() != catalog_before;
        let changed = before != details || catalog_changed;
        let map = match target {
            DetailsTarget::Task(_) => &mut cfg.task_details,
            DetailsTarget::Chat(_) => &mut cfg.chat_details,
        };
        if details == ItemDetails::default() {
            map.remove(key);
        } else {
            map.insert(key.to_owned(), details.clone());
        }
        Ok((DetailsChange { details, kind, changed, catalog_changed }, changed))
    })
}

/// Only intentional removal clears details. Internal restarts and restores
/// replace saved IDs without passing here; Slack can reopen a closed room.
pub(crate) fn forget_saved_pane(c: &mut AppConfig, id: &str, waiting_only: bool) {
    let rooms: Vec<String> = c
        .saved_panes
        .iter()
        .filter(|p| p.id == id && (!waiting_only || p.waiting))
        .filter(|p| p.task_id == super::CHAT_TASK_ID && p.kind == "agent" && p.checkout_id.is_none())
        .filter_map(|p| p.cwd.clone())
        .collect();
    c.saved_panes.retain(|p| !(p.id == id && (!waiting_only || p.waiting)));
    for room in rooms {
        if (waiting_only || !c.slack_threads.iter().any(|t| t.room == room))
            && !c.saved_panes.iter().any(|p| p.cwd.as_ref() == Some(&room))
        {
            c.chat_details.remove(&room);
        }
    }
}

#[cfg(test)]
mod tests;
