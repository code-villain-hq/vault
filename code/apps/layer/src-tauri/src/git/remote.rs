//! Where a repository's origin is, read from its remote URL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::run;
use crate::error::{Error, Result};

/// Unlike `origin_url`, accounts for git's `url.*.insteadOf` rewrites.
pub(crate) fn effective_origin_url(dir: &Path) -> Result<String> {
    Ok(run(dir, &["remote", "get-url", "origin"])?.trim().to_string())
}

/// `owner/repo` parsed from the origin remote.
///
/// Handles the three shapes that turn up in practice, including the SSH form
/// with an explicit port that GitHub Enterprise installs often use:
///   git@host:acme/web.git
///   https://host/acme/web.git
///   ssh://git@host:2222/acme/web.git
pub fn origin_slug(dir: &Path) -> Result<(String, String)> {
    let url = effective_origin_url(dir)?;

    // A copy of a clone that had no origin yet fetches from the clone's
    // folder, and `/Users/me/work/platform/tool` read as `platform/tool`
    // sent GitHub a repository that does not exist: a 404 under the task's
    // pull requests, for a repo that was never on GitHub at all.
    let scp = !url.contains("://") && url.split_once(':').is_some_and(|(host, _)| !host.contains('/'));
    if !(url.contains("://") || scp) {
        return Err(Error::Git(format!("origin is a folder on this Mac ({url}), not GitHub")));
    }

    // Normalise scp-style `host:path` into `host/path` so one split works for
    // everything. A `//` right after the colon means it was a real scheme.
    let normalised = match url.split_once("://") {
        Some((_, rest)) => rest.to_string(),
        None => match url.split_once(':') {
            Some((host, path)) => format!("{host}/{path}"),
            None => url.clone(),
        },
    };

    let segments: Vec<&str> =
        normalised.trim_end_matches('/').trim_end_matches(".git").split('/').filter(|s| !s.is_empty()).collect();

    // The first segment is the host (possibly with userinfo and a port); the
    // last two are always owner and repo.
    if segments.len() >= 3 {
        let repo = segments[segments.len() - 1];
        let owner = segments[segments.len() - 2];
        return Ok((owner.to_string(), repo.to_string()));
    }

    Err(Error::Git(format!("cannot parse owner/repo from {url}")))
}

/// How long a slug read by `origin_slug_cached` is trusted. A remote changed
/// by hand outside the app is seen within this; one the app moves is seen
/// at once (`forget_origins`).
const ORIGIN_KEPT: Duration = Duration::from_secs(10 * 60);

/// `owner/repo`, and when it was read.
type Read = (Instant, (String, String));

/// Slugs read, by folder.
static ORIGINS: LazyLock<Mutex<HashMap<PathBuf, Read>>> = LazyLock::new(Default::default);

/// `origin_slug`, remembered for a while, for the PR sweep. It asked git for
/// every repository of every task every 90 seconds, for a URL that changes
/// about never. Only a slug read is remembered: a failure is asked again.
pub fn origin_slug_cached(dir: &Path) -> Result<(String, String)> {
    if let Some((at, slug)) = ORIGINS.lock().get(dir) {
        if at.elapsed() < ORIGIN_KEPT {
            return Ok(slug.clone());
        }
    }
    let slug = origin_slug(dir)?;
    let mut origins = ORIGINS.lock();
    // Folders of checkouts since removed go with the rest of the stale.
    origins.retain(|_, (at, _)| at.elapsed() < ORIGIN_KEPT);
    origins.insert(dir.to_path_buf(), (Instant::now(), slug.clone()));
    Ok(slug)
}

/// Forget every slug read: the app has pointed a copy at another origin, and
/// its worktrees with it.
pub fn forget_origins() {
    ORIGINS.lock().clear();
}

