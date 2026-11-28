//! On-demand reading, separate from the feedback picker's selection and logs.

use std::collections::HashSet;
use std::future::Future;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::{timeout_at, Duration, Instant};

use super::{parse_note, parse_thread, GitHub, Note, ReadState, ReviewThread};

#[derive(Debug, Clone, Default, Serialize)]
pub struct PrComments {
    pub comments: Vec<Note>,
    pub reviews: Vec<Note>,
    pub threads: Vec<ReviewThread>,
    pub comments_state: ReadState,
    pub reviews_state: ReadState,
    pub threads_state: ReadState,
}

const MAX_PAGES: usize = 10;
// Limit both fan-out per thread and requests across the whole PR.
const MAX_REPLY_REQUESTS: usize = 40;
const SECTION_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_INFO: &str = "pageInfo { hasNextPage endCursor }";
const NOTE_FIELDS: &str = "author { login __typename } body url";
const PR_PATH: &str = "/data/repository/pullRequest";

impl GitHub {
    /// Read one known pull request with no dependency on tasks, git or CI.
    pub async fn pr_comments(&self, owner: &str, repo: &str, number: u64) -> PrComments {
        self.pr_comments_with_timeout(owner, repo, number, SECTION_TIMEOUT).await
    }

    async fn pr_comments_with_timeout(&self, owner: &str, repo: &str, number: u64, timeout: Duration) -> PrComments {
        let pr = Pr { owner, repo, number };
        let conversation = format!("{NOTE_FIELDS} createdAt");
        let submitted = format!("{NOTE_FIELDS} state submittedAt");
        // All sections start together. For threads, listing and reply
        // follow-ups share this deadline rather than restarting the clock.
        let deadline = Instant::now() + timeout;
        let (comments, reviews, (threads, threads_state)) = tokio::join!(
            self.pr_pages(pr, "comments", 100, &conversation, deadline),
            self.pr_pages(pr, "reviews", 100, &submitted, deadline),
            self.comment_threads(pr, deadline),
        );
        let (comments, comments_state) = comments.notes(NoteKind::Conversation);
        let (reviews, reviews_state) = reviews.notes(NoteKind::Review);
        PrComments { comments, reviews, threads, comments_state, reviews_state, threads_state }
    }

