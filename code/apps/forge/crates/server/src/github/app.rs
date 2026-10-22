//! Talking to GitHub as Forge's GitHub App: signing the app's JWT,
//! exchanging it for installation tokens, keeping a clone of each
//! connected repository on the server, and asking whether a pull request
//! can be merged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_domain::PullRequestBlocker;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tokio_util::task::TaskTracker;

#[derive(Clone)]
pub struct GitHubApp {
    pub app_id: String,
    /// For the install link: github.com/apps/<slug>.
    pub slug: String,
    key: EncodingKey,
    pub webhook_secret: String,
    /// Clones live in `<data_dir>/repos/<owner>/<name>`.
    pub data_dir: PathBuf,
    client: reqwest::Client,
    api: String,
    /// Where repositories are cloned from.
    web: String,
    algorithm: Algorithm,
    /// How long to wait before asking again while GitHub works out whether
    /// a pull request can be merged.
    retry_after: Duration,
    /// How long to wait between asks while a pull request's checks haven't
    /// started yet.
    checks_after: Duration,
    /// How many repositories to ask for at a time when listing an
    /// installation's.
    page_size: usize,
    /// One clone or update at a time per repository: a webhook and the
    /// setup callback can both start one for the same repository.
    cloning: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// What it's doing in the background: updating clones, asking about
    /// pull requests.
    background: TaskTracker,
}

impl std::fmt::Debug for GitHubApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubApp")
            .field("app_id", &self.app_id)
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct Claims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Deserialize)]
struct InstallationToken {
    token: String,
}

impl GitHubApp {
    /// From `github.app_id`, `github.app_slug`, `github.private_key` (PEM)
    /// or `github.private_key_path`, `github.webhook_secret` and `data_dir`.
    /// `None` if any is missing, which turns the connection off.
    pub fn from_settings(settings: &crate::config::Settings) -> Option<GitHubApp> {
        let var = |name: &str| settings.var(name);
        let pem = var("FORGE_GITHUB_PRIVATE_KEY").or_else(|| {
            var("FORGE_GITHUB_PRIVATE_KEY_PATH").and_then(|p| std::fs::read_to_string(p).ok())
        })?;
        let key = match EncodingKey::from_rsa_pem(pem.as_bytes()) {
            Ok(key) => key,
            Err(e) => {
                tracing::error!(error = %e, "FORGE_GITHUB_PRIVATE_KEY isn't a valid RSA key");
                return None;
            }
        };
        Some(GitHubApp::new(
            var("FORGE_GITHUB_APP_ID")?,
            var("FORGE_GITHUB_APP_SLUG")?,
            key,
            var("FORGE_GITHUB_WEBHOOK_SECRET")?,
            PathBuf::from(var("FORGE_DATA_DIR").unwrap_or_else(|| "data".into())),
        ))
    }

    pub fn new(
        app_id: String,
        slug: String,
        key: EncodingKey,
        webhook_secret: String,
        data_dir: PathBuf,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("forge")
            .build()
            .expect("an HTTP client with default settings builds");
        GitHubApp {
            app_id,
            slug,
            key,
            webhook_secret,
            data_dir,
            client,
            api: "https://api.github.com".into(),
            web: "https://github.com".into(),
            algorithm: Algorithm::RS256,
            retry_after: Duration::from_secs(3),
            checks_after: Duration::from_secs(10),
            page_size: 100,
            cloning: Arc::default(),
            background: TaskTracker::new(),
        }
    }

    /// For tests: talks to a stand-in for GitHub at `api` (its API, and
    /// where repositories are cloned from), which takes an app token signed
    /// with the shared key the app was made with, and lists repositories
    /// two to a page.
    pub fn with_test_api(mut self, api: String) -> Self {
        self.web = api.clone();
        self.api = api;
        self.algorithm = Algorithm::HS256;
        self.retry_after = Duration::from_millis(10);
        self.checks_after = Duration::from_millis(10);
        self.page_size = 2;
        self
    }

