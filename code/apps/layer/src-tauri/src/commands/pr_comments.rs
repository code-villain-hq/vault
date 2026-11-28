//! Read one checkout's open pull request without invoking the feedback picker.

use std::path::Path;

use reqwest::Url;
use serde::Serialize;
use tauri::State;

use crate::config::{AppConfig, Checkout, GithubConfig};
use crate::error::{Error, Result};
use crate::git;
use crate::integrations::github::{GitHub, PrComments};

use super::github_sources::client_for;
use super::{off_runtime, pr_branches, AppState};

#[derive(Debug, Serialize)]
pub struct CheckoutPrComments {
    pub checkout_id: String,
    pub number: u64,
    pub url: String,
    #[serde(flatten)]
    pub comments: PrComments,
}

#[tauri::command]
pub async fn github_pr_comments(
    state: State<'_, AppState>,
    task_id: String,
    checkout_id: String,
    number: u64,
) -> Result<CheckoutPrComments> {
    let (checkout, branch) = state.config.with(|config| {
        let checkout = task_checkout(config, &task_id, &checkout_id)?;
        let branch = config
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .map(|task| task.branch.clone())
            .ok_or_else(|| Error::Other("That task is no longer available.".into()))?;
        Ok::<_, Error>((checkout, branch))
    })?;
    // The checkout's origin picks the GitHub source to ask; GitHub.com's
    // SSH-over-443 remote names ssh.github.com but belongs to github.com.
    let cfg = state.config.read();
    let path = checkout.path.clone();
    let (client, config) = off_runtime(move || {
        let origin = git::github_origin(Path::new(&path))?;
        let key = if origin.host == "ssh.github.com" { "github.com" } else { origin.host.as_str() };
        client_for(&cfg, key)
    })
    .await?
    .map_err(safe_connection_error)?;
    read_checkout_comments(&client, &config, checkout, branch, number).await
}

fn task_checkout(config: &AppConfig, task_id: &str, checkout_id: &str) -> Result<Checkout> {
    config
        .checkouts
        .iter()
        .find(|c| c.id == checkout_id && c.task_id == task_id)
        .cloned()
        .ok_or_else(|| Error::Other("That checkout does not belong to this task.".into()))
}

async fn read_checkout_comments(
    client: &GitHub,
    config: &GithubConfig,
    checkout: Checkout,
    task_branch: String,
    number: u64,
) -> Result<CheckoutPrComments> {
    if number == 0 || i32::try_from(number).is_err() {
        return Err(Error::Other("That pull request number is not valid.".into()));
    }
    let checkout_id = checkout.id.clone();
    let web_url = config.web_url.clone();
    let (owner, repo, branches) = off_runtime(move || {
        let (owner, repo) = checkout_repository(Path::new(&checkout.path), &web_url)?;
        Ok::<_, Error>((owner, repo, pr_branches::branches_of(&checkout, &task_branch)))
    })
    .await??;
    let selected = pr_branches::open_pull(client, &owner, &repo, &branches).await.map_err(safe_connection_error)?;
    let Some(pr) = selected.filter(|pr| pr.number == number && pr.state == "open" && branches.contains(&pr.head))
    else {
        return Err(Error::Other(
            "That pull request is no longer open for this checkout. Refresh the pull requests and try again.".into(),
        ));
    };
    // API and web URLs are set independently. A branch and PR number can
    // match on two different instances, so also check the API's permalink.
    if !matches_pr_permalink(&config.web_url, &pr.url, &owner, &repo, number) {
        return Err(Error::Other(
            "GitHub returned a different pull request for this checkout. Check the GitHub connection and try again."
                .into(),
        ));
    }
    let comments = client.pr_comments(&owner, &repo, number).await;
    Ok(CheckoutPrComments { checkout_id, number, url: pr.url, comments })
}

/// A permalink identifies a PR; it is never a request destination.
pub(super) fn matches_pr_permalink(web_url: &str, permalink: &str, owner: &str, repo: &str, number: u64) -> bool {
    let (Ok(web), Ok(link)) = (Url::parse(web_url), Url::parse(permalink)) else { return false };
    if !matches!(link.scheme(), "https" | "http")
        || link.origin() != web.origin()
        || !link.username().is_empty()
        || link.password().is_some()
    {
        return false;
    }
    let Some(mut segments) = link.path_segments() else { return false };
    // GitHub repository names compare without case, but the route and PR
    // number must match exactly, with no extra path segments.
    segments.next().is_some_and(|segment| segment.eq_ignore_ascii_case(owner))
        && segments.next().is_some_and(|segment| segment.eq_ignore_ascii_case(repo))
        && segments.next() == Some("pull")
        && segments.next() == Some(number.to_string().as_str())
        && segments.next().is_none()
}

