//! Linking a ticket that already exists to a Work task that has none
//! (TASK-21, TKT-23), from the task's header or by an agent through
//! `task_link_ticket`. Start work makes a new task; this keeps the task,
//! its checkouts and its spec, and only records the ticket and tells the
//! task's agents about it.
//!
//! The ticket is looked up without holding the config lock, so the tracker
//! it was found on is checked again before the link is saved: a key such as
//! `API-42` can name a different ticket on another Jira site or GitHub
//! source.

use std::sync::LazyLock;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::config::{AppConfig, Task, TrackerKind};
use crate::error::{Error, Result};
use crate::tickets::github::{in_workspace, Issues};
use crate::tickets::{Find, Ticket, TicketPage, Tracker};

use super::task_context::write_ticket_context;
use super::AppState;

#[derive(Debug, Serialize)]
pub struct TicketSearch {
    #[serde(flatten)]
    pub(super) page: TicketPage,
    /// Only for the Link ticket picker: which tracker its results came from,
    /// for `link_task_ticket` to check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tracker_scope: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LinkedTaskTicket {
    pub task: Task,
    pub ticket: Ticket,
    pub context_warning: Option<String>,
}

/// What the Work header shows of a ticket just linked, which may not be on
/// your own list (done, or someone else's).
#[derive(Clone, Serialize)]
struct TaskTicketLinked<'a> {
    task_id: &'a str,
    key: &'a str,
    url: &'a str,
    status: &'a str,
    status_category: &'a str,
    context_warning: Option<&'a str>,
}

/// Link an existing ticket to this task without replaying Start work.
#[tauri::command]
pub async fn link_task_ticket(
    app: AppHandle,
    task_id: String,
    key: String,
    tracker_scope: String,
) -> Result<LinkedTaskTicket> {
    link_with(app, task_id, key, Some(tracker_scope), Tracker::connect).await
}

/// The same for an agent (`task_link_ticket`), behind the MCP confirm gate.
pub(crate) async fn link_task_ticket_inner(app: AppHandle, task_id: String, key: String) -> Result<LinkedTaskTicket> {
    link_with(app, task_id, key, None, Tracker::connect).await
}

/// A search for the picker, with the scope its results belong to.
pub(super) async fn find_for_link<R: tauri::Runtime>(
    app: AppHandle<R>,
    find: Find,
    connect: impl FnOnce(&AppConfig) -> Result<Tracker> + Send + 'static,
) -> Result<TicketSearch> {
    let (tracker, scope) =
        super::blocking(app.clone(), move |state| with_scope(state, |cfg, scope| Ok((connect(cfg)?, scope)))).await?;
    let page = tracker.find(find).await?;
    super::blocking(app, move |state| {
        with_scope(state, |_, current| {
            if current != scope {
                return Err(picker_connection_changed());
            }
            Ok(TicketSearch { page, tracker_scope: Some(scope.token()?) })
        })
    })
    .await
}