    async fn pr_pages(&self, pr: Pr<'_>, field: &str, size: usize, selection: &str, deadline: Instant) -> Pages {
        // A first-page error in one connection must not stop the others.
        let query = format!(
            "query($owner: String!, $name: String!, $number: Int!, $after: String) {{
              repository(owner: $owner, name: $name) {{
                pullRequest(number: $number) {{
                  number
                  {field}(first: {size}, after: $after) {{ {PAGE_INFO} nodes {{ {selection} }} }}
                }}
              }}
            }}"
        );
        let query = &query;
        let mut pages = Pages::default();
        let mut budget = MAX_PAGES;
        pages
            .read(None, MAX_PAGES, &mut budget, deadline, |after| async move {
                let vars = json!({ "owner": pr.owner, "name": pr.repo, "number": pr.number, "after": after });
                let mut v = self.graphql(query, vars).await.ok()?;
                let errors = graphql_errors(&v);
                let found = v.pointer_mut(PR_PATH)?;
                if found.get("number").and_then(Value::as_u64) != Some(pr.number) {
                    return None;
                }
                Page::take(found.get_mut(field)?, errors)
            })
            .await;
        pages
    }

    async fn comment_threads(&self, pr: Pr<'_>, deadline: Instant) -> (Vec<ReviewThread>, ReadState) {
        // The hunk has its own alias so it stays pinned to the first comment
        // even when the thread's replies continue on later pages.
        let selection = format!(
            "id isResolved isOutdated path line originalLine startLine originalStartLine diffSide
             comments(first: 20) {{ {PAGE_INFO} nodes {{ {NOTE_FIELDS} createdAt submission: state }} }}
             hunk: comments(first: 1) {{ nodes {{ diffHunk submission: state }} }}"
        );
        let Pages { nodes, mut state } = self.pr_pages(pr, "reviewThreads", 25, &selection, deadline).await;
        let mut threads = Vec::new();
        let mut budget = MAX_REPLY_REQUESTS;
        for mut node in nodes {
            let (Some(id), Some(_), Some(_), Some(_)) = (
                node.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()),
                node.get("path").and_then(Value::as_str),
                node.get("isResolved").and_then(Value::as_bool),
                node.get("isOutdated").and_then(Value::as_bool),
            ) else {
                state = ReadState::Incomplete;
                continue;
            };
            let id = id.to_owned();
            let (comments, replies_state) =
                self.thread_replies(&id, node.get_mut("comments"), &mut budget, deadline).await;
            if replies_state != ReadState::Complete {
                state = ReadState::Incomplete;
            }
            if !comments.is_empty() {
                if node.pointer("/hunk/nodes/0/submission").and_then(Value::as_str) == Some("PENDING") {
                    if let Some(hunk) = node.get_mut("hunk") {
                        *hunk = Value::Null;
                    }
                }
                threads.push(parse_thread(&node, comments));
            }
        }
        (threads, state)
    }

    async fn thread_replies(
        &self,
        id: &str,
        first: Option<&mut Value>,
        budget: &mut usize,
        deadline: Instant,
    ) -> (Vec<Note>, ReadState) {
        let mut pages = Pages::default();
        if let Some(page) = first.and_then(|first| Page::take(first, false)) {
            if let Some(after) = pages.append(page) {
                let query = format!(
                    "query($id: ID!, $after: String) {{
                      node(id: $id) {{
                        ... on PullRequestReviewThread {{
                          id
                          comments(first: 20, after: $after) {{
                            {PAGE_INFO} nodes {{ {NOTE_FIELDS} createdAt submission: state }}
                          }}
                        }}
                      }}
                    }}"
                );
                let query = &query;
                pages
                    .read(Some(after), MAX_PAGES - 1, budget, deadline, |after| async move {
                        let vars = json!({ "id": id, "after": after });
                        let mut v = self.graphql(query, vars).await.ok()?;
                        let errors = graphql_errors(&v);
                        let node = v.pointer_mut("/data/node")?;
                        if node.get("id").and_then(Value::as_str) != Some(id) {
                            return None;
                        }
                        Page::take(node.get_mut("comments")?, errors)
                    })
                    .await;
            }
        }
        pages.notes(NoteKind::Reply)
    }
}

#[derive(Clone, Copy)]
struct Pr<'a> {
    owner: &'a str,
    repo: &'a str,
    number: u64,
}

fn graphql_errors(v: &Value) -> bool {
    v.get("errors").is_some_and(|errors| !errors.as_array().is_some_and(Vec::is_empty))
}

struct Page {
    nodes: Vec<Value>,
    next: Option<String>,
    clean: bool,
}

impl Page {
    fn take(connection: &mut Value, errors: bool) -> Option<Self> {
        let Value::Array(nodes) = connection.get_mut("nodes")?.take() else { return None };
        let has_next = connection.pointer("/pageInfo/hasNextPage").and_then(Value::as_bool);
        let cursor = connection.pointer("/pageInfo/endCursor");
        let (next, page_info_ok) = match (has_next, cursor) {
            (Some(false), Some(Value::Null | Value::String(_))) => (None, true),
            (Some(true), Some(Value::String(next))) if !next.is_empty() => (Some(next.to_owned()), true),
            _ => (None, false),
        };
        // An empty malformed connection proves nothing. An imperfect page
        // with items is still useful, but we cannot call the read complete.
        (page_info_ok || !nodes.is_empty()).then_some(Self { nodes, next, clean: page_info_ok && !errors })
    }
}

#[derive(Default)]
struct Pages {
    nodes: Vec<Value>,
    state: ReadState,
}

impl Pages {
    fn append(&mut self, page: Page) -> Option<String> {
        self.nodes.extend(page.nodes);
        self.state = if page.clean { ReadState::Complete } else { ReadState::Incomplete };
        if page.clean {
            page.next
        } else {
            None
        }
    }