fn checkout_repository(dir: &Path, web_url: &str) -> Result<(String, String)> {
    let missing = || Error::Other("That checkout does not have a repository on the connected GitHub site.".into());
    let web = Url::parse(web_url).map_err(|_| missing())?;
    let remote = git::effective_origin_url(dir).map_err(|_| missing())?;
    // The same owner and repo on two hosts are different repositories. An
    // SSH port need not match the port that serves the web interface.
    let mut github_ssh_443 = false;
    if remote.contains("://") {
        let url = Url::parse(&remote).map_err(|_| missing())?;
        match url.scheme() {
            "ssh" => {
                // This public endpoint is GitHub.com's documented exception;
                // other SSH aliases cannot verify the checkout's host.
                github_ssh_443 = web.scheme() == "https"
                    && web.host_str() == Some("github.com")
                    && web.port_or_known_default() == Some(443)
                    && url.host_str().is_some_and(|host| host.eq_ignore_ascii_case("ssh.github.com"))
                    && url.port() == Some(443)
                    && url.username() == "git"
                    && url.password().is_none();
            }
            "http" | "https" if url.origin() == web.origin() => {}
            _ => return Err(missing()),
        }
    } else if !remote.split_once(':').is_some_and(|(host, _)| !host.contains('/')) {
        // A relative folder can have the same spelling as a normalized key.
        return Err(missing());
    }
    let key = git::remote_key(&remote);
    let (host, rest) = key.split_once('/').ok_or_else(missing)?;
    let (owner, repo) = rest.split_once('/').ok_or_else(missing)?;
    let matching_host = web.host_str() == Some(host) || (github_ssh_443 && host == "ssh.github.com");
    if !matching_host || !repo_segment(owner) || !repo_segment(repo) {
        return Err(missing());
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

pub(super) fn repo_segment(value: &str) -> bool {
    !matches!(value, "" | "." | "..")
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn safe_connection_error(error: Error) -> Error {
    match error {
        Error::NotConfigured(_) | Error::TokenRejected(_) | Error::TokenNeedsSso(_) => error,
        // The remote may have echoed a URL or credential in its error body.
        _ => Error::Other("Could not read this checkout's pull request from GitHub. Try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Query,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn checkout(dir: &Path, id: &str, task: &str) -> Checkout {
        serde_json::from_value(json!({
            "id": id, "task_id": task, "project_id": id, "path": dir, "base": "main"
        }))
        .unwrap()
    }

    #[test]
    fn a_checkout_from_another_task_is_refused() {
        let cfg = AppConfig {
            checkouts: vec![checkout(Path::new("/unused"), "c1", "t1"), checkout(Path::new("/unused"), "c2", "t2")],
            ..Default::default()
        };
        assert_eq!(task_checkout(&cfg, "t1", "c1").unwrap().id, "c1");
        assert!(task_checkout(&cfg, "t1", "c2").is_err());
    }

    #[tokio::test]
    async fn no_open_pr_for_this_checkout_does_not_read_comments() {
        assert_missing("https://gh.example/acme/api.git", json!([])).await;
    }

    #[tokio::test]
    async fn a_number_other_than_the_selected_open_pr_is_refused() {
        assert_missing(
            "https://gh.example/acme/api.git",
            json!([{ "number": 43, "state": "open", "head": {"ref": "task"} }]),
        )
        .await;
    }

    #[tokio::test]
    async fn same_slug_from_a_different_host_does_not_read_comments() {
        assert_missing(
            "https://other.example/acme/api.git",
            json!([{ "number": 42, "state": "open", "head": {"ref": "task"} }]),
        )
        .await;
    }

    #[tokio::test]
    async fn a_local_origin_that_looks_like_a_github_path_does_not_read_comments() {
        assert_missing("gh.example/acme/api", json!([{ "number": 42, "state": "open", "head": {"ref": "task"} }]))
            .await;
    }

    async fn assert_missing(origin: &str, listed: Value) {
        let (got, reads) = call(origin, listed).await;
        assert!(matches!(got, Err(Error::Other(_))), "{got:?}");
        assert_eq!(reads, 0);
    }

    fn selected(url: &str) -> Value {
        json!([{ "number": 42, "state": "open", "head": {"ref": "task"}, "html_url": url }])
    }

    #[tokio::test]
    async fn a_matching_open_pr_with_a_permalink_on_another_web_origin_is_refused() {
        assert_missing("https://gh.example/acme/api.git", selected("https://other.example/acme/api/pull/42")).await;
    }

    #[tokio::test]
    async fn a_matching_open_pr_with_a_permalink_to_a_different_pr_is_refused() {
        for url in [
            "https://gh.example/acme/other/pull/42",
            "https://gh.example/acme/api/pull/43",
            "https://gh.example/acme/api/pull/42/files",
        ] {
            assert_missing("https://gh.example/acme/api.git", selected(url)).await;
        }
    }

    #[tokio::test]
    async fn a_matching_open_pr_reads_only_that_repo_and_returns_its_identity() {
        let (got, reads) = call("git@gh.example:acme/api.git", selected("https://gh.example/Acme/API/pull/42")).await;
        let out = got.unwrap();
        assert_eq!(out["checkout_id"], "c1");
        assert_eq!(out["number"], 42);
        assert_eq!(out["url"], "https://gh.example/Acme/API/pull/42");
        assert_eq!(out["comments_state"], "complete");
        assert_eq!(out["comments"][0]["body"], "Only on the selected PR");
        assert_eq!(reads, 3);
    }

    #[tokio::test]
    async fn github_com_accepts_its_official_ssh_over_443_remote() {
        let (got, reads) = call_on(
            "https://github.com",
            "ssh://git@ssh.github.com:443/acme/api.git",
            selected("https://github.com/acme/api/pull/42"),
        )
        .await;
        assert_eq!(got.unwrap()["number"], 42);
        assert_eq!(reads, 3);
    }

    #[tokio::test]
    async fn ssh_over_443_does_not_allow_unverified_aliases_or_other_github_origins() {
        for (web, origin, url) in [
            ("https://github.com", "ssh://git@alias.example:443/acme/api.git", "https://github.com/acme/api/pull/42"),
            ("https://github.com", "ssh://git@ssh.github.com:22/acme/api.git", "https://github.com/acme/api/pull/42"),
            ("https://github.com", "ssh://me@ssh.github.com:443/acme/api.git", "https://github.com/acme/api/pull/42"),
            ("https://gh.example", "ssh://git@ssh.github.com:443/acme/api.git", "https://gh.example/acme/api/pull/42"),
        ] {
            let (got, reads) = call_on(web, origin, selected(url)).await;
            assert!(got.is_err(), "{web} accepted {origin}: {got:?}");
            assert_eq!(reads, 0);
        }
    }

    async fn call(origin: &str, listed: Value) -> (Result<Value>, usize) {
        call_on("https://gh.example", origin, listed).await
    }

    async fn call_on(web_url: &str, origin: &str, listed: Value) -> (Result<Value>, usize) {
        let dir = std::env::temp_dir().join(format!("vl-pr-comments-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git::run_for_tests(&dir, &["init", "-q", "-b", "main"]).unwrap();
        git::run_for_tests(&dir, &["remote", "add", "origin", origin]).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let reads = requests.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cfg = GithubConfig {
            api_url: format!("http://{}", listener.local_addr().unwrap()),
            web_url: web_url.into(),
            review_team: None,
        };
        let app = Router::new()
            .route(
                "/repos/acme/api/pulls",
                get(move |Query(query): Query<HashMap<String, String>>| async move {
                    assert_eq!(query["head"], "acme:task");
                    assert_eq!(query["state"], "open");
                    Json(listed)
                }),
            )
            .route(
                "/graphql",
                post(move |Json(body): Json<Value>| async move {
                    assert_eq!(body["variables"]["owner"], "acme");
                    assert_eq!(body["variables"]["name"], "api");
                    assert_eq!(body["variables"]["number"], 42);
                    let query = body["query"].as_str().unwrap();
                    assert!(query.starts_with("query"));
                    reads.fetch_add(1, Ordering::SeqCst);
                    let (key, nodes) = if query.contains("reviewThreads(") {
                        ("reviewThreads", vec![])
                    } else if query.contains("reviews(") {
                        ("reviews", vec![])
                    } else {
                        (
                            "comments",
                            vec![json!({"author": {"login": "ana", "__typename": "User"},
                        "body": "Only on the selected PR", "url": "https://gh.example/acme/api/pull/42#issuecomment-1",
                        "createdAt": "2026-11-01T12:00:00Z"})],
                        )
                    };
                    let mut out = json!({ "data": { "repository": { "pullRequest": { "number": 42 } } } });
                    out["data"]["repository"]["pullRequest"][key] =
                        json!({"nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}});
                    Json(out)
                }),
            );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = GitHub::new(&cfg, "synthetic").unwrap();
        let got = read_checkout_comments(&client, &cfg, checkout(&dir, "c1", "t1"), "task".into(), 42)
            .await
            .map(|out| json!(out));
        std::fs::remove_dir_all(dir).unwrap();
        (got, requests.load(Ordering::SeqCst))
    }
}