async fn link_with<R: tauri::Runtime>(
    app: AppHandle<R>,
    task_id: String,
    key: String,
    expected_scope: Option<String>,
    connect: impl FnOnce(&AppConfig) -> Result<Tracker> + Send + 'static,
) -> Result<LinkedTaskTicket> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(Error::Other("Choose a ticket to link.".into()));
    }
    let (tracker, scope, task_id, key) = super::blocking(app.clone(), move |state| {
        with_scope(state, |c, scope| {
            linkable_task(c, &task_id)?;
            if let Some(expected) = expected_scope.as_deref() {
                if expected != scope.token()? {
                    return Err(picker_connection_changed());
                }
            }
            let tracker = connect(c)?;
            if let Tracker::Github(_, issues) = &tracker {
                in_workspace(issues, &key)?;
            }
            Ok((tracker, scope, task_id, key))
        })
    })
    .await?;
    let ticket = tracker.ticket(&key).await?;
    if ticket.key.trim().is_empty() || ticket.url.trim().is_empty() {
        return Err(Error::Other("The tracker did not return a ticket key and URL. Try again.".into()));
    }

    let linked = super::blocking(app.clone(), move |state| {
        // The lookup did not hold the config lock. Check again before claiming
        // the ticket, so two calls cannot both claim it or replace a link.
        let task = state.config.update(|c| {
            if tracker_identity(c).as_ref() != Some(&scope.identity) {
                return Err(tracker_changed());
            }
            if scope.identity.0 == TrackerKind::Github {
                let issues = Issues::of(c, &scope.identity.1[2]);
                in_workspace(&issues, &ticket.key)?;
            }
            let at = linkable_task(c, &task_id)?;
            if let Some(owner) = c.tasks.iter().find(|t| claims(t, &ticket.key)) {
                return Err(Error::Other(format!("{} is already linked to the task \"{}\".", ticket.key, owner.name)));
            }
            let task = &mut c.tasks[at];
            task.issue_key = Some(ticket.key.clone());
            task.issue_url = Some(ticket.url.clone());
            Ok(task.clone())
        })??;
        let context_warning = write_ticket_context(state, &task, &ticket)
            .err()
            .map(|e| format!("The ticket is linked, but Layer could not refresh the agent context: {e}"));
        Ok(LinkedTaskTicket { task, ticket, context_warning })
    })
    .await?;
    // Both ways in report the saved link, even when writing context failed.
    let _ = app.emit(
        "task:ticket-linked",
        TaskTicketLinked {
            task_id: &linked.task.id,
            key: &linked.ticket.key,
            url: &linked.ticket.url,
            status: &linked.ticket.status,
            status_category: &linked.ticket.status_category,
            context_warning: linked.context_warning.as_deref(),
        },
    );
    Ok(linked)
}

fn claims(task: &Task, key: &str) -> bool {
    task.issue_key.as_deref().is_some_and(|saved| saved.eq_ignore_ascii_case(key))
}

fn tracker_changed() -> Error {
    Error::Other("The tracker connection changed during lookup. Try again.".into())
}

fn picker_connection_changed() -> Error {
    Error::Other("The tracker connection changed. Search for the ticket again.".into())
}

/// The tracker a lookup was made against.
#[derive(PartialEq, Serialize)]
struct TrackerScope {
    identity: (TrackerKind, Vec<String>),
}

impl TrackerScope {
    fn token(&self) -> Result<String> {
        // The webview echoes this process's proof, not a site or account it
        // chose. Only the identity is hashed, never a credential.
        static NONCE: LazyLock<[u8; 16]> = LazyLock::new(|| *uuid::Uuid::new_v4().as_bytes());
        let mut hash = Sha256::new();
        hash.update(*NONCE);
        hash.update(serde_json::to_vec(self)?);
        Ok(hex::encode(hash.finalize()))
    }
}

fn with_scope<T>(state: &AppState, read: impl FnOnce(&AppConfig, TrackerScope) -> Result<T>) -> Result<T> {
    state.config.with(|c| {
        let identity = tracker_identity(c).ok_or(Error::NotConfigured("Ticket tracker"))?;
        read(c, TrackerScope { identity })
    })
}

/// The site and account a ticket is read from. Search preferences and
/// workflow rules can change without making a lookup stale; another Jira
/// site or account, another GitHub Issues source, or a reconnected token
/// (a new `revision`) do.
fn tracker_identity(c: &AppConfig) -> Option<(TrackerKind, Vec<String>)> {
    let kind = c.tracker_in_use()?;
    match kind {
        TrackerKind::Jira => c.jira.as_ref().map(|j| (kind, vec![j.base_url.clone(), j.email.clone()])),
        TrackerKind::Github => c.github_source(c.github_issue_host()?).map(|g| {
            let revision = g.revision.map(|r| r.to_string()).unwrap_or_default();
            (kind, vec![g.key, g.config.api_url, g.config.web_url, revision])
        }),
    }
}

