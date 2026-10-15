//! What Chrome says without being asked, routed to the tab it is about,
//! wherever it is among its task's tabs: by the tab's session, or its
//! target id.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::cdp::{Cdp, Event};
use super::{attach, changed, console_line, Browser, Dialog, Inner, Viewport, CONSOLE_LINES};
use crate::commands::AppState;

/// The task, and the target id, of the tab an event is about.
fn tab_of(inner: &Inner, session: Option<&str>, target: Option<&str>) -> Option<(String, String)> {
    inner.tabs.iter().find_map(|(task, tabs)| {
        tabs.list
            .iter()
            .find(|t| match (session, target) {
                (Some(s), _) => t.session == s,
                (None, Some(id)) => t.target == id,
                (None, None) => false,
            })
            .map(|t| (task.clone(), t.target.clone()))
    })
}

/// How long an action waits for a window its page asked for. Past it, the
/// page's popup was blocked or closed before Chrome made it.
const WINDOW_WAIT: Duration = Duration::from_secs(3);

/// A window a page opened, to be attached as a tab.
struct Opened {
    cdp: Arc<Cdp>,
    user_agent: Option<String>,
    /// The opener's: the tab is laid out at the panel's size, as it was.
    viewport: Viewport,
    target: String,
    opener: String,
    url: String,
}