    async fn read<F: Future<Output = Option<Page>>>(
        &mut self,
        mut after: Option<String>,
        cap: usize,
        budget: &mut usize,
        deadline: Instant,
        fetch: impl Fn(Option<String>) -> F,
    ) {
        let mut seen: HashSet<_> = after.iter().cloned().collect();
        for _ in 0..cap {
            if *budget == 0 || Instant::now() >= deadline {
                break;
            }
            *budget -= 1;
            let Ok(Some(page)) = timeout_at(deadline, fetch(after)).await else { break };
            let Some(next) = self.append(page) else { return };
            if !seen.insert(next.clone()) {
                break;
            }
            after = Some(next);
        }
        if self.state == ReadState::Complete {
            self.state = ReadState::Incomplete;
        }
    }

    fn notes(self, kind: NoteKind) -> (Vec<Note>, ReadState) {
        let mut state = self.state;
        let mut notes = Vec::new();
        for node in self.nodes {
            match (kind, node.get("state").and_then(Value::as_str), node.get("submission").and_then(Value::as_str)) {
                (NoteKind::Review, Some("PENDING"), _) | (NoteKind::Reply, _, Some("PENDING")) => continue,
                (NoteKind::Review, None, _) | (NoteKind::Reply, _, None) => {
                    state = ReadState::Incomplete;
                    continue;
                }
                _ => {}
            }
            if node.get("body").and_then(Value::as_str).is_none()
                || node.get("url").and_then(Value::as_str).is_none_or(str::is_empty)
            {
                state = ReadState::Incomplete;
                continue;
            }
            let note = parse_note(&node);
            if !note.body.is_empty() || matches!(kind, NoteKind::Reply) {
                notes.push(note);
            }
        }
        (notes, state)
    }
}

#[derive(Clone, Copy)]
enum NoteKind {
    Conversation,
    Review,
    Reply,
}

#[cfg(test)]
mod tests {
    use super::super::GitHub;
    use crate::config::GithubConfig;
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::future::{pending, ready, Future};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    async fn read(github: &GitHub) -> Value {
        json!(github.pr_comments("acme", "api", 42).await)
    }

    async fn github(respond: impl Fn(Value) -> Value + Send + Sync + 'static) -> GitHub {
        async_github(move |body| ready(respond(body))).await
    }

