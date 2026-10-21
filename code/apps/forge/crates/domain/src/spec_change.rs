//! Spec changes (decision "Specs change only when the work is done", FOR-55):
//! an issue carries what it changes in the specs, and the change takes effect
//! when the issue is done, when Forge applies it (FOR-74). A change to a
//! spec's requirements follows
//! OpenSpec's delta format as-is: `## ADDED Requirements`, `## MODIFIED
//! Requirements`, `## REMOVED Requirements` and `## RENAMED Requirements`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, optional};
use crate::verdict::{HEADING, Requirement, requirement_lines, requirements};

/// What an issue changes in the specs: per spec, and the decisions it
/// replaces. Or, with no spec changes, why none changes. One per issue.
/// Written through the server only (agents use the MCP tools).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpecChange {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// None once the issue is deleted: a spec change that took effect stays,
    /// as part of its specs' history.
    pub issue_id: Option<Uuid>,
    pub specs: Vec<SpecDelta>,
    /// Decisions the issue replaces: superseded when it takes effect.
    pub replaces_decision_ids: Vec<Uuid>,
    /// Why no spec changes, when none does.
    pub no_change_reason: Option<String>,
    /// When it took effect: when the issue was done. Set by the server.
    pub applied_at: Option<DateTime<Utc>>,
    /// The specs it changed or created then.
    pub applied_spec_ids: Vec<Uuid>,
    /// The specs it couldn't change then, because they no longer fit it.
    pub not_applied: Vec<NotApplied>,
    /// What it was when work on the issue started, so a change afterwards
    /// shows. Set by the server; none before work starts.
    #[serde(default)]
    pub as_started: Option<StartedWith>,
    /// Set by the server.
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An issue's spec change as it was when work on the issue started. Empty
/// for one written only after that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartedWith {
    pub specs: Vec<SpecDelta>,
    pub no_change_reason: Option<String>,
    #[serde(default)]
    pub replaces_decision_ids: Vec<Uuid>,
}

impl SpecChange {
    /// Whether it's written: it changes a spec, says why none changes, or
    /// already took effect. Naming replaced decisions alone isn't enough.
    pub fn is_written(&self) -> bool {
        !self.specs.is_empty() || self.no_change_reason.is_some() || self.applied_at.is_some()
    }

    /// Whether it says something else now than when work on its issue
    /// started: other specs, other text, or another reason for none.
    pub fn changed_after_start(&self) -> bool {
        let Some(started) = &self.as_started else {
            return false;
        };
        let what = |d: &SpecDelta| {
            (
                d.spec_id,
                d.new_title.clone(),
                d.new_service_id,
                d.text.clone(),
                d.rewrite,
            )
        };
        // In any order: saving parts again can reorder them.
        let sorted = |parts: &[SpecDelta]| {
            let mut all: Vec<_> = parts.iter().map(what).collect();
            all.sort();
            all
        };
        let decisions = |ids: &[Uuid]| {
            let mut all = ids.to_vec();
            all.sort();
            all
        };
        started.no_change_reason != self.no_change_reason
            || decisions(&started.replaces_decision_ids) != decisions(&self.replaces_decision_ids)
            || sorted(&started.specs) != sorted(&self.specs)
    }
}

/// What an issue changes in one spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SpecDelta {
    /// The spec it changes; none for a new spec.
    pub spec_id: Option<Uuid>,
    /// A new spec's title.
    pub new_title: Option<String>,
    /// A new spec's service.
    pub new_service_id: Option<Uuid>,
    /// OpenSpec's delta sections, or with `rewrite` the spec's whole new text.
    pub text: String,
    /// `text` is the whole spec: a rewrite, or a new spec.
    pub rewrite: bool,
    /// When this part was first written, and kept when it's written again.
    /// Of an issue rewriting a spec and another changing it, the one written
    /// first goes first. Set by the server.
    #[serde(default)]
    pub written_at: Option<DateTime<Utc>>,
    /// When the part first touched each requirement it changes, removes,
    /// renames or adds, by name. Of two open issues touching the same one,
    /// the one that touched it first goes first, so widening a part later
    /// never jumps ahead. Set by the server.
    #[serde(default)]
    #[ts(type = "Record<string, string>")]
    pub touched_at: std::collections::BTreeMap<String, DateTime<Utc>>,
    /// The spec's text when this part was last written, to tell whether
    /// what it changes has changed since. Set by the server.
    #[serde(default)]
    pub based_on: Option<String>,
}