fn linkable_task(c: &AppConfig, id: &str) -> Result<usize> {
    let at = c.tasks.iter().position(|t| t.id == id).ok_or_else(|| Error::NotFound(format!("task {id}")))?;
    let task = &c.tasks[at];
    if task.epic || task.review.is_some() || task.sub_task.is_some() {
        return Err(Error::Other("Only regular Work tasks can link a ticket.".into()));
    }
    if task.issue_key.is_some() || task.issue_url.is_some() {
        return Err(Error::Other("This task already has a ticket. Open its linked ticket instead.".into()));
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JiraConfig;
    use crate::integrations::jira::Jira;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::test::{mock_app, MockRuntime};
    use tauri::Manager;
    use tokio::net::TcpListener;

    use super::super::repos::tests::{add_task, setup, state};

    struct Fixture {
        app: tauri::App<MockRuntime>,
        root: PathBuf,
        listener: Option<TcpListener>,
        url: String,
    }

    impl Fixture {
        async fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (root, mut cfg) = setup();
            cfg.tracker = Some(TrackerKind::Jira);
            cfg.jira =
                Some(JiraConfig { base_url: url.clone(), email: "dev@example.test".into(), ..Default::default() });
            add_task(&mut cfg, &root, "first");
            add_task(&mut cfg, &root, "second");
            for task in &cfg.tasks {
                std::fs::create_dir_all(&task.root).unwrap();
            }
            let app = mock_app();
            app.manage(state(&root, cfg));
            Self { app, root, listener: Some(listener), url }
        }

        fn serve(&mut self, router: Router) {
            let listener = self.listener.take().unwrap();
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        }

        fn serve_jira(&mut self) {
            self.serve(Router::new().route("/rest/api/3/issue/{key}", get(|| async { jira_answer() })));
        }

        fn handle(&self) -> AppHandle<MockRuntime> {
            self.app.handle().clone()
        }

        fn state(&self) -> tauri::State<'_, AppState> {
            self.app.state()
        }

        async fn link(&self, id: &str, key: &str) -> Result<LinkedTaskTicket> {
            link_with(self.handle(), id.into(), key.into(), None, connect).await
        }

        fn task_root(&self, id: &str) -> PathBuf {
            PathBuf::from(self.state().config.task(id).unwrap().root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    // The real Jira client and its decoding; only the token and the site are
    // stand-ins.
    fn connect(cfg: &AppConfig) -> Result<Tracker> {
        let jira = cfg.jira.clone().unwrap();
        Ok(Tracker::Jira(Jira::new(&jira, "test-token")?, jira))
    }

    fn jira_answer() -> Json<Value> {
        Json(json!({
            "key": "API-42",
            "fields": {
                "summary": "Repair the API",
                "description": "Someone else's ticket",
                "assignee": { "displayName": "Ada" },
                "status": { "name": "To Do", "id": "11", "statusCategory": { "key": "new" } },
                "issuetype": { "name": "Bug" }
            }
        }))
    }

    fn picker_find() -> Find {
        Find {
            text: Some("API-42".into()),
            whose: "anyone".into(),
            include_done: true,
            types: vec![],
            order: String::new(),
        }
    }

    #[tokio::test]
    async fn linking_saves_the_canonical_ticket_and_tells_the_tasks_agents() {
        let mut f = Fixture::new().await;
        f.serve_jira();

        let linked = f.link("first", " api-42 ").await.unwrap();

        let url = format!("{}/browse/API-42", f.url);
        assert_eq!(linked.ticket.key, "API-42");
        assert!(linked.context_warning.is_none());
        let saved = f.state().config.task("first").unwrap();
        assert_eq!(saved.issue_key.as_deref(), Some("API-42"));
        assert_eq!(saved.issue_url.as_deref(), Some(url.as_str()));
        assert_eq!(f.state().config.checkouts_of("first").len(), 1, "the task keeps its checkout");
        let root = f.task_root("first");
        assert!(std::fs::read_to_string(root.join("TICKET.md")).unwrap().contains("API-42 Repair the API"));
        for file in ["AGENTS.md", "CLAUDE.md"] {
            let context = std::fs::read_to_string(root.join(file)).unwrap();
            assert!(context.contains("API-42 Repair the API"));
            assert!(!context.contains("## Link existing work"), "a linked task is not told to link again");
        }
    }

    #[tokio::test]
    async fn linking_refuses_a_task_that_already_has_a_ticket_and_leaves_its_context() {
        let mut f = Fixture::new().await;
        f.serve_jira();
        f.state()
            .config
            .update(|c| {
                c.tasks[0].issue_key = Some("API-9".into());
                c.tasks[0].issue_url = Some("https://example.test/browse/API-9".into());
            })
            .unwrap();
        let path = f.task_root("first").join("TICKET.md");
        std::fs::write(&path, "previously linked").unwrap();

        let err = f.link("first", "API-42").await.unwrap_err();

        assert!(err.to_string().contains("already has a ticket"), "{err}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "previously linked");
        assert_eq!(f.state().config.task("first").unwrap().issue_key.as_deref(), Some("API-9"));
    }

    #[tokio::test]
    async fn linking_refuses_a_ticket_another_task_already_has() {
        let mut f = Fixture::new().await;
        f.serve_jira();
        f.state().config.update(|c| c.tasks[1].issue_key = Some("api-42".into())).unwrap();

        let err = f.link("first", "API-42").await.unwrap_err();

        assert!(err.to_string().contains("already linked to the task \"second\""), "{err}");
        assert!(f.state().config.task("first").unwrap().issue_key.is_none());
    }

    #[tokio::test]
    async fn an_epic_a_review_or_a_sub_task_cannot_link_a_ticket() {
        let f = Fixture::new().await;
        f.state().config.update(|c| c.tasks[0].epic = true).unwrap();

        let err = f.link("first", "API-42").await.unwrap_err();

        assert!(err.to_string().contains("Only regular Work tasks"), "{err}");
    }

    #[tokio::test]
    async fn a_failed_lookup_saves_no_link_and_writes_no_context() {
        let mut f = Fixture::new().await;
        f.serve(Router::new());

        assert!(f.link("first", "API-42").await.is_err());

        assert!(f.state().config.task("first").unwrap().issue_key.is_none());
        assert!(!f.task_root("first").join("TICKET.md").exists());
    }

    #[tokio::test]
    async fn a_picker_result_from_another_jira_site_cannot_be_linked() {
        let mut f = Fixture::new().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        f.serve(
            Router::new()
                .route("/rest/api/3/search/jql", post(|| async { Json(json!({ "issues": [jira_answer().0] })) }))
                .route(
                    "/other/rest/api/3/issue/API-42",
                    get(move || {
                        let seen = seen.clone();
                        async move {
                            seen.fetch_add(1, Ordering::SeqCst);
                            jira_answer()
                        }
                    }),
                ),
        );
        let page = find_for_link(f.handle(), picker_find(), connect).await.unwrap();
        assert_eq!(page.page.issues[0].url, format!("{}/browse/API-42", f.url));
        f.state().config.update(|c| c.jira.as_mut().unwrap().base_url = format!("{}/other", f.url)).unwrap();

        let result = link_with(f.handle(), "first".into(), "API-42".into(), page.tracker_scope, connect).await;

        assert!(result.unwrap_err().to_string().contains("connection changed"));
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the new site is not asked about the old result");
        assert!(f.state().config.task("first").unwrap().issue_key.is_none());
    }

    #[tokio::test]
    async fn a_picker_search_answered_after_the_connection_changed_is_not_returned() {
        let mut f = Fixture::new().await;
        let app = f.handle();
        f.serve(Router::new().route(
            "/rest/api/3/search/jql",
            post(move || {
                let app = app.clone();
                async move {
                    app.state::<AppState>()
                        .config
                        .update(|c| c.jira.as_mut().unwrap().email = "new@example.test".into())
                        .unwrap();
                    Json(json!({ "issues": [jira_answer().0] }))
                }
            }),
        ));

        let result = find_for_link(f.handle(), picker_find(), connect).await;

        assert!(result.unwrap_err().to_string().contains("connection changed"));
    }

    #[test]
    fn a_github_lookup_is_stale_once_the_issue_source_changes() {
        let mut cfg: AppConfig = serde_json::from_value(json!({
            "tracker": "github",
            "github_sources": {
                "legacy_host": "github.com",
                "shared": [
                    {"api_url": "https://api.github.com", "web_url": "https://github.com"},
                    {"api_url": "https://git.example.com/api/v3", "web_url": "https://git.example.com"}
                ]
            }
        }))
        .unwrap();
        let first = tracker_identity(&cfg).unwrap();
        assert_eq!(first.1[0], "github.com");
        cfg.github_issue_source = Some("git.example.com".into());
        let other = tracker_identity(&cfg).unwrap();
        assert_ne!(first, other);
        cfg.tracker = Some(TrackerKind::Jira);
        assert!(tracker_identity(&cfg).is_none(), "Jira without a site is no tracker to link from");
    }
}