    async fn async_github<F>(respond: impl Fn(Value) -> F + Send + Sync + 'static) -> GitHub
    where
        F: Future<Output = Value> + Send + 'static,
    {
        let respond = Arc::new(respond);
        let app = Router::new().route(
            "/api/graphql",
            post(move |Json(body): Json<Value>| {
                let respond = respond.clone();
                async move {
                    assert!(body["query"].as_str().unwrap().starts_with("query"));
                    Json(respond(body).await)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_url = format!("http://{}/api/v3", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        GitHub::new(&GithubConfig { api_url, web_url: "https://gh.example".into(), review_team: None }, "synthetic")
            .unwrap()
    }

    fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
        json!({ "nodes": nodes, "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next } })
    }

    fn pr(key: &str, connection: Value) -> Value {
        let mut data = json!({ "data": { "repository": { "pullRequest": { "number": 42 } } } });
        data["data"]["repository"]["pullRequest"][key] = connection;
        data
    }

    fn note(body: &str) -> Value {
        json!({ "author": { "login": "ana", "__typename": "User" }, "body": body,
            "url": format!("https://gh.example/acme/api/pull/42#{body}"), "createdAt": "2026-11-01T12:00:00Z" })
    }

    fn review(body: &str, state: &str) -> Value {
        let mut note = note(body);
        note.as_object_mut().unwrap().remove("createdAt");
        note["state"] = json!(state);
        note["submittedAt"] = if state == "PENDING" { Value::Null } else { json!("2026-11-01T12:00:00Z") };
        note
    }

    fn reply(body: &str, submission: &str) -> Value {
        let mut note = note(body);
        note["submission"] = json!(submission);
        note
    }

    fn thread(id: &str, nodes: Vec<Value>, next: Option<&str>) -> Value {
        json!({ "id": id, "path": "a.rs", "isResolved": true, "isOutdated": true,
            "line": null, "startLine": null, "originalLine": 10, "originalStartLine": 9, "diffSide": "RIGHT",
            "comments": page(nodes, next), "hunk": { "nodes": [ { "diffHunk": "@@ -1,0 +9,2 @@\n+nine\n+ten", "submission": "SUBMITTED" } ] } })
    }

    fn section(body: &Value) -> &str {
        let query = body["query"].as_str().unwrap();
        if query.contains("node(id:") {
            "replies"
        } else if query.contains("reviewThreads") {
            "reviewThreads"
        } else if query.contains("reviews(") {
            "reviews"
        } else {
            "comments"
        }
    }

    fn replies(id: &str, connection: Value) -> Value {
        json!({ "data": { "node": { "id": id, "comments": connection } } })
    }

    fn bodies(notes: &Value) -> Vec<&str> {
        notes.as_array().unwrap().iter().map(|n| n["body"].as_str().unwrap()).collect()
    }

    async fn read_with_short_deadline(github: &GitHub) -> Value {
        // The outer timeout makes RED fail promptly even if the reader has no deadline.
        let read = github.pr_comments_with_timeout("acme", "api", 42, Duration::from_secs(1));
        json!(tokio::time::timeout(Duration::from_secs(4), read).await.expect("section read exceeded its deadline"))
    }

    #[tokio::test]
    async fn a_stalled_thread_list_page_spends_the_reply_deadline_without_losing_embedded_replies() {
        let followups = Arc::new(AtomicUsize::new(0));
        let called = followups.clone();
        let gh = async_github(move |body| {
            let called = called.clone();
            async move {
                let key = section(&body);
                match (key, body["variables"]["after"].as_str()) {
                    ("comments", None) => pr(key, page(vec![note("conversation before stall")], Some("c1"))),
                    ("comments", Some("c1")) | ("reviewThreads", Some("t1")) => pending::<Value>().await,
                    ("reviewThreads", None) => pr(
                        key,
                        page(vec![thread("thread-first", vec![reply("opening", "SUBMITTED")], Some("r1"))], Some("t1")),
                    ),
                    ("replies", Some("r1")) => {
                        called.fetch_add(1, Ordering::SeqCst);
                        replies("thread-first", page(vec![reply("too late", "SUBMITTED")], None))
                    }
                    ("reviews", None) => pr(key, page(vec![review("submitted", "APPROVED")], None)),
                    _ => panic!("unexpected request: {body}"),
                }
            }
        })
        .await;
        let out = read_with_short_deadline(&gh).await;
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(bodies(&out["comments"]), ["conversation before stall"]);
        assert_eq!(out["threads_state"], "incomplete");
        assert_eq!(bodies(&out["threads"][0]["comments"]), ["opening"]);
        assert_eq!(followups.load(Ordering::SeqCst), 0);
        assert_eq!(out["reviews_state"], "complete");
        assert_eq!(bodies(&out["reviews"]), ["submitted"]);
    }

    #[tokio::test]
    async fn a_stalled_reply_page_preserves_read_replies_and_does_not_start_another_threads_followups() {
        let second_thread_followups = Arc::new(AtomicUsize::new(0));
        let called = second_thread_followups.clone();
        let gh = async_github(move |body| {
            let called = called.clone();
            async move {
                let vars = &body["variables"];
                let key = section(&body);
                match (key, vars["id"].as_str(), vars["after"].as_str()) {
                    ("reviewThreads", _, None) => pr(
                        key,
                        page(
                            vec![
                                thread("t1", vec![reply("opening", "SUBMITTED")], Some("r1")),
                                thread("t2", vec![reply("other opening", "SUBMITTED")], Some("r1")),
                            ],
                            None,
                        ),
                    ),
                    ("replies", Some("t1"), Some("r1")) => {
                        replies("t1", page(vec![reply("answer before stall", "SUBMITTED")], Some("r2")))
                    }
                    ("replies", Some("t1"), Some("r2")) | ("reviews", _, None) => pending::<Value>().await,
                    ("replies", Some("t2"), Some("r1")) => {
                        called.fetch_add(1, Ordering::SeqCst);
                        replies("t2", page(vec![reply("too late", "SUBMITTED")], None))
                    }
                    ("comments", _, None) => pr(key, page(vec![], None)),
                    _ => panic!("unexpected request: {body}"),
                }
            }
        })
        .await;
        let out = read_with_short_deadline(&gh).await;
        assert_eq!(out["threads_state"], "incomplete");
        assert_eq!(bodies(&out["threads"][0]["comments"]), ["opening", "answer before stall"]);
        assert_eq!(bodies(&out["threads"][1]["comments"]), ["other opening"]);
        assert_eq!(second_thread_followups.load(Ordering::SeqCst), 0);
        assert_eq!(out["comments_state"], "complete");
        assert_eq!(out["reviews_state"], "unavailable");
    }

    #[tokio::test]
    async fn pages_the_conversation_submitted_review_text_threads_and_replies() {
        let gh = github(|body| {
            let vars = &body["variables"];
            let section = section(&body);
            let after = vars["after"].as_str();
            if section != "replies" {
                assert_eq!(
                    (vars["owner"].as_str(), vars["name"].as_str(), vars["number"].as_u64()),
                    (Some("acme"), Some("api"), Some(42))
                );
            }
            match (section, after) {
                ("comments", None) => pr("comments", page(vec![note("first")], Some("c1"))),
                ("comments", Some("c1")) => pr("comments", page(vec![note("last")], None)),
                ("reviews", None) => {
                    pr("reviews", page(vec![review("draft", "PENDING"), review("", "APPROVED")], Some("v1")))
                }
                ("reviews", Some("v1")) => pr("reviews", page(vec![review("submitted", "CHANGES_REQUESTED")], None)),
                ("reviewThreads", None) => pr(
                    "reviewThreads",
                    page(vec![thread("t1", vec![reply("opening", "SUBMITTED")], Some("r1"))], Some("t1")),
                ),
                ("reviewThreads", Some("t1")) => {
                    pr("reviewThreads", page(vec![thread("t2", vec![reply("another", "SUBMITTED")], None)], None))
                }
                ("replies", Some("r1")) if vars["id"] == "t1" => {
                    replies("t1", page(vec![reply("pending", "PENDING"), reply("answer", "SUBMITTED")], None))
                }
                _ => panic!("unexpected request: {body}"),
            }
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "complete");
        assert_eq!(out["reviews_state"], "complete");
        assert_eq!(out["threads_state"], "complete");
        assert_eq!(bodies(&out["comments"]), ["first", "last"]);
        assert_eq!(bodies(&out["reviews"]), ["submitted"]);
        let first = &out["threads"][0];
        assert_eq!(bodies(&first["comments"]), ["opening", "answer"]);
        assert_eq!(first["comments"][1]["author"], "ana");
        assert_eq!(first["comments"][1]["url"], "https://gh.example/acme/api/pull/42#answer");
        assert_eq!(first["url"], "https://gh.example/acme/api/pull/42#opening");
        assert_eq!(first["line"], 10);
        assert_eq!(first["start_line"], 9);
        assert_eq!(first["path"], "a.rs");
        assert_eq!(first["resolved"], true);
        assert_eq!(first["outdated"], true);
        assert_eq!(first["code"][1]["text"], "ten");
        assert_eq!(bodies(&out["threads"][1]["comments"]), ["another"]);
    }

    #[tokio::test]
    async fn later_errors_keep_earlier_items_and_malformed_sections_do_not_hide_other_sections() {
        let gh = github(|body| match (section(&body), body["variables"]["after"].as_str()) {
            ("comments", None) => pr("comments", page(vec![note("kept")], Some("c1"))),
            ("comments", Some("c1")) => {
                json!({"data": {"repository": {"pullRequest": {"number": 42, "comments": null}}},
                "errors": [{ "message": "secret response", "path": ["repository", "pullRequest", "comments"] }]})
            }
            ("reviews", None) => pr("reviews", json!({"pageInfo": {"hasNextPage": false, "endCursor": null}})),
            ("reviewThreads", None) => {
                pr("reviewThreads", page(vec![thread("t1", vec![reply("still here", "SUBMITTED")], Some("r1"))], None))
            }
            ("replies", Some("r1")) => replies("wrong-thread", page(vec![reply("not this thread", "SUBMITTED")], None)),
            _ => panic!("unexpected request: {body}"),
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(bodies(&out["comments"]), ["kept"]);
        assert_eq!(out["reviews_state"], "unavailable");
        assert_eq!(out["reviews"], json!([]));
        assert_eq!(out["threads_state"], "incomplete");
        assert_eq!(bodies(&out["threads"][0]["comments"]), ["still here"]);
        assert!(!out.to_string().contains("secret response"));
    }

    #[tokio::test]
    async fn a_valid_empty_read_is_complete_in_each_section() {
        let gh = github(|body| pr(section(&body), page(vec![], None))).await;
        let out = read(&gh).await;
        assert_eq!(
            out,
            json!({ "comments": [], "reviews": [], "threads": [],
            "comments_state": "complete", "reviews_state": "complete", "threads_state": "complete" })
        );
    }

    #[tokio::test]
    async fn an_empty_connection_with_missing_page_info_fields_is_unavailable() {
        let gh = github(|body| {
            let key = section(&body);
            if key == "comments" {
                return pr("comments", json!({"pageInfo": {"hasNextPage": false}, "nodes": []}));
            }
            pr(key, page(vec![], None))
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "unavailable");
        assert_eq!(out["reviews_state"], "complete");
        assert_eq!(out["threads_state"], "complete");
    }

    #[tokio::test]
    async fn a_top_level_page_cap_keeps_values_and_marks_the_section_incomplete() {
        let gh = github(|body| {
            let key = section(&body);
            if key != "comments" {
                return pr(key, page(vec![], None));
            }
            let next = body["variables"]["after"].as_str().map_or(1, |n| n.parse::<usize>().unwrap() + 1);
            pr(key, page(vec![note(&next.to_string())], Some(&next.to_string())))
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(bodies(&out["comments"]), ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
        assert_eq!(out["reviews_state"], "complete");
    }

    #[tokio::test]
    async fn many_long_threads_cannot_each_spend_the_whole_reply_budget() {
        let called = Arc::new(AtomicUsize::new(0));
        let calls = called.clone();
        let gh = github(move |body| match section(&body) {
            "reviewThreads" => pr(
                "reviewThreads",
                page(
                    (0..6).map(|i| thread(&i.to_string(), vec![reply("opening", "SUBMITTED")], Some("1"))).collect(),
                    None,
                ),
            ),
            "replies" => {
                calls.fetch_add(1, Ordering::SeqCst);
                let vars = &body["variables"];
                let n = vars["after"].as_str().unwrap().parse::<usize>().unwrap();
                replies(
                    vars["id"].as_str().unwrap(),
                    page(vec![reply("retained", "SUBMITTED")], Some(&(n + 1).to_string())),
                )
            }
            other => pr(other, page(vec![], None)),
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(called.load(Ordering::SeqCst), 40);
        assert_eq!(out["threads_state"], "incomplete");
        assert_eq!(out["threads"].as_array().unwrap().len(), 6);
        assert_eq!(bodies(&out["threads"][0]["comments"]).len(), 10);
        assert_eq!(bodies(&out["threads"][5]["comments"]), ["opening"]);
    }

    #[tokio::test]
    async fn valid_nodes_survive_a_partial_graphql_page_without_looking_complete() {
        let gh = github(|body| {
            let key = section(&body);
            if key != "comments" {
                return pr(key, page(vec![], None));
            }
            let mut out = pr(key, page(vec![note("kept"), Value::Null], None));
            out["errors"] = json!([{ "message": "secret response", "path": ["repository", "pullRequest", "comments", "nodes", 1] }]);
            out
        }).await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(bodies(&out["comments"]), ["kept"]);
        assert_eq!(out["reviews_state"], "complete");
        assert!(!out.to_string().contains("secret response"));
    }

    #[tokio::test]
    async fn a_cyclic_cursor_stops_with_earlier_pages_and_a_missing_pr_is_not_an_empty_read() {
        let gh = github(|body| {
            let key = section(&body);
            match (key, body["variables"]["after"].as_str()) {
                ("comments", None) => pr(key, page(vec![note("first")], Some("a"))),
                ("comments", Some("a")) => pr(key, page(vec![note("second")], Some("b"))),
                ("comments", Some("b")) => pr(key, page(vec![note("third")], Some("a"))),
                ("reviews", None) => json!({ "data": { "repository": { "pullRequest": null } } }),
                ("reviewThreads", None) => pr(key, page(vec![], None)),
                _ => panic!("unexpected request: {body}"),
            }
        })
        .await;
        let out = read(&gh).await;
        assert_eq!(out["comments_state"], "incomplete");
        assert_eq!(bodies(&out["comments"]), ["first", "second", "third"]);
        assert_eq!(out["reviews_state"], "unavailable");
        assert_eq!(out["threads_state"], "complete");
    }
}
