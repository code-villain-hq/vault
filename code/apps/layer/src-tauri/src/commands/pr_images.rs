//! `pr_images` (PR-13): an agent shows screenshots in a task's open pull
//! request. The storing and the description are `integrations/github/images.rs`;
//! this finds the repository and its PR, and reads the files.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config::Checkout;
use crate::error::{Error, Result};
use crate::git;
use crate::integrations::github::{self, Image, MAX_BYTES, MAX_IMAGES};
use crate::mcp::{bool_prop, required, str_prop, tool};

use super::github::github_client;
use super::{off_runtime, AppState};

pub fn pr_images_tools() -> Vec<Value> {
    vec![tool(
        "pr_images",
        "Show images in a repository's open pull request: screenshots of a visible change, \
         night and day. Give every image the PR should show in one call; a later call \
         replaces them. They go in a section of the description the app keeps, and the \
         rest of the description is left as written. Files must be PNG, JPEG, GIF or WebP \
         (browser_screenshot with save: true writes one). Open the PR first with open_prs. \
         Needs confirm: true after the user agrees: the images are stored in the repository, \
         for everyone who can read it.",
        json!({
            "task_id": str_prop("Task id from list_tasks"),
            "repo": str_prop("The repository's name as list_tasks shows it; may be left out when the task has one"),
            "images": {
                "type": "array",
                "description": "At most 10, in the order to show them",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": str_prop("Absolute path of the image file"),
                        "caption": str_prop("Shown above the image, e.g. \"Night\"; the file's name when left out"),
                    },
                    "required": ["path"],
                },
            },
            "confirm": bool_prop(
                "Set true only after the user agrees. Calls without it are refused — \
                 this tool changes shared state that is hard to undo.",
            ),
        }),
        vec!["task_id", "images"],
    )]
}

pub fn pr_images_owns(name: &str) -> bool {
    name == "pr_images"
}

/// The confirm gate is `mcp.rs`'s: `pr_images` is in its `DANGER` list.
pub async fn pr_images_call(state: &AppState, args: Value) -> Result<Value> {
    let task = state.config.task(required(&args, "task_id")?)?;
    super::pr_task::not_a_review(&task, "given images")?;
    let checkout = checkout_for(state, &task.id, args.get("repo").and_then(Value::as_str))?;
    let repo = state.config.project(&checkout.project_id)?.name;

    let wanted = wanted(&args)?;
    let images = off_runtime(move || read_images(&wanted)).await??;

    let dir = PathBuf::from(&checkout.path);
    let (owner, name) = off_runtime(move || git::origin_slug_cached(&dir)).await??;
    let (client, _) = github_client(state)?;
    let pr = client.pull_for_branch(&owner, &name, &task.branch).await?.ok_or_else(|| {
        Error::Other(format!(
            "{repo} has no open pull request for {}: open it with open_prs first",
            task.branch
        ))
    })?;
    let shown = client.show_images(&owner, &name, pr.number, &pr.url, &images).await?;
    Ok(json!({
        "repo": repo,
        "pull_request": pr.url,
        "images": shown.iter().map(|s| json!({ "caption": s.caption, "url": s.url })).collect::<Vec<_>>(),
    }))
}

/// The task's checkout of `repo`, or its only one when none is named.
fn checkout_for(state: &AppState, task_id: &str, repo: Option<&str>) -> Result<Checkout> {
    let named: Vec<(String, Checkout)> = state
        .config
        .checkouts_of(task_id)
        .into_iter()
        .map(|c| (state.config.project(&c.project_id).map(|p| p.name).unwrap_or_default(), c))
        .collect();
    let names = || named.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
    match repo.map(str::trim).filter(|r| !r.is_empty()) {
        Some(want) => named
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(want))
            .map(|(_, c)| c.clone())
            .ok_or_else(|| Error::Other(format!("the task has no repository {want}; it has {}", names()))),
        None if named.len() == 1 => Ok(named[0].1.clone()),
        None if named.is_empty() => Err(Error::Other("the task has no repositories".into())),
        None => Err(Error::Other(format!("say which repo: the task has {}", names()))),
    }
}