impl Browser {
    /// One event from Chrome, on its reading thread: nothing here waits.
    pub(super) fn on_event<R: Runtime>(&self, app: &AppHandle<R>, e: Event) {
        if e.method == "Page.screencastFrame" {
            return self.on_frame(app, e);
        }
        if e.method == "Target.targetCreated" {
            return self.on_window(app, &e.params);
        }
        let mut save = false;
        let mut tell = false;
        let mut ask = None;
        let task = {
            let mut inner = self.inner.lock();
            // A window gone is not one still on its way. Kept, the set grew by
            // one for every popup and link opened in a new tab, for good.
            if e.method == "Target.targetDestroyed" {
                if let Some(id) = e.params.get("targetId").and_then(Value::as_str) {
                    inner.heard.remove(id);
                }
            }
            // Shutting down: Chrome says each tab is destroyed as it closes,
            // and taken at its word, quitting the app forgot every tab it
            // was to bring back (BRW-6).
            if inner.running.is_none() {
                return;
            }
            let target = e
                .params
                .pointer("/targetInfo/targetId")
                .or_else(|| e.params.get("targetId"))
                .and_then(Value::as_str);
            let Some((task, id)) = tab_of(&inner, e.session.as_deref(), target) else { return };
            let Some(tabs) = inner.tabs.get_mut(&task) else { return };

            // Not a crash: a tab whose page crashed is still a tab, and a
            // reload brings it back.
            if matches!(e.method.as_str(), "Target.targetDestroyed" | "Target.detachedFromTarget") {
                tabs.remove(&id);
                // The last tab gone (a page closing itself): the task has
                // none until its browser is next needed, which makes one.
                if tabs.list.is_empty() {
                    inner.tabs.remove(&task);
                }
                save = true;
                tell = true;
            } else if let Some(at) = tabs.position(&id) {
                let active = at == tabs.active;
                let tab = &mut tabs.list[at];
                match e.method.as_str() {
                    "Page.frameNavigated" if e.params.pointer("/frame/parentId").is_none() => {
                        // A page that could not load is shown as Chrome's own
                        // error page, whose address is not a site; the one that
                        // failed is what the tab is on, for the panel and for
                        // agents alike.
                        let url = e
                            .params
                            .pointer("/frame/unreachableUrl")
                            .or_else(|| e.params.pointer("/frame/url"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        // A new page holds no password the app filled in.
                        tab.secret = None;
                        if tab.url != url {
                            tab.url = url.to_string();
                            save = true;
                            tell = true;
                        }
                    }
                    "Page.frameStartedLoading" | "Page.frameStoppedLoading" => {
                        if e.params.get("frameId").and_then(Value::as_str) == Some(tab.target.as_str()) {
                            tab.loading = e.method == "Page.frameStartedLoading";
                            tell = true;
                            if !tab.loading {
                                ask = Some(tab.target.clone());
                            }
                        }
                    }
                    // A route change in a page that changes its own address.
                    "Page.navigatedWithinDocument" => {
                        if e.params.get("frameId").and_then(Value::as_str) == Some(tab.target.as_str()) {
                            // A page that signs in without loading another
                            // moves on to its own next one: the form is gone.
                            tab.secret = None;
                            ask = Some(tab.target.clone());
                        }
                    }
                    "Target.targetInfoChanged" => {
                        let title = e.params.pointer("/targetInfo/title").and_then(Value::as_str).unwrap_or_default();
                        if tab.title != title {
                            tab.title = title.to_string();
                            tell = true;
                        }
                    }
                    "Runtime.consoleAPICalled" | "Runtime.exceptionThrown" => {
                        tab.console.push_back(console_line(&e.method, &e.params));
                        while tab.console.len() > CONSOLE_LINES {
                            tab.console.pop_front();
                        }
                    }
                    "Page.javascriptDialogOpening" => {
                        tab.dialog = Some(Dialog::from_event(&e.params));
                        // Only the active tab's dialog is anyone's to answer
                        // now; an action waits on no other.
                        if active {
                            self.dialog_opened.notify_waiters();
                        }
                        tell = true;
                    }
                    // Sent as the page asks, before Chrome has made the window:
                    // under load, longer than an action's first wait.
                    "Page.windowOpen" => {
                        let w = &mut tab.windows;
                        // One that never came (a blocked popup) is forgotten,
                        // or every window after it would read as still coming.
                        if !w.last_asked.is_some_and(|at| at.elapsed() < WINDOW_WAIT) {
                            w.heard = w.asked;
                        }
                        w.asked += 1;
                        w.last_asked = Some(Instant::now());
                    }
                    "Page.javascriptDialogClosed" => {
                        tab.dialog = None;
                        tell = true;
                    }
                    _ => {}
                }
            }
            task
        };
        if tell {
            changed(app, &task);
        }
        // Outside the lock, and off this thread: a config write waits on the disk.
        if save {
            self.save_soon(app, &task);
        }
        if let Some(target) = ask {
            self.ask_about(app, task, target);
        }
    }

    /// Ask Chrome what a tab is called and where it is, off this thread.
    /// Chrome says when a tab's address changes, but its title as it does is
    /// only the address again: the page's own title, read from the page as
    /// it loads, is never announced.
    fn ask_about<R: Runtime>(&self, app: &AppHandle<R>, task: String, target: String) {
        let Some(cdp) = self.inner.lock().running.as_ref().map(|r| r.cdp.clone()) else { return };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(info) = cdp.call("Target.getTargetInfo", serde_json::json!({ "targetId": target }), None).await else { return };
            let get = |k: &str| info.pointer(&format!("/targetInfo/{k}")).and_then(Value::as_str).unwrap_or_default().to_string();
            let (title, url) = (get("title"), get("url"));
            let browser = &app.state::<AppState>().browser;
            let moved = {
                let mut inner = browser.inner.lock();
                let Some(tab) = inner.tabs.get_mut(&task).and_then(|t| t.list.iter_mut().find(|t| t.target == target)) else { return };
                let moved = !url.is_empty() && !url.starts_with("chrome-error:") && tab.url != url;
                if tab.title == title && !moved {
                    return;
                }
                tab.title = title;
                if moved {
                    tab.url = url;
                }
                moved
            };
            changed(&app, &task);
            if moved {
                browser.save_soon(&app, &task);
            }
        });
    }

