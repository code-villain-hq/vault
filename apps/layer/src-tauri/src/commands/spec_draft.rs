//! Drafting a spec's files and checking the work against it (§19): each a
//! fresh one-shot run, never the agent doing the work.
//!
//! Every file is drafted in one run for the whole task, however many
//! repositories it touches (SPEC-5): the requirements, so the split of the
//! work is decided in one place; the design, reading every repository's
//! code, so both sides of an endpoint are described once; and every
//! repository's tasks, so what comes first across them is decided in one
//! place. A run per repository wrote each side of a contract in its own
//! words, and nothing compared them.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::Task;
use crate::error::{Error, Result};
use crate::oneshot::{oneshot, Oneshot};
use crate::shellenv;
use crate::spec::{self, Kind, Part};

use super::jira::{review_context, reviewed_repos, task_repos};
use super::panes::agent_file_dir;
use super::spec::{spec_of, Check, Slot, Spec, Verdict};
use super::spec_prompts::{answers_prompt, check_prompt, design_prompt, merge_prompt, requirements_prompt, tasks_prompt};
use super::spec_view::CheckView;
use super::task_context::TICKET_FILE;

#[derive(Clone, Serialize)]
struct SpecDraftChunk<'a> {
    request_id: &'a str,
    part: Part,
    /// Whose tasks; None for the task's requirements and design.
    checkout_id: Option<&'a str>,
    /// The whole of this file's draft so far.
    text: &'a str,
}

/// Sonnet without thinking: what a ticket means is a judgement Haiku makes
/// thinly, and thinking would leave the user watching an empty editor.
fn how(read: bool) -> Oneshot<'static> {
    Oneshot { model: "sonnet", read }
}

fn claude() -> Result<String> {
    shellenv::which("claude").ok_or_else(|| Error::NotFound("claude is not on your PATH".into()))
}

/// Where a run that reads code stands: the task folder, which holds every
/// repository's worktree, or the one worktree of a task without one.
fn reading_dir(state: &super::AppState, task: &Task, spec: &Spec) -> PathBuf {
    agent_file_dir(state, task)
        .or_else(|| spec.homes.first().map(|h| PathBuf::from(&h.checkout.path)))
        .unwrap_or_else(std::env::temp_dir)
}