    /// The app's own short-lived JWT, for app-level API calls.
    fn jwt(&self) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp();
        // Backdated a minute for clock drift; GitHub allows at most ten minutes.
        let claims = Claims {
            iat: now - 60,
            exp: now + 540,
            iss: self.app_id.clone(),
        };
        jsonwebtoken::encode(&Header::new(self.algorithm), &claims, &self.key)
            .map_err(|e| e.to_string())
    }

    async fn get(&self, path: &str, token: &str) -> Result<Value, String> {
        let response = self
            .client
            .get(format!("{}{path}", self.api))
            .bearer_auth(token)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let body: Value = response.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "GitHub {status}: {}",
                body["message"].as_str().unwrap_or("")
            ));
        }
        Ok(body)
    }

    /// A token that can read the installation's repositories, valid an hour.
    pub async fn installation_token(&self, installation_id: i64) -> Result<String, String> {
        let response = self
            .client
            .post(format!(
                "{}/app/installations/{installation_id}/access_tokens",
                self.api
            ))
            .bearer_auth(self.jwt()?)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub refused an installation token: {}",
                response.status()
            ));
        }
        let token: InstallationToken = response.json().await.map_err(|e| e.to_string())?;
        Ok(token.token)
    }

    /// The account an installation belongs to, which proves it's one of ours.
    pub async fn installation_account(&self, installation_id: i64) -> Result<String, String> {
        let body = self
            .get(
                &format!("/app/installations/{installation_id}"),
                &self.jwt()?,
            )
            .await?;
        Ok(body["account"]["login"]
            .as_str()
            .unwrap_or("unknown")
            .to_string())
    }

    /// The repositories an installation can read: (owner/name, default
    /// branch), every page of them.
    pub async fn installation_repositories(
        &self,
        installation_id: i64,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        let token = self.installation_token(installation_id).await?;
        let mut repositories = Vec::new();
        for page in 1.. {
            let body = self
                .get(
                    &format!(
                        "/installation/repositories?per_page={}&page={page}",
                        self.page_size
                    ),
                    &token,
                )
                .await?;
            let listed = body["repositories"].as_array().cloned().unwrap_or_default();
            let count = listed.len();
            repositories.extend(listed.iter().filter_map(|r| {
                Some((
                    r["full_name"].as_str()?.to_string(),
                    r["default_branch"].as_str().map(str::to_string),
                ))
            }));
            let total = body["total_count"].as_u64().unwrap_or(0) as usize;
            if count < self.page_size || page * self.page_size >= total {
                break;
            }
        }
        Ok(repositories)
    }

    /// A repository's default branch, as GitHub has it now.
    pub async fn default_branch(
        &self,
        installation_id: i64,
        full_name: &str,
    ) -> Result<String, String> {
        if !valid_full_name(full_name) {
            return Err(format!("unexpected repository {full_name}"));
        }
        let token = self.installation_token(installation_id).await?;
        let body = self.get(&format!("/repos/{full_name}"), &token).await?;
        body["default_branch"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("GitHub didn't say which branch is {full_name}'s default"))
    }

    /// Does `task` in the background, after the request that started it.
    pub fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.background.spawn(task);
    }

    /// Waits until everything it was doing in the background is done,
    /// including what that started in turn. For tests: what a webhook
    /// changes is only all there once this returns.
    pub async fn idle(&self) {
        self.background.close();
        self.background.wait().await;
        self.background.reopen();
    }

    /// How long GitHub takes to start working out whether a pull request
    /// can be merged, e.g. after the branch it goes into moved.
    pub fn retry_after(&self) -> Duration {
        self.retry_after
    }

    /// How long to wait before asking again while a pull request's checks
    /// haven't started yet.
    pub fn checks_after(&self) -> Duration {
        self.checks_after
    }

    /// An open pull request as GitHub sees it now: whether it's a draft, and
    /// what keeps it from being merged. GitHub works out conflicts in the
    /// background once asked, so this asks again a few times while it
    /// doesn't know yet; still unknown, it counts as no conflicts.
    ///
    /// Right after a push, GitHub may not have started its checks yet. With
    /// `expect_checks`, a latest commit without any checks counts as checks
    /// running when the commit it goes into has some (the repository runs
    /// checks), and says so (`awaiting_checks`); otherwise it passes.
    pub async fn pull_request(
        &self,
        installation_id: i64,
        full_name: &str,
        number: i64,
        expect_checks: bool,
    ) -> Result<PullRequestNow, String> {
        if !valid_full_name(full_name) {
            return Err(format!("unexpected repository {full_name}"));
        }
        let token = self.installation_token(installation_id).await?;
        let path = format!("/repos/{full_name}/pulls/{number}");
        let mut pr = self.get(&path, &token).await?;
        for _ in 0..5 {
            if pr["state"] != "open" || !pr["mergeable"].is_null() {
                break;
            }
            tokio::time::sleep(self.retry_after).await;
            pr = self.get(&path, &token).await?;
        }
        let open = pr["state"] == "open" && pr["merged"] != true;
        let conflicts = pr["mergeable"] == false;
        let mut awaiting_checks = false;
        let checks = match pr["head"]["sha"].as_str() {
            Some(sha) if open && !conflicts => match self.checks(&token, full_name, sha).await {
                Some(checks) => checks,
                None => {
                    awaiting_checks = match pr["base"]["sha"].as_str() {
                        Some(base) if expect_checks => {
                            self.checks(&token, full_name, base).await.is_some()
                        }
                        _ => false,
                    };
                    if awaiting_checks {
                        Checks::Running
                    } else {
                        Checks::Passed
                    }
                }
            },
            _ => Checks::Passed,
        };
        Ok(PullRequestNow {
            open,
            draft: pr["draft"] == true,
            awaiting_checks,
            blocker: if conflicts {
                Some(PullRequestBlocker::Conflicts)
            } else {
                match checks {
                    Checks::Failed => Some(PullRequestBlocker::ChecksFailed),
                    Checks::Running => Some(PullRequestBlocker::ChecksRunning),
                    Checks::Passed => None,
                }
            },
        })
    }

    /// How a commit's checks went: check runs (GitHub Actions and other
    /// apps) and commit statuses. `None` for a commit without any, and for
    /// checks Forge may not read (the app lacks the permission).
    async fn checks(&self, token: &str, full_name: &str, sha: &str) -> Option<Checks> {
        let mut checks = None::<Checks>;
        let mut add = |c: Checks| checks = Some(checks.map_or(c, |checks| checks.and(c)));
        let runs = self
            .get(
                &format!("/repos/{full_name}/commits/{sha}/check-runs?per_page=100"),
                token,
            )
            .await;
        for run in runs
            .iter()
            .flat_map(|r| r["check_runs"].as_array())
            .flatten()
        {
            add(match (run["status"].as_str(), run["conclusion"].as_str()) {
                (
                    Some("completed"),
                    Some(
                        "failure" | "timed_out" | "cancelled" | "action_required"
                        | "startup_failure",
                    ),
                ) => Checks::Failed,
                (Some("completed"), _) => Checks::Passed,
                _ => Checks::Running,
            });
        }
        // Without any statuses, GitHub calls the combined state "pending".
        if let Ok(status) = self
            .get(&format!("/repos/{full_name}/commits/{sha}/status"), token)
            .await
            && status["total_count"].as_i64().unwrap_or(0) > 0
        {
            add(match status["state"].as_str() {
                Some("failure" | "error") => Checks::Failed,
                Some("pending") => Checks::Running,
                _ => Checks::Passed,
            });
        }
        checks
    }

    /// Where a repository's clone lives.
    pub fn clone_path(&self, full_name: &str) -> PathBuf {
        self.data_dir.join("repos").join(full_name)
    }

    /// Clones the repository, or brings the clone up to date with its default branch.
    pub async fn sync_clone(
        &self,
        installation_id: i64,
        full_name: &str,
        branch: &str,
    ) -> Result<(), String> {
        if !valid_full_name(full_name) || !valid_ref(branch) {
            return Err(format!(
                "unexpected repository {full_name} or branch {branch}"
            ));
        }
        let lock = self
            .cloning
            .lock()
            .expect("the lock is never held across a panic")
            .entry(full_name.to_string())
            .or_default()
            .clone();
        let _one_at_a_time = lock.lock().await;
        let token = self.installation_token(installation_id).await?;
        let path = self.clone_path(full_name);
        let url = format!("{}/{full_name}.git", self.web);
        // The token goes in a header for this command only, never into the clone's config.
        let auth = format!("http.extraHeader=Authorization: Basic {}", basic(&token));
        if path.join(".git").exists() {
            git(
                &path,
                &["-c", &auth, "fetch", "--depth", "200", "origin", branch],
            )
            .await?;
            // FETCH_HEAD, not origin/<branch>: a clone made with --branch tracks
            // only its first branch, so a renamed default branch has no
            // origin/ ref.
            git(&path, &["reset", "--hard", "FETCH_HEAD"]).await?;
        } else {
            tokio::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
                .await
                .map_err(|e| e.to_string())?;
            let target = path.to_string_lossy().to_string();
            git(
                Path::new("."),
                &[
                    "-c", &auth, "clone", "--depth", "200", "--branch", branch, &url, &target,
                ],
            )
            .await?;
        }
        Ok(())
    }
}

