//! Profiles (§25): a whole Layer each, side by side.
//!
//! A profile is a folder: `config.json` and the rest of what the app saves,
//! plus a keychain item of its own. Each open profile is its own process,
//! started with `--profile <id>`, so a crash in one never takes another's
//! agents down (PROF-2). This process is exactly one profile, chosen in
//! `start` before the window exists, and holds its folder's `running.lock`
//! until it exits: two copies over one `config.json` erase each other's
//! tasks (PROF-3).
//!
//! `profiles.json`, beside the Default profile's files, names the profiles
//! and remembers when each opened and quit, for the next launch (PROF-7).
//! Every copy writes it, so a change holds `profiles.lock` across the
//! read and the write.

mod mac;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub use mac::{bring_forward, mark_dock, COLOURS};

/// The profile that was there before profiles were (PROF-4).
pub const DEFAULT: &str = "default";

/// How long before the last profile to quit another may have quit and
/// still count as open with it, at the next launch (PROF-7). Logging out
/// quits every copy within a second or two of each other.
const TOGETHER: Duration = Duration::seconds(30);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub profiles: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub opened_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub quit_at: Option<DateTime<Utc>>,
    /// The colour chosen for it (PROF-10), an index into `COLOURS`.
    #[serde(default)]
    pub colour: Option<usize>,
    /// The server it lives on, by origin, and its id there (PROF-15). None
    /// for a workspace on this Mac.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

impl Entry {
    pub fn new(id: String, name: String) -> Self {
        Self { id, name, opened_at: None, quit_at: None, colour: None, server: None, workspace: None }
    }

    /// Its colour: the one chosen, or the one its id gives it.
    pub fn colour(&self) -> usize {
        self.colour.filter(|c| *c < COLOURS.len()).unwrap_or_else(|| mac::default_colour(&self.id))
    }
}

impl Registry {
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    /// Default is always there, first, even before `profiles.json` is.
    fn with_default(mut self) -> Self {
        if self.get(DEFAULT).is_none() {
            self.profiles.insert(0, Entry::new(DEFAULT.into(), "Default".into()));
        }
        self
    }

    /// `name`, trimmed, if no other profile than `except` has it (PROF-8).
    pub fn check_name(&self, name: &str, except: Option<&str>) -> Result<String> {
        let name = name.trim();
        let length = name.chars().count();
        if length == 0 || length > 40 {
            return Err(Error::Other("A workspace's name is 1 to 40 characters.".into()));
        }
        if name.chars().any(char::is_control) {
            return Err(Error::Other("A workspace's name is one line of text.".into()));
        }
        let taken = self
            .profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == name.to_lowercase());
        if taken {
            return Err(Error::Other(format!("There is a workspace called {name} already.")));
        }
        Ok(name.to_string())
    }

    /// An id made from `name` that no profile has: lower case letters,
    /// digits and dashes, so it is safe as a folder name and a keychain
    /// service.
    pub fn new_id(&self, name: &str) -> String {
        let mut slug = String::new();
        for c in name.trim().to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug: String = slug.trim_end_matches('-').chars().take(24).collect();
        let slug = match slug.trim_end_matches('-') {
            "" => "profile".to_string(),
            s => s.to_string(),
        };
        let free = |id: &str| id != DEFAULT && self.get(id).is_none();
        if free(&slug) {
            return slug;
        }
        (2..).map(|n| format!("{slug}-{n}")).find(|id| free(id)).unwrap_or(slug)
    }
}

/// Whether `id` could be a profile's folder name: what `new_id` makes. A
/// profile's id is only used once found in the registry, and this is the
/// second fence, should the file have been edited by hand.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The Default profile's config folder, which holds everything else
/// (PROF-4). Where Tauri's `app_config_dir` is on a Mac, worked out before
/// there is an app to ask.
pub fn base_dir(identifier: &str) -> PathBuf {
    #[allow(deprecated)]
    let home = std::env::home_dir().unwrap_or_else(std::env::temp_dir);
    home.join("Library/Application Support").join(identifier)
}

/// A profile's own folder.
pub fn folder_of(base: &Path, id: &str) -> PathBuf {
    if id == DEFAULT {
        base.to_path_buf()
    } else {
        base.join("profiles").join(id)
    }
}