/// Draft one file of the spec (SPEC-5), streamed as `spec:draft` into its
/// editor and kept as its draft once done: the requirements or the design
/// of the task, or every repository's tasks. Nothing is approved.
#[tauri::command]
pub async fn draft_spec(app: AppHandle, task_id: String, part: Part, kind: Option<Kind>, request_id: String) -> Result<()> {
    let program = claude()?;
    let emitter = app.clone();
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        if spec.homes.is_empty() {
            return Err(Error::Other("this task has no repositories to write a spec for".into()));
        }
        let say = |part: Part, checkout: Option<&str>, text: &str| {
            let _ = emitter.emit("spec:draft", SpecDraftChunk { request_id: &request_id, part, checkout_id: checkout, text });
        };
        let folders: Vec<String> = spec.homes.iter().map(|h| h.folder.clone()).collect();
        let reqs_slot = Slot::shared(Part::Requirements);
        match part {
            Part::Requirements => {
                let kind = kind.unwrap_or_else(|| spec.kind());
                spec.set_kind(kind)?;
                let ticket = agent_file_dir(state, &task).and_then(|d| std::fs::read_to_string(d.join(TICKET_FILE)).ok());
                // What the user answered on the draft before is decided:
                // a redraft from the ticket alone asked it all again and
                // threw the answers away (SPEC-5).
                let decided = spec.latest(&reqs_slot).map(|t| spec::answered(&t)).unwrap_or_default();
                let prompt = requirements_prompt(&task, kind, &folders, &task_repos(state, &task), ticket.as_deref(), &decided);
                let mut so_far = String::new();
                let out = oneshot(&program, home_dir(), how(false), &prompt, |delta| {
                    so_far.push_str(delta);
                    say(part, None, &so_far);
                })?;
                if out.trim().is_empty() {
                    return Err(Error::Other("claude returned no requirements".into()));
                }
                spec.save_draft(&reqs_slot, Some(&out))?;
            }
            Part::Design => {
                let reqs = spec.latest(&reqs_slot).ok_or_else(|| Error::Other("approve the requirements first".into()))?;
                let old = spec.approved(&Slot::shared(Part::Design));
                let prompt = design_prompt(spec.kind(), &folders, &reqs, old.as_deref());
                let mut so_far = String::new();
                let out = oneshot(&program, reading_dir(state, &task, &spec), how(true), &prompt, |delta| {
                    so_far.push_str(delta);
                    say(part, None, &so_far);
                })?;
                if out.trim().is_empty() {
                    return Err(Error::Other("claude returned an empty design".into()));
                }
                spec.save_draft(&Slot::shared(Part::Design), Some(&out))?;
            }
            Part::Tasks => {
                let reqs = spec.latest(&reqs_slot).ok_or_else(|| Error::Other("approve the requirements first".into()))?;
                let design = spec.latest(&Slot::shared(Part::Design)).unwrap_or_else(|| "None.".into());
                let old: Vec<(String, String)> = spec
                    .homes
                    .iter()
                    .filter_map(|h| Some((h.folder.clone(), spec.approved(&Slot::tasks(&h.folder))?)))
                    .collect();
                let prompt = tasks_prompt(&folders, &reqs, &design, &old);
                let id_of = |folder: &str| spec.home(folder).map(|h| h.checkout.id.clone());
                let mut so_far = String::new();
                let out = oneshot(&program, home_dir(), how(false), &prompt, |delta| {
                    so_far.push_str(delta);
                    for (folder, text) in by_repo(&so_far, &folders) {
                        say(part, id_of(&folder).as_deref(), &text);
                    }
                })?;
                let split = by_repo(&out, &folders);
                if split.iter().all(|(_, t)| spec::steps(t).is_empty()) {
                    return Err(Error::Other("claude returned no steps".into()));
                }
                let requirements = spec::requirements(&reqs);
                for (folder, text) in split {
                    let slot = Slot::tasks(&folder);
                    // Done stays done, and a step left without its
                    // requirements is marked, not dropped (SPEC-7).
                    let text = match spec.approved(&slot) {
                        Some(old) => {
                            let mine: Vec<_> = requirements.iter().filter(|r| r.names(&folder)).cloned().collect();
                            spec::sync_steps(&old, &text, &mine)
                        }
                        None => text,
                    };
                    say(part, id_of(&folder).as_deref(), &text);
                    spec.save_draft(&slot, Some(&text))?;
                }
            }
        }
        Ok(())
    })
    .await
}

fn home_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_else(std::env::temp_dir)
}

/// What Apply answers did to the requirements (SPEC-20), by id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppliedAnswers {
    pub changed: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Build the answered open questions into the requirements (SPEC-20): one
/// run over them as the editor holds them (the tab saves them first).
/// Streamed as `spec:draft` and kept as the draft; nothing is approved.
#[tauri::command]
pub async fn apply_spec_answers(app: AppHandle, task_id: String, request_id: String) -> Result<AppliedAnswers> {
    let program = claude()?;
    let emitter = app.clone();
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let slot = Slot::shared(Part::Requirements);
        let before = spec.latest(&slot).unwrap_or_default();
        if spec::answered(&before).is_empty() {
            return Err(Error::Other("no open question has an answer yet".into()));
        }
        let say = |text: &str| {
            let _ = emitter.emit("spec:draft", SpecDraftChunk { request_id: &request_id, part: Part::Requirements, checkout_id: None, text });
        };
        let folders: Vec<String> = spec.homes.iter().map(|h| h.folder.clone()).collect();
        let mut so_far = String::new();
        let out = oneshot(&program, home_dir(), how(false), &answers_prompt(&folders, &before), |delta| {
            so_far.push_str(delta);
            say(&so_far);
        })?;
        if out.trim().is_empty() {
            say(&before);
            return Err(Error::Other("claude returned nothing".into()));
        }
        say(&out);
        spec.save_draft(&slot, Some(&out))?;
        let (changed, added, removed) = compare(&before, &out);
        Ok(AppliedAnswers { changed, added, removed })
    })
    .await
}

