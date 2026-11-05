//! The sign-in window (BRW-23): the browser's profile in an ordinary Chrome
//! window, for a site that signs no one in to a browser a program drives.
//!
//! Google refuses ("This browser or app may not be secure"), and ↗ to the
//! user's own browser did not help: the rejected page's address is good
//! only in the session that asked, and a sign-in there never reaches the
//! app's profile. So the app steps its own browser aside, and the user
//! signs in to the same profile in a Chrome nothing drives.

use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use super::{changed, chrome, sites, Browser};
use crate::commands::AppState;
use crate::error::{Error, Result};

/// What the panel and agents are told while the window is open.
pub const WAITING: &str =
    "The browser is paused while you sign in in the Chrome window. Quit that Chrome (⌘Q) when you are done.";

/// The page a sign-in at `url` was for: Google's `continue` address on its
/// sign-in pages, else the page itself. None for a page that is not on the
/// web.
pub fn page_for(url: &str) -> Option<String> {
    let host = sites::host_of(url)?;
    if host == "accounts.google.com" {
        let query = url.split_once('?').map(|(_, q)| q.split('#').next().unwrap_or(q)).unwrap_or("");
        let next = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| *k == "continue")
            .map(|(_, v)| decode(v))
            .filter(|v| sites::host_of(v).is_some());
        if let Some(next) = next {
            return Some(next);
        }
    }
    Some(url.to_string())
}

/// A query value, percent-decoded.
fn decode(v: &str) -> String {
    let bytes = v.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Browser {
    /// Whether the sign-in window is open, and the browser waits for it.
    pub fn signing_in(&self) -> bool {
        self.inner.lock().window.is_some()
    }

    /// Step the browser aside and open the sign-in window on the page the
    /// task's tab was signing in to; the tab opens it when the window quits.
    pub async fn sign_in_window<R: Runtime>(&self, app: &AppHandle<R>, task: &str) -> Result<()> {
        let binary = chrome::find().ok_or_else(|| Error::Other(chrome::MISSING.into()))?;
        let url = self.state_of(task).map(|(url, _, _)| url).unwrap_or_default();
        let page = page_for(&url).ok_or_else(|| Error::Other("this tab has no page to sign in to".into()))?;
        let _making = self.making.lock().await;
        {
            let mut inner = self.inner.lock();
            if inner.window.is_some() {
                return Err(Error::Other(WAITING.into()));
            }
            // Before the browser goes: a panel or agent asking for it in
            // between would start it again on the profile the window needs.
            inner.window = Some(Window { pid: None });
        }
        let opened = self.open_window(app, task, &binary, &page).await;
        if opened.is_err() {
            self.inner.lock().window = None;
        }
        changed(app, "");
        opened
    }

    async fn open_window<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &str,
        binary: &std::path::Path,
        page: &str,
    ) -> Result<()> {
        // The tabs as they are, with the active one on the page signed in
        // to: the browser opens them when it starts again (BRW-6).
        let (saving, task_id, page_url) = (app.clone(), task.to_string(), page.to_string());
        crate::commands::off_runtime(move || {
            let browser = &saving.state::<AppState>().browser;
            browser.save(&saving, &task_id);
            browser.point_saved_at(&saving, &task_id, &page_url);
        })
        .await?;

        let closing = app.clone();
        crate::commands::off_runtime(move || closing.state::<AppState>().browser.shutdown(Duration::from_secs(3)))
            .await?;
        {
            let mut inner = self.inner.lock();
            inner.tabs.clear();
            inner.heard.clear();
            inner.watch = None;
        }

        let profile = app.state::<AppState>().config.folder().join("browser");
        let (binary, page) = (binary.to_path_buf(), page.to_string());
        let mut process =
            crate::commands::off_runtime(move || chrome::launch_window(&binary, &profile, &page)).await??;
        if let Some(w) = self.inner.lock().window.as_mut() {
            w.pid = Some(process.id());
        }
        let app = app.clone();
        std::thread::spawn(move || {
            process.wait();
            app.state::<AppState>().browser.inner.lock().window = None;
            changed(&app, "");
        });
        Ok(())
    }

    /// Done, in the panel: quit the sign-in window as ⌘Q would.
    pub fn sign_in_done(&self) {
        if let Some(pid) = self.inner.lock().window.as_ref().and_then(|w| w.pid) {
            chrome::quit(pid);
        }
    }
}

