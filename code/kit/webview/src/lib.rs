//! Keeps a studio desktop app's window drawing on the Mac.
//!
//! WebKit draws a page through a helper process, `com.apple.WebKit.GPU`.
//! When the helper stops answering (a Mac waking from sleep can do it),
//! WebKit gives up on it: it asks macOS to stop it and starts a fresh one
//! when the page next needs it. macOS 27 refuses an app that request
//! ("Could not signal service com.apple.WebKit.GPU: 1: Operation not
//! permitted"), so the old helper can live on with the page still tied to
//! it. Nothing is drawn: the window shows only its background colour, and
//! the page freezes for about 12 seconds in every 30. A restart was the
//! only way out, and a restart stops whatever the app was running.
//!
//! WebKit tells the app nothing when it gives up on its helper, so
//! `keep_drawing` looks every few seconds. A helper WebKit named before, and
//! no longer names, that still runs is stopped. The page then ties itself
//! to a fresh one, and at the next look it is asked for a frame, so it
//! draws again even if nothing on it changes. Its WebGL contexts are lost
//! and made anew, which a page using WebGL has to handle (xterm's addon
//! reports it).

use std::ffi::c_void;
use std::time::Duration;

/// One look at the window's webview. The app runs it on the main thread with
/// the window's `WKWebView`.
pub type Look = Box<dyn FnOnce(*mut c_void) + Send>;

/// How often to look. A look asks WebKit one question and the kernel two,
/// so this costs nothing; it is how long a window can stay black.
const EVERY: Duration = Duration::from_secs(5);