/// Check one repository's work against the requirements that name it
/// (SPEC-15): a run that may read the worktree, over the design's contracts
/// and the branch's changes from its branch point. Kept with the commit it
/// was checked at.
#[tauri::command]
pub async fn check_spec(app: AppHandle, task_id: String, checkout_id: String) -> Result<CheckView> {
    let program = claude()?;
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let home = spec
            .homes
            .iter()
            .find(|h| h.checkout.id == checkout_id)
            .ok_or_else(|| Error::NotFound(format!("checkout {checkout_id}")))?;
        let requirements = spec
            .approved(&Slot::shared(Part::Requirements))
            .ok_or_else(|| Error::Other("there are no approved requirements to check against".into()))?;
        let ids: Vec<String> = spec::requirements(&requirements).into_iter().filter(|r| r.names(&home.folder)).map(|r| r.id).collect();
        if ids.is_empty() {
            return Err(Error::Other(format!("no requirement names {}", home.folder)));
        }
        let design = spec.approved(&Slot::shared(Part::Design));
        let repo: Vec<_> = reviewed_repos(state, &task).into_iter().filter(|r| r.checkout_id == checkout_id).collect();
        let sha = crate::git::head_commit(Path::new(&home.checkout.path))?;
        let prompt = check_prompt(&home.folder, &ids, &requirements, design.as_deref(), &review_context(&repo));
        let out = oneshot(&program, Path::new(&home.checkout.path), how(true), &prompt, |_| {})?;
        let results = verdicts(&out, &ids);
        if results.is_empty() {
            return Err(Error::Other("the check gave no answer for any requirement".into()));
        }
        let check = Check { sha, at: chrono::Utc::now().timestamp_millis(), results };
        spec.set_check(home, check.clone())?;
        Ok(CheckView { check, stale: false })
    })
    .await
}

/// Merge the specs each repository had of its own into one (SPEC-25): one
/// run over every copy, which writes the task's requirements and design and
/// each repository's tasks, streamed and kept as drafts. Nothing in a
/// worktree changes until they are approved.
#[tauri::command]
pub async fn merge_specs(app: AppHandle, task_id: String, request_id: String) -> Result<()> {
    let program = claude()?;
    let emitter = app.clone();
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let kind = spec.kind();
        let folders: Vec<String> = spec.homes.iter().map(|h| h.folder.clone()).collect();
        let copies: Vec<(String, Vec<(Part, String)>)> = spec
            .homes
            .iter()
            .map(|h| (h.folder.clone(), Part::ALL.iter().filter_map(|&p| Some((p, h.copy(p, kind)?))).collect()))
            .collect();
        let prompt = merge_prompt(kind, &folders, &copies);
        let mut names = vec![REQUIREMENTS_HEADING.to_string(), DESIGN_HEADING.to_string()];
        names.extend(folders.iter().cloned());
        let id_of = |name: &str| spec.home(name).map(|h| h.checkout.id.clone());
        let part_of = |name: &str| match name {
            REQUIREMENTS_HEADING => Part::Requirements,
            DESIGN_HEADING => Part::Design,
            _ => Part::Tasks,
        };
        let say = |name: &str, text: &str| {
            let checkout = id_of(name);
            let _ = emitter.emit("spec:draft", SpecDraftChunk { request_id: &request_id, part: part_of(name), checkout_id: checkout.as_deref(), text });
        };
        let mut so_far = String::new();
        let out = oneshot(&program, home_dir(), how(false), &prompt, |delta| {
            so_far.push_str(delta);
            for (name, text) in by_repo(&so_far, &names) {
                say(&name, &text);
            }
        })?;
        let split = by_repo(&out, &names);
        if !split.iter().any(|(n, t)| n == REQUIREMENTS_HEADING && !spec::requirements(t).is_empty()) {
            return Err(Error::Other("the merge gave no requirements".into()));
        }
        for (name, text) in split {
            let slot = match part_of(&name) {
                Part::Tasks => Slot::tasks(&name),
                part => Slot::shared(part),
            };
            spec.save_draft(&slot, Some(&text))?;
        }
        Ok(())
    })
    .await
}

/// The merge's headings for the task's own files, beside the repositories'.
const REQUIREMENTS_HEADING: &str = "requirements";
const DESIGN_HEADING: &str = "design";

