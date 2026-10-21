use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError, invalid, text};

/// A repository with more than this many notes has stopped curating them.
pub const NOTES_PER_REPO: usize = 50;
/// In characters. A note is a fact, not a document.
pub const NOTE_MAX: usize = 500;
const PATHS_MAX: usize = 20;

/// What an agent learned about a repository that holds beyond one task,
/// kept for the next task on it, on anyone's Mac. A note that is wrong is
/// worse than none, so each carries the commit it was last checked at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Note {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// The repository, by its remote key (`github.com/acme/billing`), so a
    /// note outlives the repository's place in the workspace.
    pub repo: String,
    pub text: String,
    /// Files or folders it is about, relative to the repository's root.
    pub paths: Vec<String>,
    /// Where it came from: "Claude Code in ACME-12 Refunds…", or "you".
    pub source: String,
    /// The default branch's commit when it was written.
    pub written_commit: Option<String>,
    pub written_at: DateTime<Utc>,
    /// When someone last said it still holds. Writing and editing count.
    pub checked_at: DateTime<Utc>,
    pub checked_commit: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct NoteCreate {
    pub repo: String,
    pub text: String,
    pub paths: Option<Vec<String>>,
    pub source: Option<String>,
    pub written_commit: Option<String>,
    /// Now when left out; the desktop app sends it for notes it moves in.
    pub written_at: Option<DateTime<Utc>>,
    pub checked_at: Option<DateTime<Utc>>,
    pub checked_commit: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct NotePatch {
    pub text: Option<String>,
    pub paths: Option<Vec<String>>,
    pub checked_at: Option<DateTime<Utc>>,
    pub checked_commit: Option<String>,
}

impl Validate for NoteCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.repo, "A note's repository", 300)?;
        note_text(&self.text)?;
        self.paths.as_deref().map(paths).transpose()?;
        if let Some(source) = &self.source
            && source.chars().count() > 200
        {
            return invalid("A note's source is at most 200 characters.");
        }
        commit(&self.written_commit)?;
        commit(&self.checked_commit)
    }
}

impl Validate for NotePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.text.as_deref().map(note_text).transpose()?;
        self.paths.as_deref().map(paths).transpose()?;
        commit(&self.checked_commit)
    }
}

fn note_text(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid("A note needs some text.");
    }
    let n = value.chars().count();
    if n > NOTE_MAX {
        return invalid(format!(
            "A note is at most {NOTE_MAX} characters and this one is {n}; keep it to the one fact."
        ));
    }
    Ok(())
}

/// Relative, inside the repository: what git will be given.
fn paths(paths: &[String]) -> Result<(), ValidationError> {
    if paths.len() > PATHS_MAX {
        return invalid(format!("A note is about at most {PATHS_MAX} paths."));
    }
    for p in paths {
        if p.is_empty()
            || p.starts_with('/')
            || p.starts_with('-')
            || p.split('/').any(|s| s == "..")
        {
            return invalid(format!("{p:?} is not a path inside the repository."));
        }
    }
    Ok(())
}

fn commit(value: &Option<String>) -> Result<(), ValidationError> {
    match value {
        Some(sha)
            if !(4..=64).contains(&sha.len()) || !sha.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            invalid("A commit is its hash.")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_one_fact() {
        assert!(note_text("Tests need `bun run db` first.").is_ok());
        assert!(note_text(" ").is_err());
        assert!(note_text(&"x".repeat(NOTE_MAX + 1)).is_err());
    }

    #[test]
    fn a_note_is_about_paths_inside_the_repository() {
        assert!(paths(&["src/app.ts".into(), "docs".into()]).is_ok());
        for bad in ["/etc/passwd", "../x", "-rf", ""] {
            assert!(paths(&[bad.into()]).is_err(), "{bad}");
        }
    }
}
