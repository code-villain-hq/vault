//! The profile menu's commands (§25). Each works on `profiles.json` and the
//! profiles' folders, which every copy of the app shares, so each reads
//! them afresh rather than trusting what this copy saw last.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::{Error, Result};
use crate::profiles::{self, Entry, DEFAULT};

#[derive(Debug, Clone, Serialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    /// Open in a copy of the app, this one or another (PROF-3).
    pub open: bool,
    /// This window's.
    pub current: bool,
    /// Where its task folders go: what deleting it leaves behind (PROF-9).
    pub worktree_root: String,
    /// Its colour, as on its Dock icon (PROF-6).
    pub colour: usize,
    /// The server it lives on, by host; None on this Mac (PROF-15).
    pub server: Option<String>,
}

/// Every profile, Default first.
#[tauri::command]
pub async fn list_profiles(app: AppHandle) -> Result<Vec<ProfileView>> {
    super::blocking(app, |state| {
        let me = profiles::current();
        Ok(list_in(&me.base, &me.id, state.config.worktree_root()))
    })
    .await
}

pub(crate) fn list_in(base: &Path, me: &str, my_root: PathBuf) -> Vec<ProfileView> {
    profiles::read(base)
        .profiles
        .into_iter()
        .map(|p| {
            let folder = profiles::folder_of(base, &p.id);
            let current = p.id == me;
            let root = if current { my_root.clone() } else { root_in(&folder, &p.id) };
            ProfileView {
                colour: p.colour(),
                server: p.server.as_deref().map(crate::workspace::server::host),
                open: current || profiles::holder(&folder).is_some(),
                current,
                worktree_root: root.to_string_lossy().to_string(),
                id: p.id,
                name: p.name,
            }
        })
        .collect()
}

