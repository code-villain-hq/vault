//! An epic's sub-tasks (§26): each repository's steps grouped under a
//! second-level heading, the same heading across repositories being one
//! sub-task (EPIC-4), and which wait for which (EPIC-5).

use serde::Serialize;

use super::{checkbox, list_item, steps, Step};

/// One sub-task of an epic: its title, the ticket its heading names, its
/// share of each repository's steps, and the sub-tasks it waits for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubTask {
    pub title: String,
    /// `ACME-104` from `## ACME-104 Session refresh`: an existing ticket.
    pub key: Option<String>,
    pub repos: Vec<SubTaskRepo>,
    /// The titles of the sub-tasks it waits for.
    pub waits_for: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubTaskRepo {
    /// The repository's folder in the task: `api`.
    pub folder: String,
    pub steps: Vec<Step>,
    /// Its `tasks.md`: these steps alone, with an `after` on another
    /// sub-task's step left out.
    #[serde(skip)]
    pub tasks: String,
}

impl SubTask {
    /// The requirements its steps serve, in the order they first appear.
    pub fn serves(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for id in self.repos.iter().flat_map(|r| r.steps.iter()).flat_map(|s| s.serves.iter()) {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }
}

/// One heading's lines in one repository's `tasks.md`. `None` for steps
/// under no heading, or under `## Tasks`.
struct Group<'a> {
    heading: Option<&'a str>,
    lines: Vec<&'a str>,
}

fn groups(tasks: &str) -> Vec<Group<'_>> {
    let mut found: Vec<Group> = vec![Group { heading: None, lines: Vec::new() }];
    let mut fence = false;
    for line in tasks.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        let heading = (!fence).then(|| line.strip_prefix("## ")).flatten().map(str::trim);
        match heading {
            Some(h) if h.eq_ignore_ascii_case("tasks") => found.push(Group { heading: None, lines: Vec::new() }),
            Some(h) => found.push(Group { heading: Some(h), lines: Vec::new() }),
            None => {
                if let Some(last) = found.last_mut() {
                    last.lines.push(line);
                }
            }
        }
    }
    found.retain(|g| g.lines.iter().any(|l| list_item(l.trim_start()).and_then(checkbox).is_some()));
    found
}

/// `ACME-104 Session refresh` as (Some("ACME-104"), "Session refresh").
fn key_of(heading: &str) -> (Option<String>, String) {
    let (first, rest) = heading.split_once(' ').unwrap_or((heading, ""));
    let looks_like_key = first
        .split_once('-')
        .is_some_and(|(p, n)| {
            p.len() >= 2
                && p.starts_with(|c: char| c.is_ascii_uppercase())
                && p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !n.is_empty()
                && n.chars().all(|c| c.is_ascii_digit())
        });
    if looks_like_key && !rest.trim().is_empty() {
        (Some(first.to_string()), rest.trim().trim_start_matches([':', '-', '—', '–']).trim().to_string())
    } else {
        (None, heading.to_string())
    }
}

/// `api 4` as ("api", 4); `4` alone is in `own`.
fn step_ref(after: &str, own: &str) -> Option<(String, u32)> {
    let after = after.trim();
    match after.rsplit_once(' ') {
        Some((repo, n)) => Some((repo.trim().to_string(), n.trim().trim_end_matches('.').parse().ok()?)),
        None => Some((own.to_string(), after.parse().ok()?)),
    }
}

