//! Read-only access to a git repository's tracked files, for the AI.
//!
//! Only files git tracks can be listed or read: that keeps ignored files
//! such as `.env` out of reach. A service's path globs narrow it further.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::RegexBuilder;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RepoError(pub String);

pub struct Repo {
    root: PathBuf,
    /// Tracked files in scope, sorted.
    files: BTreeSet<String>,
    /// The scope's globs, for git commands.
    globs: Vec<String>,
}

const MAX_LIST: usize = 400;
const MAX_READ_LINES: usize = 400;
const MAX_READ_BYTES: usize = 60_000;
const MAX_MATCHES: usize = 80;
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_GIT_OUTPUT: usize = 30_000;

impl Repo {
    /// Opens the repository at `root`, limited to the files matching
    /// `globs` (all tracked files when there are none).
    pub async fn open(root: &Path, globs: &[String]) -> Result<Repo, RepoError> {
        let output = git(root, &["ls-files", "-z"]).await?;
        let scope = glob_set(globs)?;
        let files = output
            .split('\0')
            .filter(|f| !f.is_empty())
            .filter(|f| scope.as_ref().is_none_or(|s| s.is_match(f)))
            .map(str::to_string)
            .collect();
        Ok(Repo {
            root: root.to_path_buf(),
            files,
            globs: globs.to_vec(),
        })
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Files in scope, optionally narrowed by a glob.
    pub fn list(&self, glob: Option<&str>) -> Result<String, RepoError> {
        let filter = glob
            .map(|g| glob_set(&[g.to_string()]))
            .transpose()?
            .flatten();
        let matching: Vec<&String> = self
            .files
            .iter()
            .filter(|f| filter.as_ref().is_none_or(|g| g.is_match(f)))
            .collect();
        let mut out: Vec<String> = matching
            .iter()
            .take(MAX_LIST)
            .map(|f| f.to_string())
            .collect();
        if matching.len() > MAX_LIST {
            out.push(format!(
                "… and {} more; narrow the glob",
                matching.len() - MAX_LIST
            ));
        }
        if out.is_empty() {
            return Ok("No files match.".into());
        }
        Ok(out.join("\n"))
    }

    /// A file's lines, numbered, from `start` to `end` (1-based, inclusive).
    pub async fn read(
        &self,
        path: &str,
        start: Option<usize>,
        end: Option<usize>,
    ) -> Result<String, RepoError> {
        let path = self.in_scope(path)?;
        let text = tokio::fs::read_to_string(self.root.join(&path))
            .await
            .map_err(|e| RepoError(format!("can't read {path}: {e}")))?;
        let lines: Vec<&str> = text.lines().collect();
        let start = start.unwrap_or(1).max(1);
        let end = end
            .unwrap_or(start + MAX_READ_LINES - 1)
            .min(lines.len())
            .min(start + MAX_READ_LINES - 1);
        if start > lines.len() {
            return Ok(format!("{path} has only {} lines.", lines.len()));
        }
        let mut out = String::new();
        for (number, line) in lines.iter().enumerate().take(end).skip(start - 1) {
            out.push_str(&format!("{:>5}  {line}\n", number + 1));
            if out.len() > MAX_READ_BYTES {
                out.push_str("… cut off; read a smaller range\n");
                break;
            }
        }
        if end < lines.len() {
            out.push_str(&format!(
                "({path} has {} lines; this shows {start}-{end})\n",
                lines.len()
            ));
        }
        Ok(out)
    }

    /// Lines matching a regular expression (case-insensitive), as `path:line: text`.
    pub async fn search(&self, pattern: &str, glob: Option<&str>) -> Result<String, RepoError> {
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .size_limit(1 << 20)
            .build()
            .map_err(|e| RepoError(format!("invalid pattern: {e}")))?;
        let filter = glob
            .map(|g| glob_set(&[g.to_string()]))
            .transpose()?
            .flatten();
        let mut matches = Vec::new();
        for file in self
            .files
            .iter()
            .filter(|f| filter.as_ref().is_none_or(|g| g.is_match(f)))
        {
            let full = self.root.join(file);
            let Ok(meta) = tokio::fs::metadata(&full).await else {
                continue;
            };
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            // Binary or unreadable files are skipped.
            let Ok(text) = tokio::fs::read_to_string(&full).await else {
                continue;
            };
            for (number, line) in text.lines().enumerate() {
                if regex.is_match(line) {
                    let line: String = line.trim().chars().take(200).collect();
                    matches.push(format!("{file}:{}: {line}", number + 1));
                    if matches.len() >= MAX_MATCHES {
                        matches.push("… more matches; narrow the pattern or glob".into());
                        return Ok(matches.join("\n"));
                    }
                }
            }
        }
        if matches.is_empty() {
            return Ok("No matches.".into());
        }
        Ok(matches.join("\n"))
    }

    /// Recent commits touching the scope (or one path), newest first, with the files they changed.
    pub async fn log(&self, path: Option<&str>, limit: usize) -> Result<String, RepoError> {
        let limit = limit.clamp(1, 50).to_string();
        let mut args = vec![
            "log",
            "--date=short",
            "--format=%h %ad %an: %s",
            "--stat=120",
            "-n",
            &limit,
            "--",
        ];
        let path = path.map(|p| self.in_scope(p)).transpose()?;
        let specs: Vec<String> = match &path {
            Some(path) => vec![path.clone()],
            None => self.globs.iter().map(|g| format!(":(glob){g}")).collect(),
        };
        args.extend(specs.iter().map(String::as_str));
        let output = git(&self.root, &args).await?;
        Ok(cut(output, MAX_GIT_OUTPUT))
    }

    /// The full hash of the latest commit touching the scope, if any.
    pub async fn head(&self) -> Result<Option<String>, RepoError> {
        let specs: Vec<String> = self.globs.iter().map(|g| format!(":(glob){g}")).collect();
        let mut args = vec!["log", "-n", "1", "--format=%H", "--"];
        args.extend(specs.iter().map(String::as_str));
        let output = git(&self.root, &args).await?;
        let hash = output.trim();
        Ok((!hash.is_empty()).then(|| hash.to_string()))
    }

    /// One commit's message and diff, limited to the scope.
    pub async fn show(&self, commit: &str) -> Result<String, RepoError> {
        let commit = commit.trim();
        if commit.is_empty() || commit.len() > 40 || !commit.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(RepoError("give a commit hash, like a1b2c3d".into()));
        }
        let specs: Vec<String> = self.globs.iter().map(|g| format!(":(glob){g}")).collect();
        let mut args = vec![
            "show",
            "--format=%H%n%an %ad%n%n%B",
            "--date=short",
            commit,
            "--",
        ];
        args.extend(specs.iter().map(String::as_str));
        let output = git(&self.root, &args).await?;
        Ok(cut(output, MAX_GIT_OUTPUT))
    }