/// A run's text, one part per name: a first-level heading that is a name
/// alone starts its part. With one name, no heading is needed.
fn by_repo(text: &str, names: &[String]) -> Vec<(String, String)> {
    let named = |line: &str| {
        let name = line.strip_prefix("# ")?.trim().trim_matches('`').trim_end_matches('/');
        names.iter().find(|f| f.eq_ignore_ascii_case(name)).cloned()
    };
    let mut parts: Vec<(String, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        if let Some(folder) = named(line.trim_end()) {
            parts.push((folder, String::new()));
        } else if let Some((_, part)) = parts.last_mut() {
            part.push_str(line);
        } else if names.len() == 1 {
            parts.push((names[0].clone(), line.to_string()));
        }
    }
    parts.into_iter().map(|(f, t)| (f, t.trim_start().to_string())).collect()
}

/// The requirements changed, added and removed between two texts, by id.
fn compare(before: &str, after: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let old = spec::requirements(before);
    let new = spec::requirements(after);
    let normal = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let changed = new
        .iter()
        .filter(|n| old.iter().any(|o| o.id == n.id && (normal(&o.text) != normal(&n.text) || o.repos != n.repos)))
        .map(|n| n.id.clone())
        .collect();
    let added = new.iter().filter(|n| !old.iter().any(|o| o.id == n.id)).map(|n| n.id.clone()).collect();
    let removed = old.iter().filter(|o| !new.iter().any(|n| n.id == o.id)).map(|o| o.id.clone()).collect();
    (changed, added, removed)
}

