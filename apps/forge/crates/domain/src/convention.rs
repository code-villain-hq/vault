//! Whether a behaviour spec follows OpenSpec's convention (decision "Behaviour
//! specs follow OpenSpec's spec convention"): the same structural checks as
//! `openspec validate`. Problems are warnings; people decide, so the spec is
//! saved anyway. The web app checks the same way (`conventionWarnings`).

use crate::verdict::{Requirement, requirements};

/// What keeps the spec from following the convention, in plain sentences.
/// Only specs with a `## Requirements` section are behaviour specs; prose
/// gets none.
pub fn convention_warnings(body: &str) -> Vec<String> {
    let mut fenced = false;
    let mut behaviour = false;
    let mut purpose: Option<String> = None;
    let mut in_purpose = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let section = !fenced && is_heading(line, 2);
        if section {
            let title = line.trim_start_matches('#').trim();
            in_purpose = title == "Purpose";
            if in_purpose {
                purpose.get_or_insert_with(String::new);
            }
            behaviour |= title == "Requirements";
        } else if in_purpose && let Some(text) = purpose.as_mut() {
            text.push_str(line);
        }
    }
    if !behaviour {
        return Vec::new();
    }

    let mut warnings = Vec::new();
    match purpose {
        None => warnings.push("It has no \"## Purpose\" section.".to_string()),
        Some(text) if text.trim().is_empty() => {
            warnings.push("Its \"## Purpose\" section is empty.".to_string())
        }
        Some(_) => {}
    }
    let found = requirements(body);
    if found.is_empty() {
        warnings.push("It has no requirements: add one as \"### Requirement: <name>\".".into());
    }
    for (i, requirement) in found.iter().enumerate() {
        let name = &requirement.name;
        warnings.extend(requirement_warnings(requirement));
        let repeats = found[..i].iter().filter(|r| &r.name == name).count();
        if repeats == 1 {
            warnings.push(format!(
                "Two requirements are named \"{name}\"; each needs its own name."
            ));
        }
    }
    warnings
}

/// What keeps one requirement from following the convention: SHALL or MUST
/// in its statement, and at least one scenario.
pub fn requirement_warnings(requirement: &Requirement) -> Vec<String> {
    let name = &requirement.name;
    let (statement, scenarios) = parts(&requirement.text);
    let mut warnings = Vec::new();
    if !says_shall_or_must(&statement) {
        warnings.push(format!("Requirement \"{name}\" doesn't say SHALL or MUST."));
    }
    if scenarios == 0 {
        warnings.push(format!(
            "Requirement \"{name}\" has no scenario: add one as \"#### Scenario: <name>\"."
        ));
    }
    warnings
}

/// A heading of at most `level`, e.g. `## Purpose` for 2.
fn is_heading(line: &str, level: usize) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=level).contains(&hashes) && line[hashes..].starts_with(' ')
}

/// A requirement's statement (what comes before its first `####` heading)
/// and how many `#### Scenario:` it has.
fn parts(text: &str) -> (String, usize) {
    let mut statement = String::new();
    let mut scenarios = 0;
    let mut fenced = false;
    let mut in_statement = true;
    // The first line is the requirement's heading.
    for line in text.lines().skip(1) {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with("#### ") {
            in_statement = false;
            scenarios += usize::from(line.starts_with("#### Scenario:"));
        } else if in_statement {
            statement.push_str(line);
            statement.push('\n');
        }
    }
    (statement, scenarios)
}

fn says_shall_or_must(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "SHALL" || word == "MUST")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "\
## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429
";

    #[test]
    fn a_spec_that_follows_the_convention_has_no_warnings() {
        assert!(convention_warnings(GOOD).is_empty());
    }

    #[test]
    fn prose_is_left_alone() {
        assert!(convention_warnings("# Overview\n\nJust prose, no purpose.\n").is_empty());
        assert!(convention_warnings("Text.\n```md\n## Requirements\n```\n").is_empty());
    }

    #[test]
    fn names_each_problem() {
        let body = "\
## Requirements

### Requirement: Tokens are limited
The API should allow 100 requests a minute.

#### Scenario: Too many
- **WHEN** a token sends a 101st request
- **THEN** it gets 429

### Requirement: Limits reset
The limit MUST reset every minute.

#### Not a scenario

### Requirement: Tokens are limited
The API SHALL limit tokens.

#### Scenario: Again
- **THEN** it's limited
";
        assert_eq!(
            convention_warnings(body),
            [
                "It has no \"## Purpose\" section.",
                "Requirement \"Tokens are limited\" doesn't say SHALL or MUST.",
                "Requirement \"Limits reset\" has no scenario: add one as \"#### Scenario: <name>\".",
                "Two requirements are named \"Tokens are limited\"; each needs its own name.",
            ]
        );
    }

    #[test]
    fn an_empty_purpose_and_no_requirements() {
        assert_eq!(
            convention_warnings("## Purpose\n\n## Requirements\n\nNone yet.\n"),
            [
                "Its \"## Purpose\" section is empty.",
                "It has no requirements: add one as \"### Requirement: <name>\".",
            ]
        );
    }

    #[test]
    fn shall_counts_only_as_a_word_in_the_statement() {
        let body = GOOD.replace("SHALL allow", "allows (MARSHALL)");
        assert_eq!(
            convention_warnings(&body),
            ["Requirement \"Tokens are limited\" doesn't say SHALL or MUST."]
        );
        let in_scenario = body.replace("it gets 429", "it MUST get 429");
        assert_eq!(convention_warnings(&in_scenario).len(), 1);
    }
}