/// The keychain service a profile's tokens are under. Not `<identifier>.<id>`:
/// a profile called "dev" would then share the dev build's item.
pub fn service_of(identifier: &str, id: &str) -> String {
    if id == DEFAULT {
        identifier.to_string()
    } else {
        format!("{identifier}.profile.{id}")
    }
}

/// Where a profile's task folders go unless the user chose (PROF-5).
pub fn root_of(id: &str) -> PathBuf {
    let shared = crate::config::default_worktree_root();
    if id == DEFAULT {
        return shared;
    }
    let mut name = shared.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!("-{id}"));
    shared.with_file_name(name)
}

fn registry_path(base: &Path) -> PathBuf {
    base.join("profiles.json")
}

/// The profiles, Default first. A file that cannot be read is kept beside
/// itself, since the next change writes over it, and the profiles' folders
/// are still there to be named again.
pub fn read(base: &Path) -> Registry {
    let path = registry_path(base);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Registry::default().with_default();
    };
    match serde_json::from_str::<Registry>(&raw) {
        Ok(r) => Registry { profiles: r.profiles.into_iter().filter(|p| safe_id(&p.id)).collect() }.with_default(),
        Err(e) => {
            let _ = std::fs::copy(&path, path.with_extension("json.unreadable"));
            eprintln!("profiles.json could not be read ({e}); a copy is beside it");
            Registry::default().with_default()
        }
    }
}

/// Change the registry, with every other copy of the app kept out until it
/// is written: each reads, edits and writes the whole file.
pub fn change<T>(base: &Path, f: impl FnOnce(&mut Registry) -> Result<T>) -> Result<T> {
    std::fs::create_dir_all(base)?;
    let lock = File::create(base.join("profiles.lock"))?;
    lock.lock()?;
    let mut registry = read(base);
    let out = f(&mut registry)?;
    crate::config::write_whole(&registry_path(base), &serde_json::to_vec_pretty(&registry)?)?;
    Ok(out)
}

/// The lock a running copy holds on its profile's folder (PROF-3). The
/// system lets go of it when the process ends, however it ends.
fn lock_path(folder: &Path) -> PathBuf {
    folder.join("running.lock")
}

/// Take `folder`'s lock, with this process's id in it for whoever finds it
/// taken. `None` when another copy holds it.
pub fn hold(folder: &Path) -> Result<Option<File>> {
    std::fs::create_dir_all(folder)?;
    let mut file = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(lock_path(folder))?;
    // Another copy looking to see whether this one is open holds it for a
    // moment too. A few tries, so that is not mistaken for a copy.
    for _ in 0..5 {
        match file.try_lock() {
            Ok(()) => {
                file.set_len(0)?;
                write!(file, "{}", std::process::id())?;
                return Ok(Some(file));
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(40)),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
    Ok(None)
}

/// The process holding `folder`'s lock, if one does: the copy that has
/// that profile open.
pub fn holder(folder: &Path) -> Option<i32> {
    let file = File::open(lock_path(folder)).ok()?;
    match file.try_lock() {
        Ok(()) => None,
        Err(_) => std::fs::read_to_string(lock_path(folder)).ok()?.trim().parse().ok(),
    }
}

/// What to open at a launch that names no profile (PROF-7), given which are
/// open already: the last profile to quit, those that quit together with
/// it, and those that never quit. Default when nothing has ever been open.
pub fn to_reopen(registry: &Registry, open: impl Fn(&str) -> bool) -> Vec<String> {
    let closed: Vec<&Entry> = registry.profiles.iter().filter(|p| !open(&p.id)).collect();
    let never_quit = |p: &Entry| matches!((p.opened_at, p.quit_at), (Some(o), q) if q.is_none_or(|q| o > q));
    let last = closed.iter().filter(|p| !never_quit(p)).filter_map(|p| p.quit_at).max();
    let chosen: Vec<String> = closed
        .iter()
        .filter(|p| never_quit(p) || matches!((p.quit_at, last), (Some(q), Some(l)) if q >= l - TOGETHER))
        .map(|p| p.id.clone())
        .collect();
    if chosen.is_empty() && !registry.profiles.iter().any(|p| open(&p.id)) {
        return vec![DEFAULT.to_string()];
    }
    chosen
}

/// The profile this process is.
pub struct Profile {
    pub id: String,
    /// The Default profile's folder (PROF-4).
    pub base: PathBuf,
    /// This profile's.
    pub folder: PathBuf,
    identifier: String,
    /// Held until the process exits (PROF-3).
    _lock: Option<File>,
}

impl Profile {
    pub fn service(&self) -> String {
        self.service_for(&self.id)
    }

    /// Profile `id`'s keychain service, under this build's identifier.
    pub fn service_for(&self, id: &str) -> String {
        service_of(&self.identifier, id)
    }

    pub fn default_root(&self) -> PathBuf {
        root_of(&self.id)
    }

    pub fn is_default(&self) -> bool {
        self.id == DEFAULT
    }
}

static CURRENT: OnceLock<Profile> = OnceLock::new();

/// The profile this process is. Set once, in `start`, before anything
/// reads it; a test that never started one is the Default profile, in a
/// folder of its own.
pub fn current() -> &'static Profile {
    CURRENT.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("vl-profile-{}", std::process::id()));
        Profile { id: DEFAULT.into(), folder: base.clone(), base, identifier: "test".into(), _lock: None }
    })
}