/// The check's lines, one per requirement it knows: `R-1 | met | why`.
fn verdicts(out: &str, ids: &[String]) -> Vec<Verdict> {
    let mut found: Vec<Verdict> = Vec::new();
    for line in out.lines() {
        let mut cells = line.trim().trim_matches('|').splitn(3, '|').map(str::trim);
        let (Some(id), Some(verdict)) = (cells.next(), cells.next()) else { continue };
        let id = id.trim_matches('*').to_ascii_uppercase();
        let verdict = verdict.trim_matches('*').trim_matches('`').to_ascii_lowercase();
        if !ids.contains(&id) || found.iter().any(|v| v.id == id) {
            continue;
        }
        let verdict = match verdict.as_str() {
            "met" => "met",
            "not met" | "unmet" => "not met",
            _ => "unclear",
        };
        found.push(Verdict { id, verdict: verdict.into(), evidence: cells.next().unwrap_or("").to_string() });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task {
            id: "t1".into(),
            name: "ACME-12 Refunds\nfail for split payments".into(),
            root: "/work/acme-12".into(),
            branch: "ACME-12".into(),
            issue_key: Some("ACME-12".into()),
            issue_url: None,
            created_at: chrono::Utc::now(),
            ticket_stage: None,
            chat: None,
            review: None,
            browser: None,
            browser_url: None,
        }
    }

    fn two() -> Vec<String> {
        vec!["api".to_string(), "web".to_string()]
    }

    #[test]
    fn a_run_for_two_repositories_is_split_at_their_headings() {
        let text = "# api\n## Tasks\n- [ ] 1. Add it\n\n# `web`\n## Tasks\n- [ ] 1. Show it\n";
        let parts = by_repo(text, &two());
        assert_eq!(parts, vec![("api".into(), "## Tasks\n- [ ] 1. Add it\n\n".into()), ("web".into(), "## Tasks\n- [ ] 1. Show it\n".into())]);
        // One repository needs no heading, and is not cut without one.
        assert_eq!(by_repo("## Tasks\nx\n", &two()[..1]), vec![("api".into(), "## Tasks\nx\n".into())]);
        // Half way through, what has come so far.
        assert_eq!(by_repo("# api\n## Ta", &two()), vec![("api".into(), "## Ta".into())]);
    }

    #[test]
    fn a_merge_is_split_into_the_tasks_own_files_and_each_repositorys_tasks() {
        let names = vec![REQUIREMENTS_HEADING.to_string(), DESIGN_HEADING.to_string(), "api".into(), "web".into()];
        let out = "# requirements\n## Requirements\n- R-1 [api]: x\n# design\n## Approach\ny\n# api\n## Tasks\n- [x] 1. Done (R-1)\n";
        let parts: Vec<String> = by_repo(out, &names).into_iter().map(|(n, _)| n).collect();
        assert_eq!(parts, vec!["requirements", "design", "api"]);
    }

    #[test]
    fn applying_answers_names_what_changed_by_id() {
        let before = "## Requirements\n- R-1: WHEN a sixth reset THE SYSTEM SHALL refuse it.\n- R-2 [api]: WHEN two addresses THE SYSTEM SHALL count apart.\n- R-3: WHEN staff ask THE SYSTEM SHALL lift it.\n";
        let after = "## Requirements\n- R-1: WHEN a sixth reset THE SYSTEM SHALL refuse it, saying when to try again.\n- R-2 [api, web]: WHEN  two addresses THE SYSTEM SHALL count apart.\n- R-4: WHEN a reset is refused THE SYSTEM SHALL log it.\n";
        let (changed, added, removed) = compare(before, after);
        assert_eq!(changed, vec!["R-1", "R-2"], "spacing alone is no change; another repository is");
        assert_eq!(added, vec!["R-4"]);
        assert_eq!(removed, vec!["R-3"]);
    }

    /// Whether the model names repositories and builds answers in. Run with
    /// `cargo test --lib real_ -- --ignored`.
    #[test]
    #[ignore = "runs claude for real"]
    fn real_requirements_for_two_repositories_name_the_repositories_that_meet_them() {
        let program = shellenv::which("claude").expect("claude on the PATH");
        let ticket = "## The ticket: ACME-12 Rate-limit sign-in\n\n> After five failed sign-ins in a minute, \
                      the API refuses more for that address, and the sign-in page says when to try again.";
        let prompt = requirements_prompt(&task(), Kind::Feature, &two(), &[], Some(ticket), &[]);
        let out = oneshot(&program, std::env::temp_dir(), how(false), &prompt, |_| {}).unwrap();
        let reqs = spec::requirements(&out);
        assert!(!reqs.is_empty(), "{out}");
        assert!(reqs.iter().any(|r| r.names("api") && !r.repos.is_empty()), "{out}");
        assert!(reqs.iter().any(|r| r.names("web") && !r.repos.is_empty()), "{out}");
        eprintln!("{out}");
    }

    #[test]
    #[ignore = "runs claude for real"]
    fn real_answers_are_built_in_and_only_answered_questions_go() {
        let program = shellenv::which("claude").expect("claude on the PATH");
        let before = "## Goal\nPassword resets are rate-limited.\n\n## Requirements\n\
                      - R-1: WHEN a sixth reset is asked for one address within an hour THE SYSTEM SHALL refuse it.\n\
                      - R-2: WHEN resets are asked for different addresses THE SYSTEM SHALL count them separately.\n\n\
                      ## Out of scope\n- Rate-limiting sign-ins.\n\n## Open questions\n\
                      - Q-1: Should the refusal say when to try again?\n  - Yes, the minutes left\n  - No\n  Answer: Yes, the minutes left\n\
                      - Q-2: Should support staff be able to lift the limit?\n  - Yes\n  - No\n-- no, not in this work\n\
                      - Q-3: Is the hour a sliding window or a clock hour?\n  - Sliding\n  - Clock hour\n";
        let out = oneshot(&program, std::env::temp_dir(), how(false), &answers_prompt(&two()[..1], before), |_| {}).unwrap();
        let (changed, added, removed) = compare(before, &out);
        assert!(changed.contains(&"R-1".to_string()) || !added.is_empty(), "the refusal's wording is in no requirement:\n{out}");
        assert!(!changed.contains(&"R-2".to_string()) && removed.is_empty(), "an untouched requirement changed:\n{out}");
        assert!(spec::answered(&out).is_empty(), "an answered question is still there:\n{out}");
        assert!(out.contains("sliding window or a clock hour"), "the unanswered question went:\n{out}");
        assert!(!out.contains("Should support staff"), "{out}");
        eprintln!("{out}");
    }

    #[test]
    fn a_checks_answers_are_read_for_known_requirements_only() {
        let out = "Here you go:\nR-1 | met | api/src/limit.ts:12\n| R-2 | **Not met** | no 429 |\nR-9 | met | ?\nR-3 | maybe | can't tell\n";
        let v = verdicts(out, &["R-1".into(), "R-2".into(), "R-3".into()]);
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].id.as_str(), v[0].verdict.as_str(), v[0].evidence.as_str()), ("R-1", "met", "api/src/limit.ts:12"));
        assert_eq!(v[1].verdict, "not met");
        assert_eq!(v[2].verdict, "unclear");
    }
}
