//! Per-requirement verdicts. A behaviour spec follows OpenSpec's convention:
//! each `### Requirement: <name>` is a unit a drift check can judge, and its
//! name is its identity. A check records one verdict per requirement, and
//! the spec page shows the latest.

use std::ops::Range;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules;

/// One `### Requirement:` of a spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub name: String,
    /// The requirement as written: its heading, sentence and scenarios.
    pub text: String,
}

pub(crate) const HEADING: &str = "### Requirement:";

/// The requirements of a spec written in OpenSpec's convention, in order.
/// Each runs from its heading to the next heading of the same or a higher
/// level; headings inside code fences don't count.
pub fn requirements(body: &str) -> Vec<Requirement> {
    let lines: Vec<&str> = body.lines().collect();
    requirement_lines(&lines)
        .into_iter()
        .map(|(name, range)| Requirement {
            name,
            text: lines[range].join("\n").trim_end().to_string(),
        })
        .collect()
}

/// Where each requirement sits among a spec's lines: its name, and its lines
/// from its heading to its last line that isn't blank.
pub(crate) fn requirement_lines<S: AsRef<str>>(lines: &[S]) -> Vec<(String, Range<usize>)> {
    let mut found: Vec<(String, Range<usize>)> = Vec::new();
    let mut current: Option<(String, Range<usize>)> = None;
    let mut fenced = false;
    for (index, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let heading = !fenced && ends_requirement(line);
        if heading && let Some(done) = current.take() {
            found.push(done);
        }
        if heading && let Some(name) = line.strip_prefix(HEADING) {
            let name = name.trim();
            if !name.is_empty() {
                current = Some((name.to_string(), index..index + 1));
            }
        } else if let Some((_, range)) = current.as_mut()
            && !line.trim().is_empty()
        {
            range.end = index + 1;
        }
    }
    found.extend(current);
    found
}

/// A heading of level 1 to 3, which ends the requirement before it.
fn ends_requirement(line: &str) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=3).contains(&hashes) && line[hashes..].starts_with(' ')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Verdict {
    Met,
    NotMet,
    /// The check couldn't tell, e.g. the code is outside the service's paths.
    Unclear,
}

/// The latest verdict on one requirement of a spec, from a drift check.
/// Written by the server only. It goes stale when the requirement's text
/// changes, or when the service's code changes after it was checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequirementVerdict {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub spec_id: Uuid,
    /// The requirement's name: its identity.
    pub requirement: String,
    pub verdict: Verdict,
    /// Markdown: the evidence (files and lines), or why it's unclear.
    pub evidence: String,
    /// The requirement as it read when checked.
    pub requirement_text: String,
    /// The spec's version that was checked: its `updated_at` then.
    pub spec_updated_at: DateTime<Utc>,
    /// The latest commit touching the service's code when checked.
    pub commit_sha: Option<String>,
    pub run_id: Option<Uuid>,
    pub checked_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementVerdictCreate {
    pub spec_id: Uuid,
    pub requirement: String,
    pub verdict: Verdict,
    pub evidence: String,
    pub requirement_text: String,
    pub spec_updated_at: DateTime<Utc>,
    pub commit_sha: Option<String>,
    pub run_id: Option<Uuid>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementVerdictPatch {
    pub verdict: Option<Verdict>,
    pub evidence: Option<String>,
    pub requirement_text: Option<String>,
    pub spec_updated_at: Option<DateTime<Utc>>,
    pub commit_sha: Option<Option<String>>,
    /// Cleared when the run is deleted.
    pub run_id: Option<Option<Uuid>>,
    pub checked_at: Option<DateTime<Utc>>,
}

impl Validate for RequirementVerdictCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.requirement, "requirement", rules::TITLE)?;
        rules::long_text(&self.evidence, "evidence", rules::DESCRIPTION)
    }
}

impl Validate for RequirementVerdictPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        match &self.evidence {
            Some(evidence) => rules::long_text(evidence, "evidence", rules::DESCRIPTION),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429

```md
### Requirement: Not one, it's in a code block
```

### Requirement:   Limits reset
The limit MUST reset every minute.

## Notes

Not part of a requirement.
";

    #[test]
    fn finds_each_requirement_with_its_scenarios() {
        let found = requirements(SPEC);
        let names: Vec<&str> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Tokens are limited", "Limits reset"]);
        assert!(
            found[0]
                .text
                .starts_with("### Requirement: Tokens are limited")
        );
        assert!(found[0].text.contains("#### Scenario: Too many"));
        assert!(found[0].text.contains("Not one, it's in a code block"));
        assert!(found[1].text.ends_with("reset every minute."));
    }

    #[test]
    fn prose_has_no_requirements() {
        assert!(requirements("# Overview\n\nJust prose.\n### Not a requirement\n").is_empty());
        assert!(requirements("### Requirement:\nNameless.").is_empty());
    }
}