/// Watches the app's window while the app runs. `reach` runs a [`Look`] on
/// the main thread with the window's `WKWebView`; in a Tauri app:
///
/// ```ignore
/// code_villain_webview::keep_drawing(move |look| {
///     let _ = window.with_webview(move |webview| look(webview.inner()));
/// });
/// ```
///
/// Every window of an app shares one helper, so one is enough. Off the Mac
/// it does nothing.
pub fn keep_drawing(reach: impl Fn(Look) + Send + 'static) {
    #[cfg(target_os = "macos")]
    {
        let watching = std::thread::Builder::new()
            .name("webview-helper".into())
            .spawn(move || {
                loop {
                    reach(Box::new(mac::look));
                    std::thread::sleep(EVERY);
                }
            });
        if let Err(e) = watching {
            tracing::warn!("the window's drawing is not watched: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = reach;
}

/// A helper process: its pid, and when it started, so a pid macOS has since
/// given to another process is never taken for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Helper {
    pid: i32,
    started: u64,
}

/// The helper to stop: the one WebKit named at the last look, if it names
/// another one now, or none, and that one still `runs`.
fn abandoned(last: Option<Helper>, named: i32, runs: impl Fn(Helper) -> bool) -> Option<Helper> {
    last.filter(|last| last.pid != named && runs(*last))
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::{Mutex, PoisonError};

    use block2::Block;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel};
    use objc2_foundation::NSString;

    use super::{Helper, abandoned};

    struct Watch {
        /// The helper WebKit named at the last look.
        named: Option<Helper>,
        /// A helper was stopped at the last look.
        stopped: bool,
    }

    static WATCH: Mutex<Watch> = Mutex::new(Watch {
        named: None,
        stopped: false,
    });

    pub(super) fn look(webview: *mut c_void) {
        if webview.is_null() {
            return;
        }
        // SAFETY: `keep_drawing`'s caller hands `look` the window's WKWebView,
        // on the main thread, alive for the call.
        let webview: &AnyObject = unsafe { &*webview.cast::<AnyObject>() };
        let Some(named) = named(webview) else { return };
        let mut watch = WATCH.lock().unwrap_or_else(PoisonError::into_inner);
        if std::mem::take(&mut watch.stopped) {
            redraw(webview);
        }
        if let Some(stuck) = abandoned(watch.named, named, |h| helper(h.pid) == Some(h)) {
            watch.stopped = stop(stuck);
        }
        watch.named = helper(named);
    }

    /// The pid of the helper WebKit draws this webview through, 0 while it
    /// has none. None if this WebKit can't say.
    fn named(webview: &AnyObject) -> Option<i32> {
        let asked = sel!(_gpuProcessIdentifier);
        // SAFETY: NSObject's respondsToSelector: takes a selector and
        // returns a BOOL.
        let answers: bool = unsafe { msg_send![webview, respondsToSelector: asked] };
        if !answers {
            return None;
        }
        // SAFETY: WebKit's -[WKWebView _gpuProcessIdentifier] takes nothing
        // and returns a pid_t; this WebKit has it, as it just said.
        Some(unsafe { msg_send![webview, _gpuProcessIdentifier] })
    }

    /// `pid` if it is a WebKit GPU helper of this user's, with when it started.
    fn helper(pid: i32) -> Option<Helper> {
        if pid <= 0 {
            return None;
        }
        // SAFETY: proc_bsdinfo is plain integers and bytes, for which zero is
        // a value.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = size_of::<libc::proc_bsdinfo>() as i32;
        // SAFETY: proc_pidinfo writes at most `size` bytes into `info`, which
        // is that big.
        let got = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size)
        };
        // SAFETY: getuid can't fail.
        if got != size || info.pbi_uid != unsafe { libc::getuid() } {
            return None;
        }
        let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: proc_pidpath writes at most the buffer's length into it.
        let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        let path = path.get(..usize::try_from(len).ok()?)?;
        path.ends_with(b"/com.apple.WebKit.GPU").then_some(Helper {
            pid,
            started: info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
        })
    }

    fn stop(stuck: Helper) -> bool {
        // SAFETY: kill only sends a signal, to the helper WebKit named before,
        // checked to be the same process a moment ago.
        if unsafe { libc::kill(stuck.pid, libc::SIGKILL) } == 0 {
            tracing::warn!(
                pid = stuck.pid,
                "WebKit gave up on its GPU helper but couldn't stop it; stopped it so the window draws again"
            );
            true
        } else {
            let e = std::io::Error::last_os_error();
            tracing::warn!(
                pid = stuck.pid,
                "WebKit's abandoned GPU helper could not be stopped: {e}"
            );
            false
        }
    }

    /// Asks the page for one frame. With its new helper WebKit means to draw
    /// the whole page again, but only does once something on it changes: a
    /// page nobody touched stayed black until the app was clicked.
    fn redraw(webview: &AnyObject) {
        let frame = NSString::from_str("requestAnimationFrame(function () {})");
        let done: Option<&Block<dyn Fn(*mut AnyObject, *mut AnyObject)>> = None;
        // SAFETY: -[WKWebView evaluateJavaScript:completionHandler:] takes a
        // string and a block it may be given none of.
        let () =
            unsafe { msg_send![webview, evaluateJavaScript: &*frame, completionHandler: done] };
    }

    #[cfg(test)]
    mod tests {
        use super::helper;

        #[test]
        fn only_a_webkit_gpu_helper_is_a_helper() {
            // This test is not one, and nothing has pid 0 or below.
            assert_eq!(helper(std::process::id() as i32), None);
            assert_eq!(helper(0), None);
            assert_eq!(helper(-1), None);
            // launchd is root's.
            assert_eq!(helper(1), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Helper, abandoned};

    const OLD: Helper = Helper {
        pid: 55302,
        started: 1,
    };

    #[test]
    fn a_helper_webkit_gave_up_on_is_stopped_while_it_runs() {
        // What Layer showed: WebKit names no helper, the old one runs on.
        assert_eq!(abandoned(Some(OLD), 0, |_| true), Some(OLD));
        // WebKit started a fresh one, and the old one runs on too.
        assert_eq!(abandoned(Some(OLD), 65720, |_| true), Some(OLD));
    }

    #[test]
    fn nothing_is_stopped_otherwise() {
        // WebKit still names it.
        assert_eq!(abandoned(Some(OLD), OLD.pid, |_| true), None);
        // It is gone, or its pid is another process's now.
        assert_eq!(abandoned(Some(OLD), 0, |_| false), None);
        // The first look, or none was named at the last one.
        assert_eq!(abandoned(None, 0, |_| true), None);
        assert_eq!(abandoned(None, 65720, |_| true), None);
    }
}
