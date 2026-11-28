//! Banners for a review requested or a ticket assigned, while the window is
//! in the background.
//!
//! These were the webview's, on a timer it ran for them — and a hidden
//! webview is the one whose timers macOS throttles or stops, so the banners
//! were least reliable exactly when they were the point. The backend now
//! keeps what has been seen, from every fetch the UI makes and from one of
//! its own while the window is away, and says what is new.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::commands::{banner, blocking, guarded_banner, AppState, ReviewQueue, SourceReviews, SubmitBanner};
use crate::config::github::GithubSource;
use crate::error::Result;
use crate::messages::{self, Kind, Level, New};
use crate::target::Target;

/// How often to look while the window is away. The same three minutes the
/// webview used.
const EVERY: Duration = Duration::from_secs(180);
/// Past this many at once, one banner rather than a stack of them.
const BATCH: usize = 3;

/// What has been seen, so only what is new is news. None until the first
/// snapshot, which is recorded and not announced: opening the app would
/// otherwise banner every pull request and ticket already waiting.
#[derive(Default)]
pub struct Seen {
    reviews: parking_lot::Mutex<HashMap<String, SeenReviewSource>>,
    tickets: parking_lot::Mutex<Option<HashSet<String>>>,
}

#[derive(Default)]
struct SeenReviewSource {
    revision: Option<uuid::Uuid>,
    mine: Option<HashSet<String>>,
    team: Option<HashSet<String>>,
}

fn identity(source: &str, pr: &crate::integrations::github::ReviewRequest) -> String {
    let repo = pr.repo.to_ascii_lowercase();
    if source.is_empty() {
        format!("{repo}#{}", pr.number)
    } else {
        format!("{source}/{repo}#{}", pr.number)
    }
}

impl Seen {
    /// The pull requests in `queue` not seen before, remembering them.
    ///
    /// A team lookup that failed comes back with nothing in it. Forgetting
    /// what it held would make the next success announce all of it again.
    ///
    /// A result already on the wire when a source was disconnected must not
    /// announce old information about that account, so only sources still
    /// connected, at the revision that was read, count.
    fn reviews_now(&self, queue: &ReviewQueue, now: &[GithubSource]) -> Vec<(String, String)> {
        let mut seen = self.reviews.lock();
        queue
            .sources
            .iter()
            .filter(|q| now.iter().any(|g| g.key == q.source && g.status == "connected" && g.revision == q.revision))
            .flat_map(|q| seen.entry(q.source.clone()).or_default().seen(q))
            .collect()
    }

    #[cfg(test)]
    fn reviews(&self, queue: &ReviewQueue) -> Vec<(String, String)> {
        let mut seen = self.reviews.lock();
        queue.sources.iter().flat_map(|q| seen.entry(q.source.clone()).or_default().seen(q)).collect()
    }

    /// The tickets in `issues` not seen before, remembering them.
    fn tickets(&self, issues: &[crate::tickets::Ticket]) -> Vec<(String, String)> {
        let mut seen = self.tickets.lock();
        let fresh = match seen.as_ref() {
            Some(prev) => {
                issues.iter().filter(|i| !prev.contains(&i.key)).map(|i| (i.key.clone(), i.summary.clone())).collect()
            }
            None => Vec::new(),
        };
        *seen = Some(issues.iter().map(|i| i.key.clone()).collect());
        fresh
    }

    /// Start again from a snapshot: the connection changed, and the old
    /// list says nothing about the new one.
    ///
    /// Not when banners are turned off, which it once was: the message
    /// center keeps these either way (MSG-2), and a snapshot every three
    /// minutes made nearly every new ticket part of one.
    pub fn forget(&self) {
        self.forget_reviews();
        self.forget_tickets();
    }

    pub(crate) fn forget_reviews(&self) {
        self.reviews.lock().clear();
    }

    /// Each source's next team list is a snapshot again.
    pub(crate) fn forget_team(&self) {
        for source in self.reviews.lock().values_mut() {
            source.team = None;
        }
    }

    pub(crate) fn forget_tickets(&self) {
        *self.tickets.lock() = None;
    }
}

impl SeenReviewSource {
    fn seen(&mut self, queue: &SourceReviews) -> Vec<(String, String)> {
        if self.revision != queue.revision {
            *self = Self { revision: queue.revision, ..Self::default() };
        }
        let key = |pr: &crate::integrations::github::ReviewRequest| identity(&queue.source, pr);
        let mut known: HashSet<String> = self.mine.iter().chain(self.team.iter()).flatten().cloned().collect();
        let mut fresh = Vec::new();
        let mut read = |before: &mut Option<HashSet<String>>, prs: &[crate::integrations::github::ReviewRequest]| {
            let primed = before.is_some();
            *before = Some(prs.iter().map(&key).collect());
            for pr in prs {
                let id = key(pr);
                if known.insert(id.clone()) && primed {
                    fresh.push((id, pr.title.clone()));
                }
            }
        };
        if queue.mine_error.is_none() {
            read(&mut self.mine, &queue.mine);
        }
        match &queue.team {
            Some(team) if team.error.is_none() => read(&mut self.team, &team.prs),
            None if queue.mine_error.is_none() => self.team = None,
            _ => {}
        }
        fresh
    }
}