/// The `images` argument, as paths and captions.
fn wanted(args: &Value) -> Result<Vec<(PathBuf, Option<String>)>> {
    let list = args.get("images").and_then(Value::as_array).cloned().unwrap_or_default();
    if list.is_empty() {
        return Err(Error::Other("images is required: at least one".into()));
    }
    if list.len() > MAX_IMAGES {
        return Err(Error::Other(format!("at most {MAX_IMAGES} images at a time, not {}", list.len())));
    }
    list.iter()
        .map(|item| {
            let path = item.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty());
            let path = path.ok_or_else(|| Error::Other("every image needs a path".into()))?;
            let caption = item.get("caption").and_then(Value::as_str).map(str::to_string);
            Ok((PathBuf::from(path.trim()), caption))
        })
        .collect()
}

/// Every file read and checked before anything is sent: one that is not an
/// image fails the call, rather than a PR showing half of what was asked.
fn read_images(wanted: &[(PathBuf, Option<String>)]) -> Result<Vec<Image>> {
    wanted.iter().map(|(path, caption)| read_image(path, caption.as_deref())).collect()
}

fn read_image(path: &Path, caption: Option<&str>) -> Result<Image> {
    let shown = path.display();
    if !path.is_absolute() {
        return Err(Error::Other(format!("{shown}: give an absolute path")));
    }
    let meta = std::fs::metadata(path).map_err(|e| Error::Other(format!("{shown}: {e}")))?;
    if !meta.is_file() {
        return Err(Error::Other(format!("{shown} is not a file")));
    }
    if meta.len() > MAX_BYTES as u64 {
        return Err(Error::Other(format!("{shown} is {} MB; at most {} MB", meta.len() / 1_048_576, MAX_BYTES / 1_048_576)));
    }
    let bytes = std::fs::read(path).map_err(|e| Error::Other(format!("{shown}: {e}")))?;
    let ext = github::kind_of(&bytes)
        .ok_or_else(|| Error::Other(format!("{shown} is not a PNG, JPEG, GIF or WebP image; nothing was sent")))?;
    let caption = match caption.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
    };
    Ok(Image { caption, ext, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vl-pr-images-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn temp(name: &str, bytes: &[u8]) -> (PathBuf, PathBuf) {
        let dir = folder();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn a_file_named_like_an_image_that_is_not_one_is_never_sent() {
        let (_dir, key) = temp("night.png", b"-----BEGIN OPENSSH PRIVATE KEY-----\n");
        let (_dir2, shot) = temp("day.png", b"\x89PNG\r\n\x1a\nday");

        let err = read_images(&[(shot, None), (key.clone(), None)]).err().unwrap().to_string();

        assert!(err.contains("is not a PNG, JPEG, GIF or WebP image"), "{err}");
        assert!(err.contains(&key.display().to_string()), "{err}");
    }

    #[test]
    fn an_image_is_captioned_by_its_file_name_unless_given_one() {
        let (_dir, path) = temp("sidebar-night.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0]);

        let named = read_image(&path, None).unwrap();
        let given = read_image(&path, Some("  Night  ")).unwrap();

        assert_eq!((named.caption.as_str(), named.ext), ("sidebar-night", "jpg"));
        assert_eq!(given.caption, "Night");
    }

    #[test]
    fn a_relative_path_or_a_folder_is_refused() {
        assert!(read_image(Path::new("shot.png"), None).is_err());
        assert!(read_image(&folder(), None).err().unwrap().to_string().contains("is not a file"));
    }

    #[test]
    fn the_number_of_images_is_capped_and_none_is_not_a_call() {
        let many: Vec<Value> = (0..=MAX_IMAGES).map(|i| json!({ "path": format!("/tmp/{i}.png") })).collect();
        assert!(wanted(&json!({ "images": many })).err().unwrap().to_string().contains("at most 10"));
        assert!(wanted(&json!({ "images": [] })).is_err());
        assert!(wanted(&json!({ "images": [{ "caption": "Night" }] })).is_err());
        assert_eq!(wanted(&json!({ "images": [{ "path": "/a.png", "caption": "Night" }] })).unwrap().len(), 1);
    }
}