/// Clone the repository a workspace knows by its remote key
/// (`github.com/acme/billing`) into `into`, which must not exist yet
/// (REPO-9). Over SSH first, as most clones on a developer's Mac are, then
/// over HTTPS, for a public repository or one a credential helper knows.
pub fn clone_by_key(key: &str, into: &Path) -> Result<()> {
    let Some((host, path)) = key.split_once('/').filter(|(host, _)| !host.is_empty()) else {
        return Err(Error::Git(format!("{key} is a folder on another Mac, so there is nothing to clone it from")));
    };
    if into.exists() {
        return Err(Error::Other(format!("{} is there already", into.display())));
    }
    let parent = into.parent().ok_or_else(|| Error::Other(format!("{} has no folder around it", into.display())))?;
    let target = into.to_string_lossy();
    let said = |e: Error| match e {
        Error::Git(m) => m,
        e => e.to_string(),
    };
    let ssh = format!("git@{host}:{path}.git");
    let Err(over_ssh) = run(parent, &["clone", "--quiet", "--", &ssh, &target]) else { return Ok(()) };
    let https = format!("https://{key}.git");
    run(parent, &["clone", "--quiet", "--", &https, &target]).map(|_| ()).map_err(|over_https| {
        Error::Other(format!("Could not clone {key}.\nOver SSH: {}\nOver HTTPS: {}", said(over_ssh), said(over_https)))
    })
}