/// Banners are on, and nobody is looking at the window.
fn announce_now<R: Runtime>(app: &AppHandle<R>) -> bool {
    let state = app.state::<AppState>();
    state.config.with(|c| c.ui.system_notifications)
        && app.get_webview_window("main").is_some_and(|w| !w.is_focused().unwrap_or(false))
}

/// A review queue has come in, from wherever it was asked for.
pub(crate) async fn saw_reviews<R: Runtime>(
    app: AppHandle<R>,
    generation: u64,
    queue: ReviewQueue,
) -> Result<ReviewQueue> {
    saw_reviews_with(app, generation, queue, |handle, submit, news| {
        if announce_now(handle) {
            for (title, body, target) in
                banners(news, "Reviews", "pull requests are waiting on a review", Target::Reviews)
            {
                let _ = guarded_banner(handle, title, body, target, submit.clone());
            }
        }
    })
    .await
}

pub(crate) async fn saw_reviews_with<R: Runtime>(
    app: AppHandle<R>,
    generation: u64,
    queue: ReviewQueue,
    notify: impl FnOnce(&AppHandle<R>, SubmitBanner, Vec<New>) + Send + 'static,
) -> Result<ReviewQueue> {
    let handle = app.clone();
    blocking(app, move |state| {
        let session = state.github_session.blocking_lock();
        if session.reviews_generation != generation {
            return Err(crate::error::Error::Other(
                "The GitHub review connection changed. Refresh Reviews and try again.".into(),
            ));
        }
        let now = state.config.with(|c| c.github_source_list());
        let news = review_news(&state.news.reviews_now(&queue, &now));
        messages::record_all(&handle, news.clone());
        // Authorization can call back inline. Do not keep the state gate
        // while asking for delivery; take it again only for the actual post.
        drop(session);
        if !news.is_empty() {
            let session = state.github_session.clone();
            let submit: SubmitBanner = Arc::new(move |post| {
                let session = session.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let session = session.blocking_lock();
                    if session.reviews_generation == generation {
                        post();
                    }
                })
            });
            notify(&handle, submit, news);
        }
        Ok(queue)
    })
    .await
}

/// Your ticket list has come in, from wherever it was asked for.
pub fn saw_tickets(app: &AppHandle, issues: &[crate::tickets::Ticket]) {
    let fresh = app.state::<AppState>().news.tickets(issues);
    let news = ticket_news(&fresh);
    messages::record_all(app, news.clone());
    if news.is_empty() || !announce_now(app) {
        return;
    }
    for (title, body, target) in banners(news, "Tickets", "tickets were assigned to you", Target::Tickets) {
        let _ = banner(app, title, body, target);
    }
}

/// New review requests, as the message center keeps them.
fn review_news(fresh: &[(String, String)]) -> Vec<New> {
    fresh
        .iter()
        .map(|(pr, title)| New {
            kind: Kind::Review,
            level: Level::Info,
            title: "Review requested".into(),
            body: format!("{pr} — {title}"),
            target: Some(Target::Review(pr).to_string()),
        })
        .collect()
}

/// New tickets, as the message center keeps them.
fn ticket_news(fresh: &[(String, String)]) -> Vec<New> {
    fresh
        .iter()
        .map(|(key, summary)| New {
            kind: Kind::Ticket,
            level: Level::Info,
            title: "New ticket".into(),
            body: format!("{key} — {summary}"),
            target: Some(Target::Ticket(key).to_string()),
        })
        .collect()
}

/// A banner each, or one for the lot past `BATCH`: title, body and what a
/// click opens.
fn banners(news: Vec<New>, heading: &str, many: &str, all: Target) -> Vec<(String, String, String)> {
    if news.len() > BATCH {
        return vec![(heading.into(), format!("{} {many}", news.len()), all.to_string())];
    }
    news.into_iter().map(|n| (n.title, n.body, n.target.unwrap_or_else(|| all.to_string()))).collect()
}

