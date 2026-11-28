//! Screenshots an agent wants in a pull request, held until the user adds
//! them from the task's Pull requests tab (PR-13).
//!
//! `pr_images` used to publish on the agent's own `confirm: true`. Claude
//! Code's auto mode refused that call as a possible leak when the user's
//! yes was only a "go on" in the chat, and the agent was left asking the
//! user to change its permissions. Offered here, the call publishes
//! nothing, and the yes is a click by the user on the images themselves.

use std::collections::HashMap;
use std::path::Path;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::Result;

use super::off_runtime;
use super::pr_images::{read_image, DraftImage};

/// What each task's agent last offered, by task id. Kept in memory: an offer
/// the app forgets on a restart is one the agent can make again.
static OFFERS: Mutex<Option<HashMap<String, Offer>>> = Mutex::new(None);

#[derive(Debug, Clone, Default, Serialize)]
pub struct Offer {
    /// The repository whose open pull request they are for.
    pub repo: String,
    /// The images, in order, each with its preview when read for the window.
    pub images: Vec<DraftImage>,
}

/// Hold `images` for `task_id`, in place of what it offered before, as a
/// later `pr_images` call replaces the PR's images, and tell the window.
pub(crate) fn offer(app: &AppHandle, task_id: &str, offer: Offer) {
    OFFERS.lock().get_or_insert_with(HashMap::new).insert(task_id.to_string(), offer);
    let _ = app.emit("pr:images-offered", task_id);
}

/// What `task_id` has on offer, each image read again for its preview: a
/// file gone or changed since is left out rather than shown stale.
#[tauri::command]
pub async fn pr_image_offer(task_id: String) -> Result<Option<Offer>> {
    let Some(held) = OFFERS.lock().as_ref().and_then(|o| o.get(&task_id).cloned()) else { return Ok(None) };
    off_runtime(move || {
        let images = held
            .images
            .into_iter()
            .filter_map(|i| {
                let caption = Some(i.caption.as_str()).filter(|c| !c.is_empty());
                let read = read_image(Path::new(&i.path), caption).ok()?;
                Some(DraftImage { preview: super::pr_images::preview(&read), caption: read.caption, ..i })
            })
            .collect();
        Some(Offer { repo: held.repo, images })
    })
    .await
}

/// Forget what `task_id` has on offer: added, or turned down.
#[tauri::command]
pub fn drop_pr_image_offer(task_id: String) {
    if let Some(offers) = OFFERS.lock().as_mut() {
        offers.remove(&task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offer_read_back_shows_its_images_and_drops_a_file_that_went_away() {
        let dir = std::env::temp_dir().join(format!("vl-offer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let night = dir.join("night.png");
        std::fs::write(&night, b"\x89PNG\r\n\x1a\nnight").unwrap();
        let image = |p: &Path, caption: &str| DraftImage {
            checkout_id: None,
            path: p.to_string_lossy().into_owned(),
            caption: caption.into(),
            repo: Some("web".into()),
            preview: String::new(),
        };
        let task = format!("t-{}", uuid::Uuid::new_v4());
        OFFERS.lock().get_or_insert_with(HashMap::new).insert(
            task.clone(),
            Offer { repo: "web".into(), images: vec![image(&night, "Night"), image(&dir.join("gone.png"), "Day")] },
        );

        let read = tauri::async_runtime::block_on(pr_image_offer(task.clone())).unwrap().unwrap();
        assert_eq!(read.images.len(), 1);
        assert_eq!(read.images[0].caption, "Night");
        assert!(read.images[0].preview.starts_with("data:image/png;base64,"));

        drop_pr_image_offer(task.clone());
        assert!(tauri::async_runtime::block_on(pr_image_offer(task)).unwrap().is_none());
    }
}