/// Clone `url` at `ref` (a tag or a branch) into `into`, which does not exist
/// yet, with no history: a toolset's pinned code, fetched for its files.
///
/// Both arguments come from a manifest, which a team can write, so neither is
/// trusted: only `https://`, `ssh://` and `git@host:` URLs are taken (not
/// `file://`, not git's `ext::`, which runs a command), a ref cannot read as
/// an option, and `--` ends the options before the positionals.
pub fn clone_at(url: &str, r#ref: &str, into: &Path) -> Result<()> {
    if !fetchable(url) {
        return Err(Error::Git(format!("{url} is not an https://, ssh:// or git@host: address, so it is not fetched")));
    }
    if !plain_ref(r#ref) {
        return Err(Error::Git(format!("{:?} is not a tag or branch name a fetch can use", r#ref)));
    }
    clone_unchecked(url, r#ref, into)
}

/// An address to clone from: a scheme git reaches over the network, no
/// spaces or control characters.
fn fetchable(url: &str) -> bool {
    let scp_host = url.strip_prefix("git@").and_then(|rest| rest.split_once(':'));
    let shaped = url.starts_with("https://") && url.len() > "https://".len()
        || url.starts_with("ssh://") && url.len() > "ssh://".len()
        || scp_host.is_some_and(|(host, path)| !host.is_empty() && !path.is_empty());
    shaped && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// A ref that is a name and not an option: it starts with a letter or digit
/// and has only what refs ordinarily have.
fn plain_ref(r: &str) -> bool {
    r.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && !r.contains("..")
        && !r.ends_with('/')
        && !r.ends_with(".lock")
        && r.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '+'))
}

/// The clone itself, for a URL already checked. A stalled transfer gives up
/// after half a minute instead of holding the install for ever.
fn clone_unchecked(url: &str, r#ref: &str, into: &Path) -> Result<()> {
    if !into.is_absolute() {
        return Err(Error::Other(format!("{} is not an absolute path", into.display())));
    }
    if into.exists() {
        return Err(Error::Other(format!("{} is there already", into.display())));
    }
    let parent = into.parent().ok_or_else(|| Error::Other(format!("{} has no folder around it", into.display())))?;
    let target = into.to_string_lossy();
    run(
        parent,
        &[
            "-c",
            "http.lowSpeedLimit=1000",
            "-c",
            "http.lowSpeedTime=30",
            "-c",
            "advice.detachedHead=false",
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--single-branch",
            "--branch",
            r#ref,
            "--",
            url,
            &target,
        ],
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_is_read_once_and_again_after_the_app_moves_it() {
        let dir = std::env::temp_dir().join(format!("vl-origin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q"]).unwrap();
        run(&dir, &["remote", "add", "origin", "git@github.com:acme/web.git"]).unwrap();
        assert_eq!(origin_slug_cached(&dir).unwrap(), ("acme".to_string(), "web".to_string()));

        run(&dir, &["remote", "set-url", "origin", "git@github.com:acme/api.git"]).unwrap();
        assert_eq!(origin_slug_cached(&dir).unwrap().1, "web", "remembered, not asked again");
        forget_origins();
        assert_eq!(origin_slug_cached(&dir).unwrap().1, "api");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repository_kept_by_a_folder_elsewhere_is_not_cloned_and_a_folder_there_is_kept() {
        let err = clone_by_key("/Users/ada/code/billing", Path::new("/tmp/billing")).unwrap_err();
        assert!(err.to_string().contains("a folder on another Mac"), "{err}");
        let there = std::env::temp_dir();
        let err = clone_by_key("github.com/acme/billing", &there).unwrap_err();
        assert!(err.to_string().contains("is there already"), "{err}");
    }

    #[test]
    fn only_addresses_over_the_network_are_fetched_and_a_ref_is_never_an_option() {
        let into = std::env::temp_dir().join(format!("vl-clone-{}", uuid::Uuid::new_v4()));
        for url in [
            "file:///etc",
            "/Users/me/repo",
            "ext::sh -c touch% /tmp/pwned",
            "--upload-pack=touch /tmp/pwned",
            "-oProxyCommand=x",
            "http://example.com/x",
            "git@",
            "git@host:",
            "https://",
            "https://example.com/a b",
            "",
        ] {
            let err = clone_at(url, "v1.0.0", &into).unwrap_err().to_string();
            assert!(err.contains("is not an https"), "{url}: {err}");
        }
        for r in ["", "-v", "--upload-pack=x", "a..b", ".hidden", "a b", "a;b", "a/", "x.lock", "a\nb"] {
            let err = clone_at("https://example.com/x.git", r, &into).unwrap_err().to_string();
            assert!(err.contains("not a tag or branch"), "{r:?}: {err}");
        }
        assert!(!into.exists());
        for url in [
            "https://github.com/obra/superpowers",
            "ssh://git@host:2222/a/b.git",
            "git@github.com:obra/superpowers.git",
        ] {
            assert!(fetchable(url), "{url}");
        }
        for r in ["v7.0.2", "main", "release/1.2", "v1.0.0+build"] {
            assert!(plain_ref(r), "{r}");
        }
    }

    #[test]
    fn a_tag_is_cloned_without_history_and_a_folder_in_the_way_is_not_cloned_over() {
        let root = std::env::temp_dir().join(format!("vl-clone-{}", uuid::Uuid::new_v4()));
        let origin = root.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        run(&origin, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(origin.join("a.txt"), "one").unwrap();
        run(&origin, &["add", "-A"]).unwrap();
        let commit = |msg: &str| {
            run(
                &origin,
                &["-c", "user.email=a@b.c", "-c", "user.name=T", "-c", "commit.gpgsign=false", "commit", "-qm", msg],
            )
            .unwrap()
        };
        commit("one");
        run(&origin, &["-c", "tag.gpgsign=false", "tag", "v1.0.0"]).unwrap();
        std::fs::write(origin.join("a.txt"), "two").unwrap();
        run(&origin, &["add", "-A"]).unwrap();
        commit("two");

        let into = root.join("clone");
        clone_unchecked(origin.to_str().unwrap(), "v1.0.0", &into).unwrap();

        assert_eq!(std::fs::read_to_string(into.join("a.txt")).unwrap(), "one", "the tag, not the branch");
        assert_eq!(run(&into, &["rev-list", "--count", "HEAD"]).unwrap().trim(), "1");
        let err = clone_unchecked(origin.to_str().unwrap(), "v1.0.0", &into).unwrap_err().to_string();
        assert!(err.contains("is there already"), "{err}");
        let err = clone_unchecked(origin.to_str().unwrap(), "v9", &root.join("other")).unwrap_err().to_string();
        assert!(err.contains("v9"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