/// Another profile's task folder location: its setting, or its default.
fn root_in(folder: &Path, id: &str) -> PathBuf {
    std::fs::read_to_string(folder.join("config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|cfg| cfg["worktree_root"].as_str().filter(|s| !s.trim().is_empty()).map(PathBuf::from))
        .unwrap_or_else(|| profiles::root_of(id))
}

/// Open profile `id`: in a copy of its own, or, when one has it open,
/// that copy's window brought forward (PROF-3).
#[tauri::command]
pub async fn open_profile(id: String) -> Result<()> {
    super::off_runtime(move || {
        let me = profiles::current();
        if id == me.id {
            return Ok(());
        }
        if profiles::read(&me.base).get(&id).is_none() {
            return Err(Error::NotFound(format!("profile {id}")));
        }
        match profiles::holder(&profiles::folder_of(&me.base, &id)) {
            Some(pid) => {
                profiles::bring_forward(pid);
                Ok(())
            }
            None => profiles::launch(&id),
        }
    })
    .await?
}

/// Make a profile called `name`, empty (PROF-5), and open it.
#[tauri::command]
pub async fn create_profile(app: AppHandle, name: String) -> Result<ProfileView> {
    super::off_runtime(move || {
        let me = profiles::current();
        let made = create_in(&me.base, &name)?;
        profiles::launch(&made.id)?;
        changed(&app);
        Ok(made)
    })
    .await?
}

pub(crate) fn create_in(base: &Path, name: &str) -> Result<ProfileView> {
    let entry = profiles::change(base, |r| {
        let name = r.check_name(name, None)?;
        let entry = Entry::new(r.new_id(&name), name);
        r.profiles.push(entry.clone());
        Ok(entry)
    })?;
    std::fs::create_dir_all(profiles::folder_of(base, &entry.id))?;
    Ok(ProfileView {
        worktree_root: profiles::root_of(&entry.id).to_string_lossy().to_string(),
        colour: entry.colour(),
        id: entry.id,
        name: entry.name,
        open: false,
        current: false,
        server: None,
    })
}

/// Rename profile `id`. Its id, and so its folders, stay (PROF-8).
#[tauri::command]
pub async fn rename_profile(app: AppHandle, id: String, name: String) -> Result<()> {
    super::off_runtime(move || {
        rename_in(&profiles::current().base, &id, &name)?;
        changed(&app);
        Ok(())
    })
    .await?
}

pub(crate) fn rename_in(base: &Path, id: &str, name: &str) -> Result<()> {
    profiles::change(base, |r| {
        let name = r.check_name(name, Some(id))?;
        let entry = r.get_mut(id).ok_or_else(|| Error::NotFound(format!("profile {id}")))?;
        entry.name = name;
        Ok(())
    })
}

/// Give profile `id` colour `colour` of the eight (PROF-10).
#[tauri::command]
pub async fn set_profile_colour(app: AppHandle, id: String, colour: usize) -> Result<()> {
    super::off_runtime(move || {
        colour_in(&profiles::current().base, &id, colour)?;
        changed(&app);
        Ok(())
    })
    .await?
}

pub(crate) fn colour_in(base: &Path, id: &str, colour: usize) -> Result<()> {
    if colour >= profiles::COLOURS.len() {
        return Err(Error::Other(format!("There is no colour {colour}.")));
    }
    profiles::change(base, |r| {
        let entry = r.get_mut(id).ok_or_else(|| Error::NotFound(format!("profile {id}")))?;
        entry.colour = Some(colour);
        Ok(())
    })
}

/// Delete profile `id`: its folder and its tokens, never its task folders
/// (PROF-9). Off the command thread: the keychain can stop and ask.
#[tauri::command]
pub async fn delete_profile(app: AppHandle, id: String) -> Result<()> {
    super::off_runtime(move || {
        let me = profiles::current();
        let service = me.service_for(&id);
        delete_in(&me.base, &me.id, &id, || crate::secrets::forget_service(&service))?;
        changed(&app);
        Ok(())
    })
    .await?
}

pub(crate) fn delete_in(base: &Path, me: &str, id: &str, forget_tokens: impl FnOnce() -> Result<()>) -> Result<()> {
    let registry = profiles::read(base);
    let Some(entry) = registry.get(id) else {
        return Err(Error::NotFound(format!("profile {id}")));
    };
    if id == DEFAULT {
        return Err(Error::Other("The Default workspace cannot be deleted.".into()));
    }
    if id == me {
        return Err(Error::Other(format!("{} is this window's workspace. Delete it from another workspace's window.", entry.name)));
    }
    let folder = profiles::folder_of(base, id);
    // Held while it goes, so no copy can open it meanwhile.
    let Some(_held) = profiles::hold(&folder)? else {
        return Err(Error::Other(format!("{} is open. Quit it first.", entry.name)));
    };
    forget_tokens()?;
    std::fs::remove_dir_all(&folder)?;
    profiles::change(base, |r| {
        r.profiles.retain(|p| p.id != id);
        Ok(())
    })
}

/// The profiles changed: this window's title, Dock icon and menu say so.
pub(super) fn changed(app: &AppHandle) {
    profiles::show_identity(app);
    let _ = app.emit("profiles:changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vl-profile-cmds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_profile_starts_empty_with_task_folders_of_its_own() {
        let base = temp();
        let made = create_in(&base, "My Project").unwrap();
        assert_eq!(made.id, "my-project");
        assert!(base.join("profiles/my-project").is_dir());
        assert!(!base.join("profiles/my-project/config.json").exists(), "nothing carried over");
        assert!(made.worktree_root.ends_with(".villain-worktrees-my-project"));

        let listed = list_in(&base, DEFAULT, PathBuf::from("/mine"));
        assert_eq!(listed.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Default", "My Project"]);
        assert!(listed[0].current && listed[0].open && listed[0].worktree_root == "/mine");
        assert!(!listed[1].open && !listed[1].current);
        assert!(create_in(&base, "my project").is_err(), "names are unique ignoring case");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn another_profiles_chosen_task_folder_location_is_what_its_list_entry_says() {
        let base = temp();
        let made = create_in(&base, "Side").unwrap();
        std::fs::write(base.join("profiles/side/config.json"), r#"{"worktree_root":"/elsewhere"}"#).unwrap();
        let listed = list_in(&base, DEFAULT, PathBuf::from("/mine"));
        assert_eq!(listed.iter().find(|p| p.id == made.id).unwrap().worktree_root, "/elsewhere");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn renaming_keeps_the_id_and_so_every_folder() {
        let base = temp();
        create_in(&base, "Work").unwrap();
        create_in(&base, "Home").unwrap();
        rename_in(&base, "work", "ACME").unwrap();
        let registry = profiles::read(&base);
        assert_eq!(registry.get("work").unwrap().name, "ACME");
        assert!(rename_in(&base, "work", "home").is_err(), "taken by another");
        assert!(rename_in(&base, "nobody", "x").is_err());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_chosen_colour_is_kept_with_the_profile_and_only_one_of_the_eight() {
        let base = temp();
        let made = create_in(&base, "Side").unwrap();
        assert!(made.colour < 6, "one of the first six until one is chosen");
        colour_in(&base, "side", 7).unwrap();
        let listed = list_in(&base, DEFAULT, PathBuf::from("/mine"));
        assert_eq!(listed.iter().find(|p| p.id == "side").unwrap().colour, 7);
        rename_in(&base, "side", "Other name").unwrap();
        assert_eq!(profiles::read(&base).get("side").unwrap().colour(), 7, "renaming keeps it");
        assert!(colour_in(&base, "side", 8).is_err());
        assert!(colour_in(&base, "nobody", 1).is_err());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn deleting_refuses_default_this_window_and_an_open_profile() {
        let base = temp();
        create_in(&base, "Side").unwrap();
        let forgot = || Ok(());
        assert!(delete_in(&base, DEFAULT, DEFAULT, forgot).is_err());
        assert!(delete_in(&base, "side", "side", forgot).is_err());

        let open = profiles::hold(&base.join("profiles/side")).unwrap().unwrap();
        let err = delete_in(&base, DEFAULT, "side", forgot).unwrap_err().to_string();
        assert!(err.contains("open"), "{err}");
        assert!(base.join("profiles/side").is_dir());
        drop(open);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn deleting_a_profile_removes_its_folder_and_tokens_and_keeps_its_task_folders() {
        let base = temp();
        create_in(&base, "Side").unwrap();
        std::fs::write(base.join("profiles/side/config.json"), "{}").unwrap();
        let tasks = base.join("worktrees-side/TASK-1");
        std::fs::create_dir_all(&tasks).unwrap();

        let mut forgotten = false;
        delete_in(&base, DEFAULT, "side", || {
            forgotten = true;
            Ok(())
        })
        .unwrap();
        assert!(forgotten);
        assert!(!base.join("profiles/side").exists());
        assert!(profiles::read(&base).get("side").is_none());
        assert!(tasks.is_dir());

        // Tokens that cannot be forgotten leave the profile as it was.
        create_in(&base, "Other").unwrap();
        assert!(delete_in(&base, DEFAULT, "other", || Err(Error::Other("denied".into()))).is_err());
        assert!(base.join("profiles/other").is_dir() && profiles::read(&base).get("other").is_some());
        std::fs::remove_dir_all(&base).ok();
    }
}