/// A spec an issue's spec change couldn't change when the issue was done,
/// and why. Forge doesn't guess: it leaves that spec as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct NotApplied {
    /// The spec; none for a new one.
    pub spec_id: Option<Uuid>,
    /// Its title.
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpecChangeCreate {
    pub issue_id: Uuid,
    pub specs: Option<Vec<SpecDelta>>,
    pub replaces_decision_ids: Option<Vec<Uuid>>,
    pub no_change_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpecChangePatch {
    /// Only ever cleared, by the server, when the issue of a spec change that
    /// took effect is deleted.
    pub issue_id: Option<Option<Uuid>>,
    pub specs: Option<Vec<SpecDelta>>,
    pub replaces_decision_ids: Option<Vec<Uuid>>,
    pub no_change_reason: Option<Option<String>>,
    pub applied_at: Option<DateTime<Utc>>,
    pub applied_spec_ids: Option<Vec<Uuid>>,
    pub not_applied: Option<Vec<NotApplied>>,
    pub as_started: Option<Option<StartedWith>>,
}

/// A spec change can't both change specs and say nothing changes.
fn check(specs: &[SpecDelta], no_change_reason: Option<&String>) -> Result<(), ValidationError> {
    if specs.len() > 50 {
        return rules::invalid("a spec change can change at most 50 specs");
    }
    for delta in specs {
        delta.validate()?;
    }
    for (i, delta) in specs.iter().enumerate() {
        let same = |other: &SpecDelta| match (delta.spec_id, other.spec_id) {
            (Some(a), Some(b)) => a == b,
            (None, None) => delta
                .new_title
                .as_deref()
                .zip(other.new_title.as_deref())
                .is_some_and(|(a, b)| same_spec_name(a, b)),
            _ => false,
        };
        if specs[..i].iter().any(same) {
            return rules::invalid("a spec change names the same spec twice");
        }
    }
    if let Some(reason) = no_change_reason {
        rules::text(reason, "the reason for no spec change", rules::DESCRIPTION)?;
        if !specs.is_empty() {
            return rules::invalid("a spec change can't change specs and say no spec changes");
        }
    }
    Ok(())
}

impl Validate for SpecDelta {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.text, "spec change", rules::DESCRIPTION)?;
        match (self.spec_id, &self.new_title) {
            (Some(_), None) if self.new_service_id.is_none() => {}
            (Some(_), _) => {
                return rules::invalid("a change to an existing spec has no new title or service");
            }
            (None, Some(title)) => {
                rules::text(title, "the new spec's title", rules::TITLE)?;
                if !self.rewrite {
                    return rules::invalid("a new spec is written as its whole text");
                }
            }
            (None, None) => return rules::invalid("say which spec changes, or name a new one"),
        }
        if self.rewrite {
            Ok(())
        } else {
            parse_delta(&self.text)
                .map(|_| ())
                .map_err(|problems| ValidationError(problems.join(" ")))
        }
    }
}

impl Validate for SpecChangeCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        check(
            self.specs.as_deref().unwrap_or_default(),
            self.no_change_reason.as_ref(),
        )
    }
}

impl Validate for SpecChangePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.specs, |specs| check(specs, None))?;
        if let Some(Some(reason)) = &self.no_change_reason {
            rules::text(reason, "the reason for no spec change", rules::DESCRIPTION)?;
        }
        Ok(())
    }
}

/// A change to a spec's requirements, from OpenSpec's delta sections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    pub added: Vec<Requirement>,
    pub modified: Vec<Requirement>,
    /// Names of the requirements removed.
    pub removed: Vec<String>,
    /// From and to names.
    pub renamed: Vec<(String, String)>,
}

impl Delta {
    /// The requirements the spec must have for the delta to apply:
    /// modified, removed and renamed ones.
    pub fn needs(&self) -> impl Iterator<Item = &str> {
        self.modified
            .iter()
            .map(|r| r.name.as_str())
            .chain(self.removed.iter().map(String::as_str))
            .chain(self.renamed.iter().map(|(from, _)| from.as_str()))
    }

    /// The names the delta brings into the spec: added ones and new names.
    pub fn brings(&self) -> impl Iterator<Item = &str> {
        self.added
            .iter()
            .map(|r| r.name.as_str())
            .chain(self.renamed.iter().map(|(_, to)| to.as_str()))
    }
}

const SECTIONS: [&str; 4] = [
    "ADDED Requirements",
    "MODIFIED Requirements",
    "REMOVED Requirements",
    "RENAMED Requirements",
];

