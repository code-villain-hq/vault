//! Read one Reviews card without resolving a task or local repository.

use serde::Serialize;
use tauri::State;

use crate::error::{Error, Result};
use crate::integrations::github::{GitHub, PrChecks, PrComments};

use super::pr_comments::{matches_pr_permalink, repo_segment};
use super::{github_client, AppState};

#[derive(Debug, Serialize)]
pub struct ReviewPrDetails {
    pub repo: String,
    pub number: u64,
    pub url: String,
    pub head_sha: String,
    #[serde(flatten)]
    pub comments: PrComments,
    #[serde(flatten)]
    pub checks: PrChecks,
}

#[tauri::command]
pub async fn github_review_pr_details(
    state: State<'_, AppState>,
    repo: String,
    number: u64,
    url: String,
) -> Result<ReviewPrDetails> {
    let (client, config) = github_client(&state).await.map_err(review_read_error)?;
    read_review_pr_details(&client, &config.web_url, &repo, number, &url).await
}

async fn read_review_pr_details(
    client: &GitHub,
    web_url: &str,
    slug: &str,
    number: u64,
    url: &str,
) -> Result<ReviewPrDetails> {
    let (owner, repo) = slug
        .split_once('/')
        .filter(|(owner, repo)| repo_segment(owner) && repo_segment(repo))
        .ok_or_else(|| Error::Other("That GitHub repository is not valid.".into()))?;
    if number == 0 || i32::try_from(number).is_err() {
        return Err(Error::Other("That pull request number is not valid.".into()));
    }
    if !matches_pr_permalink(web_url, url, owner, repo, number) {
        return Err(Error::Other(
            "That pull request link does not match this card on the connected GitHub site. Refresh Reviews and try again."
                .into(),
        ));
    }

    let pr = client.pull(owner, repo, number).await.map_err(review_read_error)?;
    if pr.number != number || pr.state != "open" {
        return Err(Error::Other(
            "That pull request is no longer available in Reviews. Refresh Reviews and try again.".into(),
        ));
    }
    // The configured API can return the same repo and number on a different
    // instance. Verify its web permalink before showing any detail.
    if !matches_pr_permalink(web_url, &pr.url, owner, repo, number) {
        return Err(Error::Other(
            "GitHub returned a different pull request for this card. Check the GitHub connection and try again.".into(),
        ));
    }

    // A fork has a different head repository, but its check runs are read
    // from the base repo by commit SHA. A missing SHA gives unavailable checks.
    let (comments, checks) =
        tokio::join!(client.pr_comments(owner, repo, number), client.pr_checks(owner, repo, &pr.head_sha));
    Ok(ReviewPrDetails { repo: slug.to_owned(), number, url: pr.url, head_sha: pr.head_sha, comments, checks })
}