    /// A page opened a window: a link to a new tab, `window.open`, a sign-in
    /// popup. It becomes a tab, next to its opener and active (BRW-14).
    /// Attached off this thread, which must not wait on Chrome's answers:
    /// it is the one that reads them.
    fn on_window<R: Runtime>(&self, app: &AppHandle<R>, p: &Value) {
        let info = |k: &str| p.pointer(&format!("/targetInfo/{k}")).and_then(Value::as_str).map(str::to_string);
        let (Some("page"), Some(opener), Some(target)) = (info("type").as_deref(), info("openerId"), info("targetId")) else {
            return;
        };
        let (task, opened) = {
            let mut inner = self.inner.lock();
            let Some((task, _)) = tab_of(&inner, None, Some(&opener)) else { return };
            inner.heard.insert(target.clone());
            if let Some(t) = inner.tabs.get_mut(&task).and_then(|tabs| tabs.list.iter_mut().find(|t| t.target == opener)) {
                t.windows.heard += 1;
            }
            // Already one of the app's: a tab it made itself.
            if tab_of(&inner, None, Some(&target)).is_some() {
                return;
            }
            let Some(r) = inner.running.as_ref() else { return };
            let viewport = inner.active(&task).map(|t| t.viewport).unwrap_or_default();
            let opened = Opened {
                cdp: r.cdp.clone(),
                user_agent: r.user_agent.clone(),
                viewport,
                target,
                opener,
                url: info("url").unwrap_or_default(),
            };
            // Counted under the same lock that marks it heard: between two
            // locks, an action settling saw a window heard of and none being
            // adopted, and answered with the page it was clicked on.
            inner.adopting += 1;
            (task, opened)
        };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let browser = &app.state::<AppState>().browser;
            let target = opened.target.clone();
            let adopted = browser.adopt(&task, opened).await;
            browser.inner.lock().adopting -= 1;
            if adopted {
                browser.save_soon(&app, &task);
                changed(&app, &task);
                // It may have loaded while it was being attached, unheard.
                browser.ask_about(&app, task, target);
            }
        });
    }

    /// Attach a window a page opened as a tab, beside its opener. False when
    /// it was closed first, as some sign-in windows are once they are done.
    async fn adopt(&self, task: &str, o: Opened) -> bool {
        let Ok(mut tab) = attach(&o.cdp, o.target, o.user_agent.as_deref(), o.viewport).await else { return false };
        if !o.url.is_empty() {
            tab.url = o.url;
        }
        tab.opener = Some(o.opener.clone());
        let mut inner = self.inner.lock();
        let Some(tabs) = inner.tabs.get_mut(task) else { return false };
        tabs.open_next_to(tab, Some(&o.opener));
        true
    }

    /// Whether a window a page opened is still being made a tab: an action
    /// that opened one has not finished until it is (`Page::settle`).
    pub fn adopting(&self) -> bool {
        self.inner.lock().adopting > 0
    }

    /// Whether one of `task`'s pages asked for a window in the last few
    /// seconds that the app has not heard of yet. On a loaded machine Chrome
    /// took longer than an action's first wait to make it, so neither its
    /// events nor its target list had it yet, and the click that opened a
    /// sign-in window was answered with the page it was clicked on (BRW-14).
    pub fn window_coming(&self, task: &str) -> bool {
        let inner = self.inner.lock();
        inner.tabs.get(task).is_some_and(|tabs| {
            tabs.list.iter().any(|t| {
                t.windows.asked > t.windows.heard && t.windows.last_asked.is_some_and(|at| at.elapsed() < WINDOW_WAIT)
            })
        })
    }

    /// Whether Chrome lists a window one of `task`'s tabs opened that the app
    /// has not heard of yet. On a busy machine its `targetCreated` came after
    /// `settle` had looked, and a click that opened a tab was answered with
    /// the page it was clicked on (BRW-14).
    pub fn unheard(&self, task: &str, targets: &[Value]) -> bool {
        let inner = self.inner.lock();
        let Some(tabs) = inner.tabs.get(task) else { return false };
        let ours = |id: &str| tabs.list.iter().any(|t| t.target == id);
        targets.iter().any(|t| {
            let s = |k: &str| t.get(k).and_then(Value::as_str);
            s("type") == Some("page")
                && s("openerId").is_some_and(ours)
                && s("targetId").is_some_and(|id| !ours(id) && !inner.heard.contains(id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::tests::app_with_task;
    use serde_json::json;
    use crate::browser::tools::call;

    fn text(content: &[Value]) -> String {
        content.iter().filter_map(|c| c.get("text").and_then(Value::as_str)).collect()
    }

    /// Chrome made the window as the click ran, and lists it; its
    /// `targetCreated` is still on its way. The click's answer waits for
    /// that window, and for no other: not one already heard of (adopting
    /// says the rest), not another task's, not the tab itself.
    #[test]
    fn an_action_waits_for_a_window_chrome_made_and_the_app_has_not_heard_of() {
        let tab = super::super::tests::tab;
        let browser = Browser::default();
        browser.inner.lock().tabs.insert("t1".into(), super::super::tabs::Tabs::new(vec![tab("home")], 0));
        let window = |id: &str, opener: &str| json!({ "type": "page", "targetId": id, "openerId": opener });

        assert!(browser.unheard("t1", &[window("popup", "home")]));
        assert!(!browser.unheard("t1", &[window("popup", "another-tasks-tab")]));
        assert!(!browser.unheard("t1", &[json!({ "type": "page", "targetId": "home" })]));
        assert!(!browser.unheard("t1", &[json!({ "type": "service_worker", "targetId": "sw", "openerId": "home" })]));
        assert!(!browser.unheard("t2", &[window("popup", "home")]), "a task with no tabs");
        browser.inner.lock().heard.insert("popup".into());
        assert!(!browser.unheard("t1", &[window("popup", "home")]));
    }

    #[test]
    fn a_window_closed_is_no_longer_remembered_as_heard_of() {
        let app = tauri::test::mock_app();
        let browser = Browser::default();
        browser.inner.lock().heard.insert("popup".into());
        let destroyed = Event {
            method: "Target.targetDestroyed".into(),
            params: json!({ "targetId": "popup" }),
            session: None,
        };

        browser.on_event(app.handle(), destroyed);

        assert!(browser.inner.lock().heard.is_empty());
    }

    /// The page asked for a window and Chrome has not made it yet: the click
    /// waits, whichever of the two Chrome sends first, and not for one that
    /// never came.
    #[test]
    fn an_action_waits_for_a_window_its_page_asked_for() {
        let browser = Browser::default();
        let tab = super::super::tests::tab("home");
        browser.inner.lock().tabs.insert("t1".into(), super::super::tabs::Tabs::new(vec![tab], 0));
        let set = |asked: u32, heard: u32, ago: u64| {
            let mut inner = browser.inner.lock();
            let w = &mut inner.tabs.get_mut("t1").unwrap().list[0].windows;
            *w = super::super::Windows { asked, heard, last_asked: Instant::now().checked_sub(Duration::from_secs(ago)) };
        };
        set(1, 0, 0);
        assert!(browser.window_coming("t1"), "asked for, not yet made");
        assert!(!browser.window_coming("t2"), "only the task's own");
        set(1, 1, 0);
        assert!(!browser.window_coming("t1"), "made");
        set(1, 2, 0);
        assert!(!browser.window_coming("t1"), "made before it was asked for, as Chrome may say");
        set(1, 0, 4);
        assert!(!browser.window_coming("t1"), "a popup that was never made is not waited on for ever");
    }

    /// Against a real Chrome: a page that cannot be reached keeps its own
    /// address, not Chrome's error page's (`chrome-error://chromewebdata/`),
    /// which is not a site: the panel offered to allow "chromewebdata", and
    /// agents were refused a page on this machine.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_that_cannot_be_reached_keeps_its_address() {
        let Some((app, root, pane)) = app_with_task() else { return };
        let state = app.state::<AppState>();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let dead = format!("http://localhost:{port}/");

        let page = state.browser.page(app.handle(), "t1").await.unwrap();
        let refused = page.navigate(&dead).await.unwrap_err();
        assert!(refused.to_string().contains("ERR_CONNECTION_REFUSED"), "{refused}");
        page.settle(&state.browser).await;
        assert_eq!(state.browser.view("t1").url, dead);

        let read = call(app.handle(), "browser_snapshot", json!({}), Some(&pane)).await.unwrap();
        assert!(text(&read).contains(&format!("URL: {dead}")), "{}", text(&read));

        state.browser.shutdown(Duration::from_secs(3));
        state.ptys.shutdown(Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&root);
    }
}