/// Look for news while the window is away, for as long as the app runs.
///
/// While it is in front the UI's own fetches keep `Seen` current, and
/// nothing needs saying.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(EVERY).await;
            let state = app.state::<AppState>();
            // Only while away with banners on: in front, the UI's own
            // fetches keep `Seen` current and record what is new.
            if !announce_now(&app) {
                continue;
            }
            let (github, tickets) = {
                let cfg = state.config.read();
                (cfg.github_any_connected(), cfg.tracker_in_use().is_some())
            };
            if github {
                let gate = crate::commands::github_gate(&state).await;
                if let Ok(queue) = crate::commands::review_queue(&state, &gate, false).await {
                    let _ = saw_reviews(app.clone(), gate.reviews_generation, queue).await;
                }
            }
            if tickets {
                if let Ok(page) = crate::commands::my_tickets(&state).await {
                    saw_tickets(&app, &page.issues);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::TeamReviews;
    use crate::integrations::github::ReviewRequest;

    fn pr(repo: &str, number: u64) -> ReviewRequest {
        ReviewRequest { repo: repo.into(), number, title: format!("t{number}"), ..Default::default() }
    }

    fn queue(mine: Vec<ReviewRequest>, team: Option<(Vec<ReviewRequest>, bool)>) -> ReviewQueue {
        ReviewQueue {
            sources: vec![SourceReviews {
                source: String::new(),
                revision: None,
                mine,
                mine_more: false,
                mine_error: None,
                team: team.map(|(prs, failed)| TeamReviews {
                    slug: "o/t".into(),
                    name: "t".into(),
                    prs,
                    more: false,
                    error: failed.then(|| "boom".into()),
                }),
                authored: None,
            }],
        }
    }

    #[test]
    fn the_first_look_is_a_snapshot_and_after_it_only_new_is_news() {
        let seen = Seen::default();
        assert!(seen.reviews(&queue(vec![pr("a", 1)], None)).is_empty());
        assert!(seen.reviews(&queue(vec![pr("a", 1)], None)).is_empty());
        let fresh = seen.reviews(&queue(vec![pr("a", 1), pr("a", 2)], None));
        assert_eq!(fresh, [("a#2".to_string(), "t2".to_string())]);
    }

    #[test]
    fn a_team_list_arriving_later_is_a_snapshot_and_a_failed_one_forgets_nothing() {
        let seen = Seen::default();
        seen.reviews(&queue(vec![], None));
        // The team is configured after the first look: its list is not news.
        assert!(seen.reviews(&queue(vec![], Some((vec![pr("b", 5)], false)))).is_empty());
        // A failed lookup keeps what was known ...
        assert!(seen.reviews(&queue(vec![], Some((vec![], true)))).is_empty());
        // ... so the next success does not announce it all again.
        let fresh = seen.reviews(&queue(vec![], Some((vec![pr("b", 5), pr("b", 6)], false))));
        assert_eq!(fresh.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["b#6"]);
    }

    #[test]
    fn two_review_hosts_keep_independent_baselines_when_one_is_down() {
        let seen = Seen::default();
        let got = |source: &str, numbers: &[u64], error: bool| {
            let mut source_queue = queue(numbers.iter().map(|&n| pr("acme/web", n)).collect(), None);
            source_queue.sources[0].source = source.into();
            source_queue.sources[0].mine_error = error.then(|| "unavailable".into());
            source_queue.sources.remove(0)
        };
        let both = |left, right| ReviewQueue { sources: vec![left, right] };
        assert!(seen.reviews(&both(got("github.com", &[7], false), got("ghe.test", &[7], false))).is_empty());
        let changed = seen.reviews(&both(got("github.com", &[7, 8], false), got("ghe.test", &[], true)));
        assert_eq!(changed, [("github.com/acme/web#8".into(), "t8".into())]);
        let recovered = seen.reviews(&both(got("github.com", &[7, 8], false), got("ghe.test", &[7, 8], false)));
        assert_eq!(recovered, [("ghe.test/acme/web#8".into(), "t8".into())]);
    }

    #[test]
    fn a_ticket_is_news_once() {
        let seen = Seen::default();
        let issue =
            |key: &str| crate::tickets::Ticket { key: key.into(), summary: format!("s {key}"), ..Default::default() };
        assert!(seen.tickets(&[issue("ACME-1")]).is_empty());
        assert_eq!(seen.tickets(&[issue("ACME-1"), issue("ACME-2")]).len(), 1);
        assert!(seen.tickets(&[issue("ACME-1"), issue("ACME-2")]).is_empty());
        seen.forget();
        assert!(seen.tickets(&[issue("ACME-3")]).is_empty(), "after forgetting, a snapshot again");
    }

    #[test]
    fn a_new_review_opens_that_pull_request() {
        let say = |fresh: &[(String, String)]| banners(review_news(fresh), "Reviews", "waiting", Target::Reviews);
        let one = [("o/a#7".to_string(), "Fix it".to_string())];
        assert_eq!(say(&one), [("Review requested".into(), "o/a#7 — Fix it".into(), "review:o/a#7".into())]);
        let many: Vec<_> = (0..4).map(|i| (format!("o/a#{i}"), String::new())).collect();
        assert_eq!(say(&many)[0].2, "reviews");
        let ticket = [("ACME-3".to_string(), "Do it".to_string())];
        assert_eq!(ticket_news(&ticket)[0].target.as_deref(), Some("ticket:ACME-3"));
    }

    #[test]
    fn every_new_review_is_kept_even_when_its_banner_is_one_of_many() {
        let many: Vec<_> = (0..5).map(|i| (format!("o/a#{i}"), String::new())).collect();
        assert_eq!(review_news(&many).len(), 5);
    }
}