fn review_read_error(error: Error) -> Error {
    match error {
        Error::NotConfigured(_) | Error::TokenRejected(_) | Error::TokenNeedsSso(_) => error,
        // The error body can contain a URL or other private data.
        _ => Error::Other("Could not read this pull request from GitHub. Try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::Request,
        http::StatusCode,
        middleware::{self, Next},
        routing::{get, post},
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::config::GithubConfig;

    const WEB: &str = "https://github.com";
    const GHE: &str = "https://ghe.example:8443";
    const URL: &str = "https://github.com/acme/widget/pull/42";
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const REST: &str = "GET /repos/acme/widget/pulls/42";

    fn fork_pr(url: &str) -> Value {
        json!({
            "number": 42, "state": "open", "html_url": url,
            "title": "Fix sign-in", "draft": false, "user": {"login": "contributor"},
            "base": {"ref": "main", "repo": {"full_name": "acme/widget"}},
            "head": {"ref": "fix-sign-in", "sha": SHA, "repo": {"full_name": "contributor/widget"}},
            "merged": false, "mergeable_state": "clean", "comments": 1, "review_comments": 0
        })
    }

    #[tokio::test]
    async fn a_fork_without_a_local_repository_uses_the_exact_head_in_the_base_repository() {
        // This feed card has no task, checkout or local repository. The card
        // and API permalinks can identify the same PR with different spelling.
        let verified_url = "https://github.com/Acme/Widget/pull/42";
        let (result, requests) = Fixture::new(fork_pr(verified_url)).read(WEB, "acme/widget", 42, URL).await;
        let out = result.unwrap();
        assert_eq!(
            out,
            json!({
                "repo": "acme/widget", "number": 42, "url": verified_url, "head_sha": SHA,
                "comments": [{
                    "author": "ana", "bot": false, "state": null, "body": "Please verify this.",
                    "url": "https://github.com/Acme/Widget/pull/42#issuecomment-1", "at": "2026-11-01T12:00:00Z"
                }],
                "reviews": [], "threads": [],
                "comments_state": "complete", "reviews_state": "complete", "threads_state": "complete",
                "checks": [{
                    "name": "build", "status": "completed", "conclusion": "failure",
                    "url": "https://github.com/acme/widget/runs/1"
                }],
                "checks_state": "complete"
            })
        );
        assert_eq!(requests[0], REST);
        assert_eq!(requests.len(), 5);
        assert_eq!(requests.iter().filter(|r| r.as_str() == "POST /graphql").count(), 3);
        assert!(requests.contains(&format!("GET /repos/acme/widget/commits/{SHA}/check-runs?per_page=100&page=1")));
    }

    #[tokio::test]
    async fn an_enterprise_permalink_uses_the_separately_configured_api() {
        let verified_url = "https://ghe.example:8443/acme/widget/pull/42";
        let mut fixture = Fixture::new(fork_pr(verified_url));
        fixture.enterprise = true;
        let (result, requests) = fixture.read(GHE, "acme/widget", 42, verified_url).await;
        let out = result.unwrap();
        assert_eq!(out["url"], verified_url);
        assert_eq!(out["comments"][0]["body"], "Please verify this.");
        assert_eq!(out["checks_state"], "complete");
        assert_eq!(requests[0], "GET /api/v3/repos/acme/widget/pulls/42");
        assert!(requests.iter().any(|r| r == "POST /api/graphql"));
    }

    #[tokio::test]
    async fn bad_slugs_and_numbers_do_not_send_requests() {
        for (slug, number) in [
            ("acme", 42),
            ("/widget", 42),
            ("acme/", 42),
            ("acme/widget/extra", 42),
            ("acme/..", 42),
            ("./widget", 42),
            ("acme/wid%67et", 42),
            ("acme/wid get", 42),
            ("acme/widget", 0),
            ("acme/widget", i32::MAX as u64 + 1),
        ] {
            let url = format!("{WEB}/{slug}/pull/{number}");
            let (result, requests) = Fixture::new(fork_pr(URL)).read(WEB, slug, number, &url).await;
            assert!(matches!(result, Err(Error::Other(_))), "{slug}#{number}: {result:?}");
            assert!(requests.is_empty(), "{slug}#{number}: {requests:?}");
        }
    }

    #[tokio::test]
    async fn a_card_url_from_another_origin_or_pr_does_not_send_requests() {
        for url in [
            "http://github.com/acme/widget/pull/42",
            "https://ghe.example/acme/widget/pull/42",
            "https://github.com:8443/acme/widget/pull/42",
            "https://user@github.com/acme/widget/pull/42",
            "https://github.com/acme/other/pull/42",
            "https://github.com/acme/widget/pull/43",
            "https://github.com/acme/widget/pull/42/files",
        ] {
            let (result, requests) = Fixture::new(fork_pr(URL)).read(WEB, "acme/widget", 42, url).await;
            assert!(matches!(result, Err(Error::Other(_))), "{url}: {result:?}");
            assert!(requests.is_empty(), "{url}: {requests:?}");
        }
    }

    #[tokio::test]
    async fn a_wrong_or_closed_rest_pr_never_loads_comments_or_checks() {
        for (field, value) in [
            ("number", json!(43)),
            ("state", json!("closed")),
            ("html_url", json!("https://ghe.example/acme/widget/pull/42")),
            ("html_url", json!("https://github.com/acme/other/pull/42")),
            ("html_url", json!("https://github.com/acme/widget/pull/43")),
        ] {
            let mut pr = fork_pr(URL);
            pr[field] = value.clone();
            let (result, requests) = Fixture::new(pr).read(WEB, "acme/widget", 42, URL).await;
            assert!(matches!(result, Err(Error::Other(_))), "{field} {value}: {result:?}");
            assert_eq!(requests, [REST]);
        }
    }

    #[tokio::test]
    async fn an_unavailable_rest_identity_never_loads_details_or_surfaces_the_remote_body() {
        for status in [StatusCode::NOT_FOUND, StatusCode::UNAUTHORIZED] {
            let mut fixture = Fixture::new(json!({ "message": "private upstream detail" }));
            fixture.pr_status = status;
            let (result, requests) = fixture.read(WEB, "acme/widget", 42, URL).await;
            let error = result.unwrap_err();
            if status == StatusCode::UNAUTHORIZED {
                assert!(matches!(error, Error::TokenRejected("GitHub")));
            } else {
                assert!(matches!(error, Error::Other(_)));
            }
            assert!(!error.to_string().contains("private upstream detail"));
            assert_eq!(requests, [REST]);
        }
    }

    #[tokio::test]
    async fn a_missing_or_invalid_head_keeps_comments_without_requesting_checks() {
        let mut missing = fork_pr(URL);
        missing["head"].as_object_mut().unwrap().remove("sha");
        let mut invalid = fork_pr(URL);
        invalid["head"]["sha"] = json!("fix-sign-in");
        for (pr, head) in [(missing, ""), (invalid, "fix-sign-in")] {
            let (result, requests) = Fixture::new(pr).read(WEB, "acme/widget", 42, URL).await;
            let out = result.unwrap();
            assert_eq!(out["head_sha"], head);
            assert_eq!(out["comments"][0]["body"], "Please verify this.");
            assert_eq!(out["comments_state"], "complete");
            assert_eq!(out["checks"], json!([]));
            assert_eq!(out["checks_state"], "unavailable");
            assert_eq!(requests, [REST, "POST /graphql", "POST /graphql", "POST /graphql"]);
        }
    }

    #[tokio::test]
    async fn failed_check_runs_keep_successful_comments() {
        let mut fixture = Fixture::new(fork_pr(URL));
        fixture.check_status = StatusCode::FORBIDDEN;
        let (result, _) = fixture.read(WEB, "acme/widget", 42, URL).await;
        let out = result.unwrap();
        assert_eq!(out["checks"], json!([]));
        assert_eq!(out["checks_state"], "unavailable");
        assert_eq!(out["comments"][0]["body"], "Please verify this.");
        assert_eq!(out["comments_state"], "complete");
    }

    #[tokio::test]
    async fn failed_comments_keep_successful_check_runs() {
        let mut fixture = Fixture::new(fork_pr(URL));
        fixture.graphql_status = StatusCode::FORBIDDEN;
        let (result, _) = fixture.read(WEB, "acme/widget", 42, URL).await;
        let out = result.unwrap();
        for (section, state) in
            [("comments", "comments_state"), ("reviews", "reviews_state"), ("threads", "threads_state")]
        {
            assert_eq!(out[section], json!([]));
            assert_eq!(out[state], "unavailable");
        }
        assert_eq!(out["checks"][0]["conclusion"], "failure");
        assert_eq!(out["checks_state"], "complete");
    }

    #[tokio::test]
    async fn an_incomplete_section_keeps_its_rows_and_does_not_change_complete_sections() {
        let mut fixture = Fixture::new(fork_pr(URL));
        fixture.partial_comments = true;
        let (result, _) = fixture.read(WEB, "acme/widget", 42, URL).await;
        let out = result.unwrap();
        assert_eq!(out["comments"][0]["body"], "Please verify this.");
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(out["reviews_state"], "complete");
        assert_eq!(out["threads_state"], "complete");
        assert_eq!(out["checks"][0]["conclusion"], "failure");
        assert_eq!(out["checks_state"], "complete");
    }

    struct Fixture {
        pr: Value,
        pr_status: StatusCode,
        graphql_status: StatusCode,
        check_status: StatusCode,
        partial_comments: bool,
        enterprise: bool,
    }

    impl Fixture {
        fn new(pr: Value) -> Self {
            Self {
                pr,
                pr_status: StatusCode::OK,
                graphql_status: StatusCode::OK,
                check_status: StatusCode::OK,
                partial_comments: false,
                enterprise: false,
            }
        }

        async fn read(self, web: &str, slug: &str, number: u64, url: &str) -> (Result<Value>, Vec<String>) {
            let Self { pr, pr_status, graphql_status, check_status, partial_comments, enterprise } = self;
            let rest = if enterprise { "/api/v3" } else { "" };
            let graph = if enterprise { "/api/graphql" } else { "/graphql" };
            let comment_url = format!("{}#issuecomment-1", pr["html_url"].as_str().unwrap_or_default());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = requests.clone();
            let app = Router::new()
                .route(&format!("{rest}/repos/acme/widget/pulls/42"), get(move || async move { (pr_status, Json(pr)) }))
                .route(
                    &format!("{rest}/repos/acme/widget/commits/{SHA}/check-runs"),
                    get(move || async move {
                        (
                            check_status,
                            Json(json!({
                                "total_count": 1,
                                "check_runs": [{
                                    "id": 1, "head_sha": SHA, "name": "build",
                                    "status": "completed", "conclusion": "failure",
                                    "html_url": "https://github.com/acme/widget/runs/1"
                                }]
                            })),
                        )
                    }),
                )
                .route(
                    graph,
                    post(move |Json(body): Json<Value>| {
                        let comment_url = comment_url.clone();
                        async move {
                            assert_eq!(
                                body["variables"],
                                json!({
                                    "owner": "acme", "name": "widget", "number": 42, "after": null
                                })
                            );
                            let query = body["query"].as_str().unwrap();
                            assert!(query.starts_with("query"));
                            let (key, nodes) = if query.contains("reviewThreads(") {
                                ("reviewThreads", json!([]))
                            } else if query.contains("reviews(") {
                                ("reviews", json!([]))
                            } else {
                                assert!(query.contains("comments("));
                                (
                                    "comments",
                                    json!([{
                                        "author": {"login": "ana", "__typename": "User"},
                                        "body": "Please verify this.", "url": comment_url,
                                        "createdAt": "2026-11-01T12:00:00Z"
                                    }]),
                                )
                            };
                            let mut out = json!({ "data": { "repository": { "pullRequest": { "number": 42 } } } });
                            out["data"]["repository"]["pullRequest"][key] =
                                json!({"nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}});
                            if partial_comments && key == "comments" {
                                out["errors"] = json!([{ "message": "Partial read" }]);
                            }
                            (graphql_status, Json(out))
                        }
                    }),
                )
                .fallback(|| async { StatusCode::NOT_FOUND })
                .layer(middleware::from_fn(move |request: Request, next: Next| {
                    recorded.lock().unwrap().push(format!("{} {}", request.method(), request.uri()));
                    next.run(request)
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let config = GithubConfig {
                api_url: format!("http://{}{rest}", listener.local_addr().unwrap()),
                web_url: web.into(),
                review_team: None,
            };
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let client = GitHub::new(&config, "synthetic").unwrap();
            let result =
                read_review_pr_details(&client, &config.web_url, slug, number, url).await.map(|out| json!(out));
            let requests = requests.lock().unwrap().clone();
            (result, requests)
        }
    }
}