/// The sub-tasks of an epic's tasks, one `tasks.md` per repository folder,
/// in the order they can start. `epic` names a repository's steps under no
/// heading: "<epic> in <repo>".
pub fn sub_tasks(epic: &str, files: &[(String, String)]) -> Vec<SubTask> {
    // Each group, by title, with its lines per repository.
    struct Found<'a> {
        title: String,
        key: Option<String>,
        repos: Vec<(String, Vec<&'a str>)>,
    }
    let mut found: Vec<Found> = Vec::new();
    for (folder, text) in files {
        for g in groups(text) {
            let (key, title) = match g.heading {
                Some(h) => key_of(h),
                None => (None, format!("{epic} in {folder}")),
            };
            let at = found.iter().position(|f| f.title.eq_ignore_ascii_case(&title) && (f.key.is_none() || key.is_none() || f.key == key));
            let at = match at {
                Some(i) => i,
                None => {
                    found.push(Found { title, key: None, repos: Vec::new() });
                    found.len() - 1
                }
            };
            let f = &mut found[at];
            f.key = f.key.take().or(key);
            match f.repos.iter_mut().find(|(r, _)| r == folder) {
                Some((_, lines)) => lines.extend(g.lines),
                None => f.repos.push((folder.clone(), g.lines)),
            }
        }
    }

    // Which sub-task each step is in, to tell an `after` inside one from one
    // that waits for another.
    let owner = |repo: &str, number: u32| -> Option<usize> {
        found.iter().position(|f| {
            f.repos.iter().any(|(r, lines)| r.eq_ignore_ascii_case(repo) && steps(&lines.join("\n")).iter().any(|s| s.number == number))
        })
    };
    let mut out: Vec<(SubTask, Vec<usize>)> = Vec::new();
    for (i, f) in found.iter().enumerate() {
        let mut waits: Vec<usize> = Vec::new();
        let mut repos = Vec::new();
        for (folder, lines) in &f.repos {
            let mut kept: Vec<String> = Vec::new();
            for line in lines {
                let step = list_item(line.trim_start()).and_then(checkbox).is_some() && line.len() - line.trim_start().len() < 2;
                if !step {
                    kept.push(line.to_string());
                    continue;
                }
                let (line, others) = keep_own_afters(line, i, |after| step_ref(after, folder).and_then(|(r, n)| owner(&r, n)));
                for o in others {
                    if !waits.contains(&o) {
                        waits.push(o);
                    }
                }
                kept.push(line);
            }
            let body = kept.join("\n");
            let tasks = format!("## Tasks\n\n{}\n", body.trim());
            repos.push(SubTaskRepo { folder: folder.clone(), steps: steps(&tasks), tasks });
        }
        out.push((SubTask { title: f.title.clone(), key: f.key.clone(), repos, waits_for: Vec::new() }, waits));
    }

    // In the order they can start, each after what it waits for, otherwise
    // as they were written. A loop is left in the order written.
    let mut placed: Vec<usize> = Vec::new();
    while placed.len() < out.len() {
        let next = (0..out.len())
            .find(|i| !placed.contains(i) && out[*i].1.iter().all(|w| placed.contains(w)))
            .or_else(|| (0..out.len()).find(|i| !placed.contains(i)));
        match next {
            Some(i) => placed.push(i),
            None => break,
        }
    }
    let titles: Vec<String> = out.iter().map(|(s, _)| s.title.clone()).collect();
    placed
        .into_iter()
        .map(|i| {
            let (mut sub, waits) = out[i].clone();
            sub.waits_for = waits.iter().map(|w| titles[*w].clone()).collect();
            sub
        })
        .collect()
}

