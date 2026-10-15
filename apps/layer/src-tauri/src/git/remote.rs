//! Where a repository's origin is, read from its remote URL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::run;
use crate::error::{Error, Result};

/// `owner/repo` parsed from the origin remote.
///
/// Handles the three shapes that turn up in practice, including the SSH form
/// with an explicit port that GitHub Enterprise installs often use:
///   git@host:acme/web.git
///   https://host/acme/web.git
///   ssh://git@host:2222/acme/web.git
pub fn origin_slug(dir: &Path) -> Result<(String, String)> {
    let url = run(dir, &["remote", "get-url", "origin"])?.trim().to_string();

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

    let segments: Vec<&str> = normalised
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

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
}