/// An open pull request as GitHub sees it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullRequestNow {
    /// Neither closed nor merged.
    pub open: bool,
    pub draft: bool,
    pub blocker: Option<PullRequestBlocker>,
    /// Its latest commit has no checks yet, though the repository runs
    /// them: they likely haven't started. Counts as checks running.
    pub awaiting_checks: bool,
}

/// How a commit's checks went, worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Checks {
    Failed,
    Running,
    Passed,
}

impl Checks {
    /// Both together: the worse of the two.
    fn and(self, other: Checks) -> Checks {
        match (self, other) {
            (Checks::Failed, _) | (_, Checks::Failed) => Checks::Failed,
            (Checks::Running, _) | (_, Checks::Running) => Checks::Running,
            _ => Checks::Passed,
        }
    }
}

fn basic(token: &str) -> String {
    use std::fmt::Write;
    // base64 of "x-access-token:<token>", without pulling in a base64 crate.
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = format!("x-access-token:{token}");
    let bytes = input.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                let _ = out.write_char(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn valid_full_name(name: &str) -> bool {
    let mut parts = name.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

fn valid_ref(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.contains("..")
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
}

async fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await
        .map_err(|e| format!("couldn't run git: {e}"))?;
    if !output.status.success() {
        // Never echo the command: it carries the token.
        return Err(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_is_standard_base64() {
        assert_eq!(basic("abc"), "eC1hY2Nlc3MtdG9rZW46YWJj");
        assert_eq!(basic("ab"), "eC1hY2Nlc3MtdG9rZW46YWI=");
    }

    #[test]
    fn repository_names_and_branches_are_checked() {
        assert!(valid_full_name("villain/forge"));
        assert!(valid_full_name("a-b/c.d_e"));
        for bad in ["forge", "a/b/c", "../x", "a/..", "a/b c", "a/-rf;"] {
            assert!(!valid_full_name(bad), "{bad}");
        }
        assert!(valid_ref("main") && valid_ref("release/1.2"));
        assert!(!valid_ref("--upload-pack=x") && !valid_ref("a..b"));
    }
}