/// The profile named by `--profile <id>` or `--profile=<id>`, if any.
fn asked(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(i, a)| match a.strip_prefix("--profile") {
        Some("") => args.get(i + 1).cloned(),
        Some(rest) => rest.strip_prefix('=').map(String::from),
        None => None,
    })
}

/// Decide which profile this process is, before the window is made, and
/// take it. `None` when there is nothing for this copy to open: the
/// profile asked for is open in another copy, whose window is brought
/// forward instead (PROF-3). Otherwise the profiles to launch beside this
/// one (PROF-7).
pub fn start(identifier: &str, args: &[String]) -> Option<Vec<String>> {
    let base = base_dir(identifier);
    let registry = read(&base);
    let is_open = |id: &str| holder(&folder_of(&base, id)).is_some();

    let wanted: Vec<String> = match asked(args) {
        Some(id) if registry.get(&id).is_some() => vec![id],
        Some(id) => {
            eprintln!("there is no profile {id}; opening as if none was named");
            to_reopen(&registry, is_open)
        }
        None => to_reopen(&registry, is_open),
    };

    for (i, id) in wanted.iter().enumerate() {
        let folder = folder_of(&base, id);
        match hold(&folder) {
            Ok(Some(lock)) => {
                let _ = change(&base, |r| {
                    if let Some(p) = r.get_mut(id) {
                        p.opened_at = Some(Utc::now());
                    }
                    Ok(())
                });
                let profile = Profile { id: id.clone(), base, folder, identifier: identifier.into(), _lock: Some(lock) };
                let _ = CURRENT.set(profile);
                return Some(wanted.iter().skip(i + 1).cloned().collect());
            }
            Ok(None) => {}
            Err(e) => eprintln!("profile {id} could not be locked: {e}"),
        }
    }
    // Everything wanted is open already: bring the first of it forward, or,
    // when nothing was wanted, the first profile open.
    let open = wanted.first().cloned().or_else(|| registry.profiles.iter().map(|p| p.id.clone()).find(|id| is_open(id)));
    if let Some(pid) = open.and_then(|id| holder(&folder_of(&base, &id))) {
        bring_forward(pid);
    }
    None
}

/// Remember that this profile quit, and when (PROF-7).
pub fn quitting() {
    let profile = current();
    let _ = change(&profile.base, |r| {
        if let Some(p) = r.get_mut(&profile.id) {
            p.quit_at = Some(Utc::now());
        }
        Ok(())
    });
}

