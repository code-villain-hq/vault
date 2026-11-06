//! The pull request template a repository keeps for its forge (PR-4), so a
//! drafted description fills it in. Before, the draft chose its own sections,
//! and a team's template (its checklist, its "How to test") was left out of
//! every pull request Layer opened.

use std::path::{Path, PathBuf};

/// One template: where it is in the repository, and its text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Template {
    pub path: String,
    pub text: String,
}

/// What a repository keeps: nothing, the one template its pull requests
/// start from, or several to pick from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Templates {
    None,
    One(Template),
    Choice(Vec<Template>),
}

const NAME: &str = "pull_request_template";
const EXTENSIONS: &[&str] = &["md", "txt", ""];

/// Where a forge looks for the template every pull request starts from, in
/// the order tried: GitHub (`.github`, root, `docs`), GitLab (its
/// `Default.md`), Bitbucket Cloud (`.bitbucket`), Azure DevOps
/// (`.azuredevops`, `.vsts`, `docs`, root), Forgejo and Gitea (`.forgejo`,
/// `.gitea`, `.github`, `docs`). A repository rarely keeps one for two forges.
const ONE: &[&[&str]] = &[
    &[".github"],
    &[".gitlab", "merge_request_templates"],
    &[".bitbucket"],
    &[".azuredevops"],
    &[".vsts"],
    &[".forgejo"],
    &[".gitea"],
    &["docs"],
    &[],
];

/// Azure DevOps' folders, where `pull_request_template/branches/<target>.md`
/// is the template for pull requests into that branch.
const AZURE: &[&[&str]] = &[&[".azuredevops"], &[".vsts"], &["docs"], &[]];

/// Folders of several templates with none the default: GitHub's
/// `PULL_REQUEST_TEMPLATE/` (in `.github`, root or `docs`), Azure DevOps'
/// `pull_request_template/`, and GitLab's merge request templates.
const SEVERAL: &[&[&str]] = &[
    &[".github", NAME],
    &[NAME],
    &["docs", NAME],
    &[".azuredevops", NAME],
    &[".vsts", NAME],
    &[".gitlab", "merge_request_templates"],
];

/// Past this a template is cut: one is a page, and the diff needs the room.
const MAX_TEXT: usize = 8_000;
/// At most this many templates to pick from.
const MAX_CHOICE: usize = 8;

/// The templates the repository at `root` keeps for a pull request into
/// `base`. Names are matched whatever their case, as GitHub and Azure DevOps
/// match them.
pub(crate) fn find(root: &Path, base: &str) -> Templates {
    for dir in AZURE {
        let branches: Vec<&str> = dir.iter().copied().chain([NAME, "branches"]).collect();
        if let Some(t) = named(root, &branches, base) {
            return Templates::One(t);
        }
    }
    for dir in ONE {
        let name = if dir.last() == Some(&"merge_request_templates") { "default" } else { NAME };
        if let Some(t) = named(root, dir, name) {
            return Templates::One(t);
        }
    }
    let mut found: Vec<Template> = Vec::new();
    for dir in SEVERAL {
        for t in every(root, dir) {
            if !found.iter().any(|f| f.path.eq_ignore_ascii_case(&t.path)) {
                found.push(t);
            }
        }
    }
    found.truncate(MAX_CHOICE);
    match found.len() {
        0 => Templates::None,
        1 => Templates::One(found.remove(0)),
        _ => Templates::Choice(found),
    }
}

/// What the agent asked to write the description is told, as it finds files
/// itself.
pub(crate) const PLACES: &str = "If a repository keeps a pull request template, fill it in: \
    `pull_request_template.md` in `.github/`, `.bitbucket/`, `.azuredevops/`, `.vsts/`, \
    `.forgejo/`, `.gitea/`, `docs/` or the root, several in a `PULL_REQUEST_TEMPLATE/` folder \
    there, or GitLab's `.gitlab/merge_request_templates/Default.md`.";

/// The prompt's part on templates, given each repository's. One description
/// goes to every pull request of the task, so templates the repositories
/// share are listed once.
pub(crate) fn prompt_section(found: &[(String, Templates)]) -> String {
    let mut listed: Vec<(&str, &Templates)> = Vec::new();
    for (repo, t) in found {
        if *t != Templates::None && !listed.iter().any(|(_, l)| same(l, t)) {
            listed.push((repo, t));
        }
    }
    let Some((_, first)) = listed.first() else { return String::new() };
    let intro = match (listed.len(), first) {
        (1, Templates::One(t)) => format!("The repository keeps a pull request template, `{}`.", t.path),
        (1, _) => "The repository keeps several pull request templates. Pick the one that fits this change.".into(),
        _ => "The repositories keep different pull request templates, and this one description goes to \
              every pull request: follow the first, and answer inside it what the others ask that it does not."
            .into(),
    };
    let mut out = format!(
        "\n# The template\n{intro} Fill it in rather than choosing your own sections: keep its \
         headings, checklists and order, answer each part from the change, write \"n/a\" under a part \
         that does not apply, and leave out its HTML comments. Tick a checklist item only where the \
         change shows it is true.\n"
    );
    for (repo, t) in listed {
        let each: Vec<&Template> = match t {
            Templates::One(t) => vec![t],
            Templates::Choice(ts) => ts.iter().collect(),
            Templates::None => Vec::new(),
        };
        for t in each {
            out.push_str(&format!("\n<template repo=\"{repo}\" path=\"{}\">\n{}\n</template>\n", t.path, t.text));
        }
    }
    out
}