/// Reads OpenSpec's delta sections, or says in plain sentences what keeps
/// them from being a delta.
pub fn parse_delta(text: &str) -> Result<Delta, Vec<String>> {
    let mut sections: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut problems = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with("## ") {
            let title = line[3..].trim();
            match SECTIONS.iter().find(|s| s.eq_ignore_ascii_case(title)) {
                Some(section) => sections.push((section, Vec::new())),
                None => problems.push(format!(
                    "\"## {title}\" isn't a delta section; use {}.",
                    quoted_sections()
                )),
            }
            continue;
        }
        match sections.last_mut() {
            Some((_, lines)) => lines.push(line),
            // Said once, however much text comes before the first section.
            None if !line.trim().is_empty() && problems.is_empty() => {
                problems.push(format!(
                    "Everything goes under a delta section: {}.",
                    quoted_sections()
                ));
            }
            None => {}
        }
    }

    let mut delta = Delta::default();
    for (section, lines) in &sections {
        let body = lines.join("\n");
        match *section {
            "ADDED Requirements" => delta.added.extend(requirements(&body)),
            "MODIFIED Requirements" => delta.modified.extend(requirements(&body)),
            "REMOVED Requirements" => {
                delta
                    .removed
                    .extend(requirements(&body).into_iter().map(|r| r.name));
            }
            _ => renames(&body, &mut delta.renamed, &mut problems),
        }
    }
    let empty = delta.added.is_empty()
        && delta.modified.is_empty()
        && delta.removed.is_empty()
        && delta.renamed.is_empty();
    if empty && problems.is_empty() {
        problems.push(format!(
            "It changes no requirement: list them as \"### Requirement: <name>\" under {}.",
            quoted_sections()
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for name in delta.needs().chain(delta.brings()) {
        if seen.contains(&name) {
            problems.push(format!("It names requirement \"{name}\" twice."));
        } else {
            seen.push(name);
        }
    }
    if problems.is_empty() {
        Ok(delta)
    } else {
        Err(problems)
    }
}

/// A spec's text with a delta applied, in OpenSpec's order: renamed, removed,
/// modified, then added requirements, which go after the spec's last
/// requirement. When the spec no longer fits the delta (a requirement it
/// changes is gone, or one it adds is already there), says what doesn't fit
/// instead, and nothing changes.
pub fn apply_delta(body: &str, delta: &Delta) -> Result<String, Vec<String>> {
    let mut lines: Vec<String> = body.lines().map(Into::into).collect();
    let current: Vec<String> = requirement_lines(&lines)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let has = |name: &str| current.iter().any(|c| c == name);
    let mut problems: Vec<String> = Vec::new();
    for name in delta.needs().filter(|n| !has(n)) {
        problems.push(format!("It has no requirement \"{name}\" any more."));
    }
    for name in delta.brings().filter(|n| has(n)) {
        problems.push(format!("It already has a requirement \"{name}\"."));
    }
    if current.is_empty() && requirements_section(&lines).is_none() {
        problems.push("It isn't written in OpenSpec's convention any more.".into());
    }
    if !problems.is_empty() {
        return Err(problems);
    }

    let find = |lines: &[String], name: &str| {
        requirement_lines(lines)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, range)| range)
            .expect("checked above")
    };
    for (from, to) in &delta.renamed {
        let range = find(&lines, from);
        lines[range.start] = format!("{HEADING} {to}");
    }
    for name in &delta.removed {
        let mut range = find(&lines, name);
        // With the blank lines before it, so no gap is left.
        while range.start > 0 && lines[range.start - 1].trim().is_empty() {
            range.start -= 1;
        }
        lines.drain(range);
    }
    for requirement in &delta.modified {
        let range = find(&lines, &requirement.name);
        lines.splice(range, requirement.text.lines().map(String::from));
    }
    for requirement in &delta.added {
        let at = match requirement_lines(&lines).last() {
            Some((_, range)) => range.end,
            None => requirements_section(&lines).map_or(lines.len(), |section| section.end),
        };
        let new = std::iter::once(String::new()).chain(requirement.text.lines().map(String::from));
        lines.splice(at..at, new);
    }

    let mut text = lines.join("\n");
    if body.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// The lines of a spec's `## Requirements` section, from its heading to its
/// last line that isn't blank.
fn requirements_section(lines: &[String]) -> Option<std::ops::Range<usize>> {
    let mut fenced = false;
    let mut section: Option<std::ops::Range<usize>> = None;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let level_two = !fenced && line.starts_with("## ");
        match section.as_mut() {
            Some(_) if level_two || (!fenced && line.starts_with("# ")) => break,
            Some(range) if !line.trim().is_empty() => range.end = index + 1,
            Some(_) => {}
            None if level_two && line[3..].trim().eq_ignore_ascii_case("Requirements") => {
                section = Some(index..index + 1);
            }
            None => {}
        }
    }
    section
}