/// Open profile `id` in a copy of its own (PROF-2): through Launch
/// Services from the app bundle, so it is an app of its own with its own
/// Dock icon, or the binary itself for a dev build, which has no bundle.
/// Blocks for as long as `open` takes.
pub fn launch(id: &str) -> Result<()> {
    if !safe_id(id) {
        return Err(Error::Other(format!("not a profile: {id}")));
    }
    let exe = std::env::current_exe()?;
    let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf);
    match bundle {
        Some(app) => {
            let out = std::process::Command::new("/usr/bin/open")
                .arg("-n")
                .arg(&app)
                .args(["--args", "--profile", id])
                .output()?;
            if !out.status.success() {
                return Err(Error::Other(format!(
                    "the workspace could not be opened: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        }
        None => {
            use std::os::unix::process::CommandExt;
            let mut child = std::process::Command::new(exe)
                .args(["--profile", id])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                // Its own group, so stopping this copy's process group
                // never reaches it.
                .process_group(0)
                .spawn()?;
            std::thread::spawn(move || child.wait());
        }
    }
    Ok(())
}

/// What the window says about its profile (PROF-6), as last read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Identity {
    pub name: String,
    /// More than one profile exists, so the title, banners and Dock icon
    /// say which this is.
    pub many: bool,
    pub colour: usize,
}

static IDENTITY: Mutex<Option<Identity>> = Mutex::new(None);

/// What a banner's title starts with, if anything (PROF-6).
pub fn banner_title(title: &str) -> String {
    match IDENTITY.lock().as_ref() {
        Some(i) if i.many => format!("{} · {title}", i.name),
        _ => title.to_string(),
    }
}

/// Read the profiles again and show which this window is (PROF-6): its
/// title and its Dock icon. Off the main thread for the read; the window
/// and the Dock are changed on it.
pub fn show_identity(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let profile = current();
        let registry = read(&profile.base);
        let entry = registry.get(&profile.id).cloned().unwrap_or_else(|| Entry::new(profile.id.clone(), profile.id.clone()));
        let identity = Identity { name: entry.name.clone(), many: registry.profiles.len() > 1, colour: entry.colour() };
        if IDENTITY.lock().replace(identity.clone()).as_ref() == Some(&identity) {
            return;
        }
        // The profile button, when the change was made in another window.
        let _ = tauri::Emitter::emit(&app, "profiles:changed", ());
        let product = app.config().product_name.clone().unwrap_or_else(|| "Layer".into());
        let title = if identity.many { format!("{product} — {}", identity.name) } else { product };
        let mark = identity.many.then(|| (identity.name.clone(), identity.colour));
        let on_main = app.clone();
        let _ = app.run_on_main_thread(move || {
            use tauri::Manager;
            if let Some(window) = on_main.get_webview_window("main") {
                let _ = window.set_title(&title);
            }
            mark_dock(mark.as_ref().map(|(name, colour)| (name.as_str(), *colour)));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vl-profiles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(id: &str, opened: Option<i64>, quit: Option<i64>) -> Entry {
        let at = |s: i64| DateTime::<Utc>::from_timestamp(1_800_000_000 + s, 0).unwrap();
        Entry { opened_at: opened.map(at), quit_at: quit.map(at), ..Entry::new(id.into(), id.into()) }
    }

    #[test]
    fn the_profile_there_before_profiles_keeps_its_folder_tokens_and_task_folders() {
        let base = PathBuf::from("/support/app.test");
        assert_eq!(folder_of(&base, DEFAULT), base);
        assert_eq!(service_of("app.test", DEFAULT), "app.test");
        assert_eq!(root_of(DEFAULT), crate::config::default_worktree_root());

        assert_eq!(folder_of(&base, "personal"), base.join("profiles/personal"));
        assert_eq!(service_of("app.test", "dev"), "app.test.profile.dev", "never the dev build's item");
        assert_eq!(
            root_of("personal").file_name().unwrap().to_string_lossy(),
            ".villain-worktrees-personal"
        );
    }

    #[test]
    fn default_is_listed_before_profiles_json_exists() {
        let base = temp();
        let registry = read(&base);
        assert_eq!(registry.profiles.len(), 1);
        assert_eq!((registry.profiles[0].id.as_str(), registry.profiles[0].name.as_str()), (DEFAULT, "Default"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_new_profiles_id_is_made_from_its_name_and_never_taken() {
        let mut registry = Registry::default().with_default();
        assert_eq!(registry.new_id("My Side Project!"), "my-side-project");
        assert_eq!(registry.new_id("Default"), "default-2", "never the Default profile's");
        assert_eq!(registry.new_id("../../etc"), "etc");
        assert_eq!(registry.new_id("日本"), "profile");
        registry.profiles.push(entry("work", None, None));
        assert_eq!(registry.new_id("Work"), "work-2");
        assert!(safe_id(&registry.new_id("a/b\\c d")));
    }

    #[test]
    fn names_are_one_line_unique_ignoring_case_and_at_most_forty_characters() {
        let mut registry = Registry::default().with_default();
        registry.profiles.push(Entry { name: "ACME".into(), ..entry("acme", None, None) });
        assert_eq!(registry.check_name("  Personal ", None).unwrap(), "Personal");
        assert!(registry.check_name("acme", None).is_err());
        assert!(registry.check_name("acme", Some("acme")).is_ok(), "renaming to its own name in other letters");
        assert!(registry.check_name("   ", None).is_err());
        assert!(registry.check_name(&"x".repeat(41), None).is_err());
        assert!(registry.check_name("a\nb", None).is_err());
    }

    #[test]
    fn an_id_from_a_hand_edited_registry_is_never_a_folder() {
        let base = temp();
        std::fs::write(
            base.join("profiles.json"),
            r#"{"profiles":[{"id":"../../escape","name":"x"},{"id":"ok","name":"ok"}]}"#,
        )
        .unwrap();
        let registry = read(&base);
        assert!(registry.get("../../escape").is_none());
        assert!(registry.get("ok").is_some());
        assert!(launch("../x").is_err());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_registry_that_cannot_be_read_is_kept_beside_itself() {
        let base = temp();
        std::fs::write(base.join("profiles.json"), "{ not json").unwrap();
        assert_eq!(read(&base).profiles.len(), 1);
        assert!(base.join("profiles.json.unreadable").is_file());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_change_is_written_whole_for_the_next_copy_to_read() {
        let base = temp();
        change(&base, |r| {
            r.profiles.push(entry("personal", None, None));
            Ok(())
        })
        .unwrap();
        let registry = read(&base);
        assert_eq!(registry.profiles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [DEFAULT, "personal"]);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_profile_open_in_one_copy_cannot_be_taken_by_another() {
        let base = temp();
        let folder = folder_of(&base, "personal");
        let held = hold(&folder).unwrap().expect("free");
        assert_eq!(holder(&folder), Some(std::process::id() as i32));
        // flock is per open file, so a second open in this process stands in
        // for another copy.
        assert!(hold(&folder).unwrap().is_none());
        drop(held);
        // A process another test forks in that moment holds the file until
        // it execs, where it is closed, so the lock goes a moment later.
        let gone = (0..100).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            holder(&folder).is_none()
        });
        assert!(gone, "let go when the copy goes");
        assert!(hold(&folder).unwrap().is_some());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_launch_reopens_the_profiles_that_quit_together_with_the_last() {
        let registry = Registry {
            profiles: vec![
                entry(DEFAULT, Some(0), Some(10_000)),
                entry("personal", Some(0), Some(10_020)),
                // Quit hours before the others: closed on purpose.
                entry("spare", Some(0), Some(100)),
            ],
        };
        assert_eq!(to_reopen(&registry, |_| false), [DEFAULT, "personal"]);
    }

    #[test]
    fn a_profile_whose_copy_never_quit_is_reopened() {
        let registry = Registry {
            profiles: vec![
                entry(DEFAULT, Some(0), Some(10_000)),
                // Opened after it last quit, and not open now: it crashed.
                entry("crashed", Some(5_000), Some(100)),
                entry("never", Some(5_000), None),
            ],
        };
        assert_eq!(to_reopen(&registry, |_| false), [DEFAULT, "crashed", "never"]);
    }

    #[test]
    fn the_first_launch_opens_default_and_one_while_others_are_open_opens_nothing_new() {
        let fresh = Registry::default().with_default();
        assert_eq!(to_reopen(&fresh, |_| false), [DEFAULT]);

        let registry = Registry { profiles: vec![entry(DEFAULT, Some(0), None), entry("personal", Some(0), Some(50))] };
        assert!(to_reopen(&registry, |id| id == DEFAULT).contains(&"personal".to_string()));
        let both_open = to_reopen(&registry, |_| true);
        assert!(both_open.is_empty(), "nothing to open: the open one is brought forward");
    }

    #[test]
    fn a_profile_is_named_on_the_command_line_either_way() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(asked(&args(&["app", "--profile", "acme"])).as_deref(), Some("acme"));
        assert_eq!(asked(&args(&["app", "--profile=acme"])).as_deref(), Some("acme"));
        assert_eq!(asked(&args(&["app", "-psn_0_123"])), None);
        assert_eq!(asked(&args(&["app", "--profiles"])), None);
    }

    #[test]
    fn a_banner_names_its_profile_only_when_there_are_several() {
        *IDENTITY.lock() = Some(Identity { name: "ACME".into(), many: false, colour: 0 });
        assert_eq!(banner_title("Agent finished"), "Agent finished");
        *IDENTITY.lock() = Some(Identity { name: "ACME".into(), many: true, colour: 0 });
        assert_eq!(banner_title("Agent finished"), "ACME · Agent finished");
        *IDENTITY.lock() = None;
    }
}