/// A step's line with each `after` on a step of another sub-task than
/// `own` left out of its last parentheses, and the sub-tasks those were in,
/// as `whose` says. An `after` on a step nobody has stays.
fn keep_own_afters(line: &str, own: usize, whose: impl Fn(&str) -> Option<usize>) -> (String, Vec<usize>) {
    let keep = |after: &str| whose(after).is_none_or(|o| o == own);
    let (Some(open), Some(close)) = (line.rfind('('), line.rfind(')')) else {
        return (line.to_string(), Vec::new());
    };
    if close < open {
        return (line.to_string(), Vec::new());
    }
    let inner = &line[open + 1..close];
    let after_of = |e: &str| e.strip_prefix("after ").or_else(|| e.strip_prefix("After ")).map(str::trim).map(str::to_string);
    let entries: Vec<&str> = inner.split([',', ';']).map(str::trim).filter(|e| !e.is_empty()).collect();
    if !entries.iter().any(|e| after_of(e).is_some_and(|a| !keep(&a))) {
        return (line.to_string(), Vec::new());
    }
    let mut serves: Vec<&str> = Vec::new();
    let mut afters: Vec<&str> = Vec::new();
    let mut others: Vec<usize> = Vec::new();
    for e in entries {
        match after_of(e) {
            Some(a) if keep(&a) => afters.push(e),
            Some(a) => others.extend(whose(&a)),
            None => serves.push(e),
        }
    }
    let mut parts = serves.join(", ");
    if !afters.is_empty() {
        if !parts.is_empty() {
            parts.push_str("; ");
        }
        parts.push_str(&afters.join(", "));
    }
    let head = line[..open].trim_end();
    let tail = &line[close + 1..];
    let line = if parts.is_empty() { format!("{head}{tail}") } else { format!("{head} ({parts}){tail}") };
    (line, others)
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "## Sign-in endpoint\n\n\
        - [ ] 1. Add the endpoint (R-1, C-1)\n  - with its tests\n\
        - [ ] 2. Rate-limit it (R-2)\n\n\
        ## ACME-104 Session refresh\n\
        - [ ] 3. Refresh sessions (R-3; after api 2)\n";
    const WEB: &str = "## Sign-in endpoint\n\
        - [ ] 1. Call it (R-1, C-1; after api 1)\n\n\
        ## Remember me\n\
        - [ ] 2. Add the box (R-4; after web 1, after api 3)\n";

    fn files() -> Vec<(String, String)> {
        vec![("api".into(), API.into()), ("web".into(), WEB.into())]
    }

    #[test]
    fn the_same_heading_in_two_repositories_is_one_sub_task() {
        let subs = sub_tasks("Sign-in overhaul", &files());
        let titles: Vec<&str> = subs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Sign-in endpoint", "Session refresh", "Remember me"]);
        let first = &subs[0];
        assert_eq!(first.repos.iter().map(|r| r.folder.as_str()).collect::<Vec<_>>(), vec!["api", "web"]);
        assert_eq!(first.repos[0].steps.len(), 2);
        assert!(first.repos[0].tasks.contains("  - with its tests"), "what is under a step goes with it");
        assert_eq!(first.serves(), vec!["R-1", "R-2"]);
        assert_eq!(subs[1].key.as_deref(), Some("ACME-104"));
    }

    #[test]
    fn an_after_on_another_sub_task_is_waiting_for_it_and_leaves_its_steps() {
        let subs = sub_tasks("Sign-in overhaul", &files());
        assert!(subs[0].waits_for.is_empty(), "an after inside the sub-task is its own business");
        assert!(subs[0].repos[1].tasks.contains("(R-1, C-1; after api 1)"));
        assert_eq!(subs[1].waits_for, vec!["Sign-in endpoint"]);
        assert!(subs[1].repos[0].tasks.contains("3. Refresh sessions (R-3)\n"));
        assert_eq!(subs[2].waits_for, vec!["Sign-in endpoint", "Session refresh"]);
        assert!(subs[2].repos[0].tasks.contains("2. Add the box (R-4)"));
        assert_eq!(subs[2].repos[0].steps[0].number, 2, "steps keep their numbers");
    }

    #[test]
    fn steps_under_no_heading_are_their_repositorys_own_sub_task() {
        let subs = sub_tasks("Refunds", &[("api".into(), "## Tasks\n- [ ] 1. Do it (R-1)\n".into())]);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].title, "Refunds in api");
        assert_eq!(subs[0].repos[0].tasks, "## Tasks\n\n- [ ] 1. Do it (R-1)\n");
    }

    #[test]
    fn what_waits_is_listed_after_what_it_waits_for() {
        let api = "## Later\n- [ ] 1. Second (R-1; after api 2)\n## Sooner\n- [ ] 2. First (R-1)\n";
        let subs = sub_tasks("E", &[("api".into(), api.into())]);
        assert_eq!(subs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["Sooner", "Later"]);
    }

    #[test]
    fn a_heading_is_a_key_only_when_it_looks_like_one() {
        assert_eq!(key_of("ACME-104 Session refresh"), (Some("ACME-104".into()), "Session refresh".into()));
        assert_eq!(key_of("OAuth2-based sign-in"), (None, "OAuth2-based sign-in".into()));
        assert_eq!(key_of("ACME-104"), (None, "ACME-104".into()));
    }
}
