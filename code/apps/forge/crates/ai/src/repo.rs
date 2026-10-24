//! Read-only access to a git repository's tracked files, for the AI.
//!
//! Only regular blobs in git’s index can be listed or read: that keeps ignored files
//! such as `.env` out of reach. A service's path globs narrow it further.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::RegexBuilder;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RepoError(pub String);

pub struct Repo {
    root: PathBuf,
    /// Tracked regular files and their indexed blob ids, sorted by path.
    files: BTreeMap<String, String>,
    /// The scope's globs, for git commands.
    globs: Vec<String>,
    reader: Mutex<Option<BlobReader>>,
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
        let output = git(root, &["ls-files", "--stage", "-z"]).await?;
        let scope = glob_set(globs)?;
        let files = output
            .split('\0')
            .filter_map(|entry| {
                let (header, path) = entry.split_once('\t')?;
                let mut fields = header.split_whitespace();
                let mode = fields.next()?;
                let blob = fields.next()?;
                let stage = fields.next()?;
                // Read regular tracked blobs, never symlinks or submodules.
                (matches!(mode, "100644" | "100755")
                    && stage == "0"
                    && scope.as_ref().is_none_or(|s| s.is_match(path)))
                .then(|| (path.to_string(), blob.to_string()))
            })
            .collect();
        Ok(Repo {
            root: root.to_path_buf(),
            files,
            globs: globs.to_vec(),
            reader: Mutex::new(None),
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
            .keys()
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
        let text = self.contents(&path).await?;
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
            .keys()
            .filter(|f| filter.as_ref().is_none_or(|g| g.is_match(f)))
        {
            let Ok(text) = self.contents(file).await else {
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

    /// Read the indexed blob, not the working-tree path. Even a regular file
    /// replaced with a symlink after opening cannot escape the repository.
    async fn contents(&self, path: &str) -> Result<String, RepoError> {
        let blob = self
            .files
            .get(path)
            .ok_or_else(|| RepoError("file is out of scope".into()))?;
        let mut reader = self.reader.lock().await;
        if reader.is_none() {
            *reader = Some(BlobReader::open(&self.root).map_err(|e| RepoError(e.to_string()))?);
        }
        let mut worker = reader.take().expect("opened above");
        let result = worker.read(blob).await;
        if result.is_ok() {
            *reader = Some(worker);
        }
        result.map_err(|e| RepoError(format!("can't read {path}: {e}")))
    }

    /// A tracked file in scope, by relative path.
    fn in_scope(&self, path: &str) -> Result<String, RepoError> {
        let path = path.trim().trim_start_matches("./").to_string();
        if self.files.contains_key(&path) {
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

/// One bounded, serialized pipe per repository; Git alone resolves object ids.
/// Dropping the repository kills the worker, including during cancellation.
struct BlobReader {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl BlobReader {
    fn open(root: &Path) -> std::io::Result<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("cat-file")
            .arg("--batch-command")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        Ok(Self {
            input: child.stdin.take().expect("piped stdin"),
            output: BufReader::new(child.stdout.take().expect("piped stdout")),
            _child: child,
        })
    }

    async fn header(&mut self, command: &str, blob: &str) -> std::io::Result<usize> {
        self.input
            .write_all(format!("{command} {blob}\n").as_bytes())
            .await?;
        self.input.flush().await?;
        let mut header = String::new();
        self.output.read_line(&mut header).await?;
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[0] != blob || fields[1] != "blob" {
            return Err(std::io::Error::other("git returned an invalid blob header"));
        }
        fields[2].parse().map_err(std::io::Error::other)
    }

    async fn read(&mut self, blob: &str) -> std::io::Result<String> {
        let size = self.header("info", blob).await?;
        if size as u64 > MAX_FILE_BYTES {
            return Err(std::io::Error::other(
                "file is too large; read a smaller tracked file",
            ));
        }
        if self.header("contents", blob).await? != size {
            return Err(std::io::Error::other("git blob size changed"));
        }
        let mut bytes = vec![0; size + 1];
        self.output.read_exact(&mut bytes).await?;
        if bytes.pop() != Some(b'\n') {
            return Err(std::io::Error::other("invalid git blob terminator"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
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
    #[cfg(unix)]
    #[tokio::test]
    async fn links_and_working_tree_replacements_cannot_expose_untracked_files() {
        use std::os::unix::fs::symlink;
        let dir = repo_with(&[("safe.txt", "tracked text\n")]);
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), "EXTERNAL_SECRET").unwrap();
        symlink(outside.path(), dir.path().join("outside.txt")).unwrap();
        symlink(".env", dir.path().join("ignored.txt")).unwrap();
        assert!(
            StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["add", "outside.txt", "ignored.txt"])
                .status()
                .unwrap()
                .success()
        );
        let repo = Repo::open(dir.path(), &[]).await.unwrap();
        assert!(repo.read("outside.txt", None, None).await.is_err());
        assert!(repo.read("ignored.txt", None, None).await.is_err());
        std::fs::remove_file(dir.path().join("safe.txt")).unwrap();
        symlink(outside.path(), dir.path().join("safe.txt")).unwrap();
        assert!(
            repo.read("safe.txt", None, None)
                .await
                .unwrap()
                .contains("tracked text")
        );
        assert_eq!(repo.search("SECRET", None).await.unwrap(), "No matches.");
    }
    #[tokio::test]
    async fn batch_search_handles_many_blobs_and_recovers_after_an_oversized_one() {
        let dir = repo_with(&[("first.txt", "small text")]);
        for i in 0..500 {
            std::fs::write(
                dir.path().join(format!("file-{i}.txt")),
                format!("content {i}"),
            )
            .unwrap();
        }
        std::fs::write(
            dir.path().join("huge.txt"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(
            StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["add", "."])
                .status()
                .unwrap()
                .success()
        );
        let repo = Repo::open(dir.path(), &[]).await.unwrap();
        let start = std::time::Instant::now();
        assert_eq!(
            repo.search("absent-pattern", None).await.unwrap(),
            "No matches."
        );
        eprintln!("502-file batch search: {:?}", start.elapsed());
        assert!(repo.read("huge.txt", None, None).await.is_err());
        assert!(
            repo.read("file-499.txt", None, None)
                .await
                .unwrap()
                .contains("content 499")
        );
        let (a, b) = tokio::join!(
            repo.read("file-1.txt", None, None),
            repo.read("file-2.txt", None, None)
        );
        assert!(a.unwrap().contains("content 1"));
        assert!(b.unwrap().contains("content 2"));
    }
}