fn quoted_sections() -> String {
    SECTIONS
        .iter()
        .map(|s| format!("\"## {s}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `- FROM: `### Requirement: Old`` followed by `- TO: `### Requirement: New``.
fn renames(body: &str, found: &mut Vec<(String, String)>, problems: &mut Vec<String>) {
    let mut from: Option<String> = None;
    for line in body.lines() {
        let line = line.trim().trim_start_matches(['-', '*']).trim();
        if let Some(name) = strip_label(line, "FROM:") {
            if let Some(dangling) = from.replace(name) {
                problems.push(format!("Renaming \"{dangling}\" has no TO."));
            }
        } else if let Some(name) = strip_label(line, "TO:") {
            match from.take() {
                Some(old) => found.push((old, name)),
                None => problems.push(format!("Renaming to \"{name}\" has no FROM.")),
            }
        }
    }
    if let Some(dangling) = from {
        problems.push(format!("Renaming \"{dangling}\" has no TO."));
    }
}

fn strip_label(line: &str, label: &str) -> Option<String> {
    let rest = line.strip_prefix(label)?.trim().trim_matches('`').trim();
    let name = rest.strip_prefix("### Requirement:").unwrap_or(rest).trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Two open issues whose spec changes change the same requirement (decision
/// "A spec change is written first, and only one issue changes a
/// requirement at a time"). The issue that came second can't start until
/// the two settle it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SpecConflict {
    pub spec_id: Uuid,
    /// The requirement both change; none when one of them rewrites the
    /// whole spec, which changes everything in it.
    pub requirement: Option<String>,
    /// The spec change that changed it first.
    pub first: Uuid,
    /// The spec change that came after.
    pub second: Uuid,
}

/// The requirements a part changes, removes, renames or adds, by name;
/// `None` for a rewrite or a new spec, and for text that isn't a delta.
pub fn touched(delta: &SpecDelta) -> Option<Vec<String>> {
    if delta.rewrite || delta.spec_id.is_none() {
        return None;
    }
    let parsed = parse_delta(&delta.text).ok()?;
    Some(
        parsed
            .needs()
            .chain(parsed.brings())
            .map(Into::into)
            .collect(),
    )
}

/// What a part of a spec change touches in its spec.
enum Touch {
    /// A rewrite: everything.
    Whole,
    /// The requirements it changes, removes, renames or adds, by name.
    Names(Vec<String>),
}

/// Where the open issues' spec changes (`open`: not done or canceled yet,
/// not taken effect) change the same requirement of a spec, or one rewrites
/// a spec another changes. Adding different requirements never conflicts.
pub fn spec_conflicts(open: &[SpecChange]) -> Vec<SpecConflict> {
    struct Part<'a> {
        when: DateTime<Utc>,
        change: Uuid,
        spec: Uuid,
        touch: Touch,
        delta: &'a SpecDelta,
    }
    let mut parts: Vec<Part> = Vec::new();
    for change in open {
        for delta in &change.specs {
            let Some(spec) = delta.spec_id else { continue };
            let touch = if delta.rewrite {
                Touch::Whole
            } else {
                let Some(names) = touched(delta) else {
                    continue;
                };
                Touch::Names(names)
            };
            parts.push(Part {
                when: delta.written_at.unwrap_or(change.created_at),
                change: change.id,
                spec,
                touch,
                delta,
            });
        }
    }
    parts.sort_by_key(|p| (p.when, p.change));
    // When a part first touched a requirement; older parts don't say, and
    // count from when they were written.
    let since = |p: &Part, name: &str| {
        let at = p.delta.touched_at.get(name).copied().unwrap_or(p.when);
        (at, p.change)
    };
    let mut conflicts = Vec::new();
    for (i, b) in parts.iter().enumerate() {
        for a in parts[..i]
            .iter()
            .filter(|a| a.spec == b.spec && a.change != b.change)
        {
            let conflict =
                |requirement: Option<String>, first: &Part, second: &Part| SpecConflict {
                    spec_id: b.spec,
                    requirement,
                    first: first.change,
                    second: second.change,
                };
            match (&a.touch, &b.touch) {
                (Touch::Names(x), Touch::Names(y)) => {
                    for name in y.iter().filter(|name| x.contains(name)) {
                        let (first, second) = if since(a, name) <= since(b, name) {
                            (a, b)
                        } else {
                            (b, a)
                        };
                        conflicts.push(conflict(Some(name.clone()), first, second));
                    }
                }
                _ => conflicts.push(conflict(None, a, b)),
            }
        }
    }
    conflicts
}

/// A spec's name as Forge compares names: ignoring case and surrounding
/// spaces, as the database does when a new spec is created.
pub fn spec_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether two spec names are the same name.
pub fn same_spec_name(a: &str, b: &str) -> bool {
    spec_name_key(a) == spec_name_key(b)
}

/// Two open issues whose spec changes write a new spec of the same name
/// (ignoring case and surrounding spaces): the one that wrote it first
/// writes it, and the other waits, as with a requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSpecHold {
    /// The new spec's name, as the one that waits wrote it.
    pub title: String,
    pub first: Uuid,
    pub second: Uuid,
}

/// Where the open issues' spec changes (`open`, as for `spec_conflicts`)
/// write a new spec of the same name.
pub fn new_spec_holds(open: &[SpecChange]) -> Vec<NewSpecHold> {
    let parts: Vec<(&SpecChange, &SpecDelta, String)> = open
        .iter()
        .flat_map(|c| c.specs.iter().map(move |d| (c, d)))
        .filter(|(_, d)| d.spec_id.is_none())
        .filter_map(|(c, d)| Some((c, d, spec_name_key(d.new_title.as_deref()?))))
        .collect();
    let since = |c: &SpecChange, d: &SpecDelta| (d.written_at.unwrap_or(c.created_at), c.id);
    let mut holds = Vec::new();
    for (i, (b, db, title)) in parts.iter().enumerate() {
        for (a, da, _) in parts[..i]
            .iter()
            .filter(|(a, _, other)| a.id != b.id && other == title)
        {
            let ((first, _), (second, d)) = if since(a, da) <= since(b, db) {
                ((a, da), (b, db))
            } else {
                ((b, db), (a, da))
            };
            holds.push(NewSpecHold {
                title: d.new_title.as_deref().unwrap_or_default().trim().into(),
                first: first.id,
                second: second.id,
            });
        }
    }
    holds
}

/// The spec changes `change` waits for (`holds`: who waits for whom, as
/// first and second, from `spec_conflicts` and `new_spec_holds`) that it
/// also goes before, directly or through others: they'd hold each other
/// back and none could ever start. Forge refuses the write that would make
/// such a loop; taking parts out or reopening an issue never makes one, so
/// the queue stays as it was written.
pub fn held_in_a_loop(holds: &[(Uuid, Uuid)], change: Uuid) -> Vec<Uuid> {
    let goes_before = |behind: Uuid| {
        let mut seen = vec![change];
        let mut next = vec![change];
        while let Some(x) = next.pop() {
            for &(_, second) in holds.iter().filter(|(first, _)| *first == x) {
                if second == behind {
                    return true;
                }
                if !seen.contains(&second) {
                    seen.push(second);
                    next.push(second);
                }
            }
        }
        false
    };
    let mut looped: Vec<Uuid> = holds
        .iter()
        .filter(|&&(first, second)| second == change && goes_before(first))
        .map(|&(first, _)| first)
        .collect();
    looped.sort();
    looped.dedup();
    looped
}

/// Who waits for whom among the open issues' spec changes, as first and
/// second: changes to the same requirement, and new specs of the same name.
pub fn holds(open: &[SpecChange]) -> Vec<(Uuid, Uuid)> {
    spec_conflicts(open)
        .into_iter()
        .map(|c| (c.first, c.second))
        .chain(
            new_spec_holds(open)
                .into_iter()
                .map(|h| (h.first, h.second)),
        )
        .collect()
}

/// What changed in a spec after a part of a spec change was written
/// against it, which taking effect would undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedSince {
    /// The spec a rewrite replaces changed.
    Spec,
    /// The requirements the part changes, removes or renames whose text
    /// changed, by name.
    Requirements(Vec<String>),
}

/// What changed in the spec (`body`, as it is now) since the part was
/// written; `None` when nothing it relies on did, or it doesn't say what it
/// was written against.
pub fn changed_since(delta: &SpecDelta, body: &str) -> Option<ChangedSince> {
    let based_on = delta.based_on.as_deref()?;
    if delta.rewrite {
        return (based_on.trim_end() != body.trim_end()).then_some(ChangedSince::Spec);
    }
    let parsed = parse_delta(&delta.text).ok()?;
    let (then, now) = (requirements(based_on), requirements(body));
    let text = |list: &[Requirement], name: &str| {
        list.iter().find(|r| r.name == name).map(|r| r.text.clone())
    };
    let changed: Vec<String> = parsed
        .needs()
        .filter(|name| text(&then, name) != text(&now, name))
        // A name it adds (or renames to) that the spec has gained since.
        .chain(
            parsed
                .brings()
                .filter(|name| text(&then, name).is_none() && text(&now, name).is_some()),
        )
        .map(Into::into)
        .collect();
    (!changed.is_empty()).then_some(ChangedSince::Requirements(changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELTA: &str = "\
## ADDED Requirements

### Requirement: Limits reset
The limit SHALL reset every minute.

#### Scenario: A new minute
- **WHEN** a minute passes
- **THEN** the token can send 100 requests again

## MODIFIED Requirements

### Requirement: Tokens are limited
The API SHALL allow 200 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 201st request within a minute
- **THEN** it gets 429

## REMOVED Requirements

### Requirement: Old limit
**Reason**: Replaced by the per-token limit.

## RENAMED Requirements

- FROM: `### Requirement: Login`
- TO: `### Requirement: Signing in`
";

    #[test]
    fn reads_each_delta_section() {
        let delta = parse_delta(DELTA).unwrap();
        let names = |rs: &[Requirement]| rs.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&delta.added), ["Limits reset"]);
        assert_eq!(names(&delta.modified), ["Tokens are limited"]);
        assert!(delta.modified[0].text.contains("201st request"));
        assert_eq!(delta.removed, ["Old limit"]);
        assert_eq!(
            delta.renamed,
            [("Login".to_string(), "Signing in".to_string())]
        );
        assert_eq!(
            delta.needs().collect::<Vec<_>>(),
            ["Tokens are limited", "Old limit", "Login"]
        );
        assert_eq!(
            delta.brings().collect::<Vec<_>>(),
            ["Limits reset", "Signing in"]
        );
    }

    #[test]
    fn says_what_isnt_a_delta() {
        assert_eq!(
            parse_delta("## Purpose\n\nRate limits.\n").unwrap_err(),
            [
                "\"## Purpose\" isn't a delta section; use \"## ADDED Requirements\", \"## MODIFIED Requirements\", \"## REMOVED Requirements\", \"## RENAMED Requirements\"."
            ]
        );
        assert!(parse_delta("Just text.").unwrap_err()[0].starts_with("Everything goes under"));
        assert!(
            parse_delta("## ADDED Requirements\n\nNothing yet.\n").unwrap_err()[0]
                .starts_with("It changes no requirement")
        );
        assert_eq!(
            parse_delta("## RENAMED Requirements\n- FROM: `### Requirement: A`\n").unwrap_err(),
            ["Renaming \"A\" has no TO."]
        );
        let twice = "## ADDED Requirements\n### Requirement: A\nIt SHALL.\n## REMOVED Requirements\n### Requirement: A\n";
        assert_eq!(
            parse_delta(twice).unwrap_err(),
            ["It names requirement \"A\" twice."]
        );
    }

    const SPEC: &str = "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429

### Requirement: Old limit
The API SHALL allow 1000 requests a day.

#### Scenario: A day
- **WHEN** a day passes
- **THEN** the count starts over

### Requirement: Login
People SHALL sign in with a password.

#### Scenario: Wrong password
- **WHEN** someone gives the wrong password
- **THEN** they aren't signed in

## Notes

Not a requirement.
";

    #[test]
    fn applies_each_delta_section_in_place() {
        let applied = apply_delta(SPEC, &parse_delta(DELTA).unwrap()).unwrap();
        assert_eq!(
            applied,
            "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 200 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 201st request within a minute
- **THEN** it gets 429

### Requirement: Signing in
People SHALL sign in with a password.

#### Scenario: Wrong password
- **WHEN** someone gives the wrong password
- **THEN** they aren't signed in

### Requirement: Limits reset
The limit SHALL reset every minute.

#### Scenario: A new minute
- **WHEN** a minute passes
- **THEN** the token can send 100 requests again

## Notes

Not a requirement.
"
        );
    }

    #[test]
    fn adds_the_first_requirement_to_the_requirements_section() {
        let spec = "## Purpose\n\nRate limits.\n\n## Requirements\n\n## Notes\n\nLater.\n";
        let added = "## ADDED Requirements\n\n### Requirement: A\nIt SHALL.\n\n#### Scenario: S\n- **WHEN** x\n- **THEN** y\n";
        assert_eq!(
            apply_delta(spec, &parse_delta(added).unwrap()).unwrap(),
            "## Purpose\n\nRate limits.\n\n## Requirements\n\n### Requirement: A\nIt SHALL.\n\n#### Scenario: S\n- **WHEN** x\n- **THEN** y\n\n## Notes\n\nLater.\n"
        );
    }

    #[test]
    fn a_spec_that_no_longer_fits_stays_as_it_is() {
        let spec = SPEC.replace("Requirement: Login", "Requirement: Signing in");
        assert_eq!(
            apply_delta(&spec, &parse_delta(DELTA).unwrap()).unwrap_err(),
            [
                "It has no requirement \"Login\" any more.",
                "It already has a requirement \"Signing in\"."
            ]
        );
        assert_eq!(
            apply_delta("# Overview\n\nProse.\n", &parse_delta(DELTA).unwrap()).unwrap_err()[3],
            "It isn't written in OpenSpec's convention any more."
        );
    }

    fn delta(
        spec_id: Option<Uuid>,
        new_title: Option<&str>,
        text: &str,
        rewrite: bool,
    ) -> SpecDelta {
        SpecDelta {
            spec_id,
            new_title: new_title.map(Into::into),
            new_service_id: None,
            text: text.into(),
            rewrite,
            written_at: None,
            touched_at: Default::default(),
            based_on: None,
        }
    }

    const PASSWORDS: &str = "\
## Requirements

### Requirement: Minimum length
Passwords SHALL have at least 3 characters.

#### Scenario: Too short
- **WHEN** someone picks \"ab\"
- **THEN** it is refused
";

    fn modifies(name: &str, rule: &str) -> String {
        format!(
            "## MODIFIED Requirements\n\n### Requirement: {name}\n{rule}\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n"
        )
    }

    fn adds(name: &str) -> String {
        format!(
            "## ADDED Requirements\n\n### Requirement: {name}\nPasswords SHALL obey it.\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n"
        )
    }

    /// An open spec change, its part for `spec` first written `minute` minutes in.
    fn open_change(spec: Uuid, text: &str, rewrite: bool, minute: i64) -> SpecChange {
        let at = DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap()
            + chrono::TimeDelta::minutes(minute);
        SpecChange {
            id: Uuid::now_v7(),
            workspace_id: Uuid::nil(),
            issue_id: Some(Uuid::now_v7()),
            specs: vec![SpecDelta {
                written_at: Some(at),
                ..delta(Some(spec), None, text, rewrite)
            }],
            replaces_decision_ids: vec![],
            no_change_reason: None,
            applied_at: None,
            applied_spec_ids: vec![],
            not_applied: vec![],
            as_started: None,
            created_by: Uuid::nil(),
            created_at: at,
            updated_at: at,
        }
    }

    #[test]
    fn a_spec_change_says_whether_it_changed_after_work_started() {
        let spec = Uuid::now_v7();
        let mut change = open_change(spec, &modifies("Minimum length", "At least 8."), false, 0);
        assert!(!change.changed_after_start(), "not started yet");
        change.as_started = Some(StartedWith {
            specs: change.specs.clone(),
            no_change_reason: None,
            replaces_decision_ids: vec![],
        });
        assert!(!change.changed_after_start());
        // Written again with the same text against a newer spec: the same.
        change.specs[0].based_on = Some(PASSWORDS.into());
        assert!(!change.changed_after_start());
        change.specs[0].text = modifies("Minimum length", "At least 12.");
        assert!(change.changed_after_start());
        // One written only after work started had nothing then.
        change.as_started = Some(StartedWith {
            specs: vec![],
            no_change_reason: None,
            replaces_decision_ids: vec![],
        });
        assert!(change.changed_after_start());
        // Replacing another decision after the start is a change too.
        change.as_started = Some(StartedWith {
            specs: change.specs.clone(),
            no_change_reason: None,
            replaces_decision_ids: vec![],
        });
        change.replaces_decision_ids = vec![Uuid::now_v7()];
        assert!(change.changed_after_start());
        // Only saved again in another order: the same.
        let other = SpecDelta {
            spec_id: Some(Uuid::now_v7()),
            ..change.specs[0].clone()
        };
        change.specs.push(other);
        change.replaces_decision_ids = vec![];
        change.as_started = Some(StartedWith {
            specs: change.specs.clone(),
            no_change_reason: None,
            replaces_decision_ids: vec![],
        });
        change.specs.reverse();
        assert!(!change.changed_after_start());
    }

    #[test]
    fn two_issues_changing_one_requirement_conflict_and_the_later_one_comes_second() {
        let (spec, other) = (Uuid::now_v7(), Uuid::now_v7());
        let max = open_change(spec, &adds("Maximum length"), false, 1);
        let special = open_change(spec, &adds("Special character"), false, 2);
        let min12 = open_change(spec, &modifies("Minimum length", "At least 12."), false, 4);
        let min8 = open_change(spec, &modifies("Minimum length", "At least 8."), false, 3);
        let elsewhere = open_change(other, &modifies("Minimum length", "x"), false, 0);
        // Adding different requirements doesn't conflict; changing the same one does.
        assert_eq!(
            spec_conflicts(&[
                max.clone(),
                special.clone(),
                min12.clone(),
                min8.clone(),
                elsewhere
            ]),
            [SpecConflict {
                spec_id: spec,
                requirement: Some("Minimum length".into()),
                first: min8.id,
                second: min12.id,
            }]
        );
        // A rewrite changes everything in its spec.
        let rewrite = open_change(spec, PASSWORDS, true, 5);
        let conflicts = spec_conflicts(&[max.clone(), rewrite.clone()]);
        assert_eq!(
            conflicts,
            [SpecConflict {
                spec_id: spec,
                requirement: None,
                first: max.id,
                second: rewrite.id,
            }]
        );
    }

    #[test]
    fn widening_an_old_part_later_never_jumps_ahead() {
        let spec = Uuid::now_v7();
        let at = |minute: i64| {
            DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap()
                + chrono::TimeDelta::minutes(minute)
        };
        // B's part added "Bursts" first; A then changed "Minimum length";
        // B's part then changed it too, keeping the time it was first written.
        let mut b = open_change(spec, &adds("Bursts"), false, 0);
        b.specs[0].text = format!(
            "{}\n{}",
            adds("Bursts"),
            modifies("Minimum length", "At least 12.")
        );
        b.specs[0].touched_at = [
            ("Bursts".to_string(), at(0)),
            ("Minimum length".to_string(), at(5)),
        ]
        .into_iter()
        .collect();
        let mut a = open_change(spec, &modifies("Minimum length", "At least 8."), false, 1);
        a.specs[0].touched_at = [("Minimum length".to_string(), at(1))]
            .into_iter()
            .collect();
        let conflicts = spec_conflicts(&[a.clone(), b.clone()]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!((conflicts[0].first, conflicts[0].second), (a.id, b.id));
    }

    fn pairs(conflicts: &[SpecConflict]) -> Vec<(Uuid, Uuid)> {
        conflicts.iter().map(|c| (c.first, c.second)).collect()
    }

    #[test]
    fn two_issues_holding_each_other_back_is_a_loop() {
        let (x, y, z) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let at = |minute: i64| {
            DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap()
                + chrono::TimeDelta::minutes(minute)
        };
        let rule = modifies("Minimum length", "At least 8.");
        let part = |spec: Uuid, minute: i64| SpecDelta {
            written_at: Some(at(minute)),
            ..delta(Some(spec), None, &rule, false)
        };
        // A first on X, B first on Y: A writing for Y as well is fine...
        let mut a = open_change(x, &rule, false, 1);
        let mut b = open_change(y, &rule, false, 2);
        a.specs.push(part(y, 3));
        let conflicts = spec_conflicts(&[a.clone(), b.clone()]);
        assert!(held_in_a_loop(&pairs(&conflicts), a.id).is_empty());
        // ...but B then writing for X would hold each back.
        b.specs.push(part(x, 4));
        let conflicts = spec_conflicts(&[a.clone(), b.clone()]);
        assert_eq!(held_in_a_loop(&pairs(&conflicts), b.id), [a.id]);
        // Through a third: A before B on X, B before C on Y, C before A on Z.
        let a = SpecChange {
            specs: vec![part(x, 0), part(z, 5)],
            ..open_change(x, &rule, false, 0)
        };
        let b = SpecChange {
            specs: vec![part(x, 1), part(y, 2)],
            ..open_change(x, &rule, false, 1)
        };
        let c = SpecChange {
            specs: vec![part(y, 3), part(z, 4)],
            ..open_change(y, &rule, false, 3)
        };
        let conflicts = spec_conflicts(&[a.clone(), b, c.clone()]);
        assert_eq!(held_in_a_loop(&pairs(&conflicts), a.id), [c.id]);
    }

    #[test]
    fn a_new_spec_of_the_same_name_holds_back_and_can_loop() {
        let x = Uuid::now_v7();
        let at = |minute: i64| {
            DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap()
                + chrono::TimeDelta::minutes(minute)
        };
        let quotas = |title: &str, minute: i64| SpecDelta {
            written_at: Some(at(minute)),
            ..delta(None, Some(title), "## Purpose\n\nQuotas.", true)
        };
        let rule = modifies("Minimum length", "At least 8.");
        // B wrote "Quotas" first; A (reopened) comes after it, whatever the case.
        let a = SpecChange {
            specs: vec![quotas(" quotas", 3)],
            ..open_change(x, &rule, false, 3)
        };
        let mut b = SpecChange {
            specs: vec![quotas("Quotas", 2)],
            ..open_change(x, &rule, false, 2)
        };
        let held = new_spec_holds(&[a.clone(), b.clone()]);
        assert_eq!(
            held,
            [NewSpecHold {
                title: "quotas".into(),
                first: b.id,
                second: a.id
            }]
        );
        // A goes first on a requirement B then also changes: a loop.
        let mut a = a;
        a.specs.push(SpecDelta {
            written_at: Some(at(4)),
            ..delta(Some(x), None, &rule, false)
        });
        b.specs.push(SpecDelta {
            written_at: Some(at(5)),
            ..delta(Some(x), None, &rule, false)
        });
        assert_eq!(
            held_in_a_loop(&holds(&[a.clone(), b.clone()]), b.id),
            [a.id]
        );
    }

    #[test]
    fn a_part_written_against_older_text_says_what_changed_since() {
        let spec = Some(Uuid::now_v7());
        let part = SpecDelta {
            based_on: Some(PASSWORDS.into()),
            ..delta(
                spec,
                None,
                &modifies("Minimum length", "At least 12."),
                false,
            )
        };
        assert_eq!(changed_since(&part, PASSWORDS), None);
        let now = PASSWORDS.replace("at least 3", "at least 8");
        assert_eq!(
            changed_since(&part, &now),
            Some(ChangedSince::Requirements(vec!["Minimum length".into()]))
        );
        // A requirement it adds that the spec has gained since does.
        let adding = SpecDelta {
            based_on: Some(PASSWORDS.into()),
            ..delta(spec, None, &adds("Maximum length"), false)
        };
        let gained = format!(
            "{PASSWORDS}\n{}",
            adds("Maximum length").replace("## ADDED Requirements\n\n", "")
        );
        assert_eq!(
            changed_since(&adding, &gained),
            Some(ChangedSince::Requirements(vec!["Maximum length".into()]))
        );
        assert_eq!(changed_since(&adding, PASSWORDS), None);
        // Another requirement added since doesn't matter to it.
        let more = format!(
            "{PASSWORDS}\n{}",
            adds("Maximum length").replace("## ADDED Requirements\n\n", "")
        );
        assert_eq!(changed_since(&part, &more), None);
        // A rewrite relies on the whole text; a part that doesn't say what it
        // was written against can't tell.
        let rewrite = SpecDelta {
            based_on: Some(PASSWORDS.into()),
            ..delta(spec, None, "## Requirements\n", true)
        };
        assert_eq!(changed_since(&rewrite, &now), Some(ChangedSince::Spec));
        assert_eq!(changed_since(&delta(spec, None, DELTA, false), &now), None);
    }

    #[test]
    fn a_spec_change_names_each_spec_once_and_new_ones_whole() {
        let spec = Some(Uuid::now_v7());
        let create = |specs: Vec<SpecDelta>, reason: Option<&str>| SpecChangeCreate {
            issue_id: Uuid::now_v7(),
            specs: Some(specs),
            replaces_decision_ids: None,
            no_change_reason: reason.map(Into::into),
        };
        assert!(
            create(vec![delta(spec, None, DELTA, false)], None)
                .validate()
                .is_ok()
        );
        assert!(
            create(
                vec![delta(None, Some("Limits"), "## Purpose\n", true)],
                None
            )
            .validate()
            .is_ok()
        );
        assert!(create(vec![], Some("Only a typo.")).validate().is_ok());

        let fails = |change: SpecChangeCreate| change.validate().unwrap_err().0;
        assert_eq!(
            fails(create(
                vec![delta(None, Some("Limits"), DELTA, false)],
                None
            )),
            "a new spec is written as its whole text"
        );
        assert_eq!(
            fails(create(
                vec![
                    delta(spec, None, DELTA, false),
                    delta(spec, None, "All.", true)
                ],
                None
            )),
            "a spec change names the same spec twice"
        );
        assert_eq!(
            fails(create(
                vec![delta(spec, None, DELTA, false)],
                Some("Nothing.")
            )),
            "a spec change can't change specs and say no spec changes"
        );
        assert!(
            fails(create(vec![delta(spec, None, "## Purpose\n", false)], None))
                .contains("isn't a delta section")
        );
        assert_eq!(
            fails(create(vec![delta(None, None, "All.", true)], None)),
            "say which spec changes, or name a new one"
        );
    }
}