    /// A tracked file in scope, by relative path.
    fn in_scope(&self, path: &str) -> Result<String, RepoError> {
        let path = path.trim().trim_start_matches("./").to_string();
        if self.files.contains(&path) {
            Ok(path)
        } else {
            Err(RepoError(format!(
                "{path} isn't a tracked file in this service; use list_files"
            )))
        }
    }
}

/// Whether a path is in scope of a service's globs (everything, with none).
pub fn matches_any(globs: &[String], path: &str) -> bool {
    match glob_set(globs) {
        Ok(Some(set)) => set.is_match(path),
        Ok(None) => true,
        Err(_) => false,
    }
}

fn glob_set(globs: &[String]) -> Result<Option<GlobSet>, RepoError> {
    if globs.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        builder.add(Glob::new(glob).map_err(|e| RepoError(format!("invalid glob {glob:?}: {e}")))?);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| RepoError(e.to_string()))
}

async fn git(root: &Path, args: &[&str]) -> Result<String, RepoError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .await
        .map_err(|e| RepoError(format!("couldn't run git: {e}")))?;
    if !output.status.success() {
        return Err(RepoError(format!(
            "git {}: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn cut(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… cut off");
    }
    text
}

#[cfg(test)]
mod tests {
    use std::process::Command as StdCommand;

    use super::*;

    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        for (path, text) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        std::fs::write(dir.path().join(".gitignore"), ".env\n").unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=1\n").unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "Add the sync engine",
        ]);
        dir
    }

    #[tokio::test]
    async fn only_tracked_files_in_scope_are_visible() {
        let dir = repo_with(&[
            ("crates/sync/src/lib.rs", "pub fn apply() {}\n"),
            ("web/app.ts", "x\n"),
        ]);
        let repo = Repo::open(dir.path(), &["crates/sync/**".into()])
            .await
            .unwrap();

        assert_eq!(repo.list(None).unwrap(), "crates/sync/src/lib.rs");
        assert!(
            repo.read("crates/sync/src/lib.rs", None, None)
                .await
                .unwrap()
                .contains("pub fn apply")
        );
        assert!(
            repo.read("web/app.ts", None, None).await.is_err(),
            "outside the service"
        );
        assert!(
            repo.read(".env", None, None).await.is_err(),
            "ignored files stay hidden"
        );
        assert!(repo.read("../../etc/passwd", None, None).await.is_err());
    }

    #[tokio::test]
    async fn search_log_and_show_stay_in_scope() {
        let dir = repo_with(&[(
            "crates/sync/src/lib.rs",
            "// Applies transactions\npub fn apply() {}\n",
        )]);
        let repo = Repo::open(dir.path(), &["crates/sync/**".into()])
            .await
            .unwrap();

        assert_eq!(
            repo.search("applies", None).await.unwrap(),
            "crates/sync/src/lib.rs:1: // Applies transactions"
        );
        let log = repo.log(None, 5).await.unwrap();
        assert!(log.contains("Add the sync engine"), "{log}");
        let hash = log.split_whitespace().next().unwrap();
        let shown = repo.show(hash).await.unwrap();
        assert!(shown.contains("+pub fn apply"), "{shown}");
        assert!(repo.show("HEAD; rm -rf /").await.is_err());
        let head = repo.head().await.unwrap().unwrap();
        assert!(head.starts_with(hash) && head.len() == 40, "{head}");
    }

    #[tokio::test]
    async fn reads_are_numbered_and_bounded() {
        let text: String = (1..=1000).map(|n| format!("line {n}\n")).collect();
        let dir = repo_with(&[("big.txt", &text)]);
        let repo = Repo::open(dir.path(), &[]).await.unwrap();

        let first = repo.read("big.txt", None, None).await.unwrap();
        assert!(first.starts_with("    1  line 1\n"));
        assert!(first.contains("this shows 1-400"));
        let range = repo.read("big.txt", Some(998), Some(1200)).await.unwrap();
        assert!(range.contains(" 1000  line 1000"), "{range}");
    }
}