/// The sign-in window, open; its process id once it started.
pub(super) struct Window {
    pub(super) pid: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with_task;
    use super::*;

    #[test]
    fn a_rejected_google_sign_in_opens_the_page_it_was_for() {
        let rejected = "https://accounts.google.com/v3/signin/rejected?continue=https%3A%2F%2Fsearch.google.com%2Fsearch-console%3Fresource_id%3Dx&dsh=S123&flowName=GlifWebSignIn";
        assert_eq!(page_for(rejected).as_deref(), Some("https://search.google.com/search-console?resource_id=x"));
    }

    #[test]
    fn a_google_sign_in_without_a_page_to_go_on_to_opens_itself() {
        let url = "https://accounts.google.com/signin?continue=javascript%3Aalert(1)";
        assert_eq!(page_for(url).as_deref(), Some(url));
        assert_eq!(page_for("https://example.com/login").as_deref(), Some("https://example.com/login"));
        assert_eq!(page_for("about:blank"), None);
    }

    #[test]
    fn a_broken_escape_is_kept_as_it_is() {
        assert_eq!(decode("50%25%2"), "50%%2");
        assert_eq!(decode("a+b%20c"), "a b c");
    }

    /// Against a real Chrome, in a window on screen, so not with the rest:
    /// `cargo test --lib a_sign_in_in_the_window -- --ignored`. A cookie set
    /// in the window reaches the tab once Done quits it: a SIGTERM quit before
    /// Chrome wrote its cookies, and the sign-in was lost.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn a_sign_in_in_the_window_reaches_the_tab() {
        use axum::{http::header::SET_COOKIE, response::IntoResponse, routing::get, Router};
        use std::sync::atomic::{AtomicU32, Ordering};
        let Some((app, root, _)) = app_with_task() else { return };
        let state = app.state::<AppState>();
        // The second visit only, which is the window's: the browser's own
        // visits, before and after, set nothing.
        static VISITS: AtomicU32 = AtomicU32::new(0);
        let router = Router::new().route(
            "/",
            get(|| async {
                let body = axum::response::Html("<p>Signed in</p>");
                match VISITS.fetch_add(1, Ordering::SeqCst) {
                    1 => ([(SET_COOKIE, "signed=yes; Max-Age=99999; Path=/")], body).into_response(),
                    _ => body.into_response(),
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://localhost:{}/", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, router).await });

        let page = state.browser.page(app.handle(), "t1").await.unwrap();
        page.navigate(&base).await.unwrap();
        page.settle(&state.browser).await;
        state.browser.sign_in_window(app.handle(), "t1").await.unwrap();
        assert!(state.browser.signing_in());
        assert!(state.browser.page(app.handle(), "t1").await.is_err(), "the browser waits for the window");

        tokio::time::sleep(Duration::from_secs(4)).await;
        state.browser.sign_in_done();
        for _ in 0..100 {
            if !state.browser.signing_in() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!state.browser.signing_in(), "the window quit");

        let page = state.browser.page(app.handle(), "t1").await.unwrap();
        page.settle(&state.browser).await;
        assert_eq!(state.browser.view("t1").url, base, "the tab is back on the page");
        let cookie = page.eval("document.cookie").await.unwrap();
        assert_eq!(cookie, serde_json::json!("signed=yes"));

        state.browser.shutdown(Duration::from_secs(3));
        state.ptys.shutdown(Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&root);
    }
}