fn same(a: &Templates, b: &Templates) -> bool {
    let texts = |t: &Templates| match t {
        Templates::One(t) => vec![t.text.clone()],
        Templates::Choice(ts) => ts.iter().map(|t| t.text.clone()).collect(),
        Templates::None => Vec::new(),
    };
    texts(a) == texts(b)
}

/// `root/<dir…>/<name>` with a template's extension, matched whatever its case.
fn named(root: &Path, dir: &[&str], name: &str) -> Option<Template> {
    let folder = within(root, dir)?;
    EXTENSIONS.iter().find_map(|ext| {
        let file = if ext.is_empty() { name.to_string() } else { format!("{name}.{ext}") };
        entry(&folder, &file).filter(|p| p.is_file()).and_then(|p| read(root, &p))
    })
}

/// Every template file directly in `root/<dir…>`, by name.
fn every(root: &Path, dir: &[&str]) -> Vec<Template> {
    let Some(folder) = within(root, dir) else { return Vec::new() };
    let Ok(read_dir) = std::fs::read_dir(&folder) else { return Vec::new() };
    let mut files: Vec<PathBuf> = read_dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["md", "txt"].iter().any(|x| e.eq_ignore_ascii_case(x)))
        })
        .collect();
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    files.iter().filter_map(|p| read(root, p)).collect()
}

/// `root/<parts…>`, each part matched whatever its case.
fn within(root: &Path, parts: &[&str]) -> Option<PathBuf> {
    parts.iter().try_fold(root.to_path_buf(), |at, part| entry(&at, part).filter(|p| p.is_dir()))
}

/// The entry of `dir` called `name`, whatever its case.
fn entry(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .map(|e| e.path())
}

fn read(root: &Path, path: &Path) -> Option<Template> {
    let mut text = std::fs::read_to_string(path).ok()?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if text.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n…");
    }
    let rel = path.strip_prefix(root).unwrap_or(path);
    Some(Template { path: rel.to_string_lossy().replace('\\', "/"), text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("vl-pr-template-{}", uuid::Uuid::new_v4()));
        for (path, text) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn one(t: Templates) -> Template {
        match t {
            Templates::One(t) => t,
            other => panic!("expected one template, got {other:?}"),
        }
    }

    #[test]
    fn githubs_template_is_found_whatever_its_case() {
        let root = repo(&[(".github/PULL_REQUEST_TEMPLATE.md", "## Why\n")]);
        let t = one(find(&root, "main"));
        assert_eq!(t.path, ".github/PULL_REQUEST_TEMPLATE.md");
        assert_eq!(t.text, "## Why");
    }

    #[test]
    fn each_forges_usual_place_is_looked_in() {
        for path in [
            "pull_request_template.md",
            "docs/pull_request_template.md",
            ".gitlab/merge_request_templates/Default.md",
            ".bitbucket/pull_request_template.md",
            ".azuredevops/pull_request_template.txt",
            ".vsts/pull_request_template.md",
            ".forgejo/PULL_REQUEST_TEMPLATE.md",
            ".gitea/pull_request_template.md",
        ] {
            let root = repo(&[(path, "Fill me in")]);
            assert_eq!(one(find(&root, "main")).path, path);
        }
    }

    #[test]
    fn azures_template_for_the_target_branch_comes_before_the_default() {
        let root = repo(&[
            (".azuredevops/pull_request_template.md", "default"),
            (".azuredevops/pull_request_template/branches/release.md", "for release"),
        ]);
        assert_eq!(one(find(&root, "release")).text, "for release");
        assert_eq!(one(find(&root, "main")).text, "default");
    }

    #[test]
    fn several_templates_and_no_default_are_a_choice() {
        let root = repo(&[
            (".github/PULL_REQUEST_TEMPLATE/feature.md", "feature"),
            (".github/PULL_REQUEST_TEMPLATE/bugfix.md", "bugfix"),
        ]);
        let Templates::Choice(ts) = find(&root, "main") else { panic!("expected a choice") };
        assert_eq!(ts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["bugfix", "feature"]);
    }

    #[test]
    fn a_repository_without_a_template_has_none_and_adds_nothing_to_the_prompt() {
        let root = repo(&[("README.md", "hello"), ("docs/guide.md", "how")]);
        assert_eq!(find(&root, "main"), Templates::None);
        assert_eq!(prompt_section(&[("api".into(), Templates::None)]), "");
    }

    #[test]
    fn repositories_sharing_a_template_list_it_once() {
        let t = Template { path: ".github/pull_request_template.md".into(), text: "## Why".into() };
        let out =
            prompt_section(&[("api".into(), Templates::One(t.clone())), ("web".into(), Templates::One(t.clone()))]);
        assert_eq!(out.matches("<template ").count(), 1);
        assert!(out.contains("The repository keeps a pull request template, `.github/pull_request_template.md`."));
    }
}
