//! A watched pane's output on its way to the window.
//!
//! Each pane has one thread for this, started with it and ended by its
//! reader at EOF. It used to be a new thread for every burst of output,
//! which for an agent redrawing, or for every key's echo, was a thread
//! started and torn down many times a second per pane. One thread also
//! means one sender, so batches leave in the order they were taken.
//!
//! Batches go over the terminal's Tauri channel as raw bytes: the position
//! just past them (8 bytes, big-endian), then the bytes. As an event they
//! were base64 inside JSON, a third larger, and encoded and decoded again
//! on the way.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::MutexGuard;
use tauri::ipc::{Channel, InvokeResponseBody};

use super::{Pane, OUTPUT_COALESCE, OUTPUT_GATHER};

/// What the reader has told the pane's flush thread.
#[derive(Default)]
pub(super) struct Flush {
    /// Output is waiting to be shipped.
    scheduled: bool,
    /// Output arrived in answer to a key, to cut the thread's wait short.
    ///
    /// The first event after a quiet spell already went out after
    /// `OUTPUT_GATHER`, but a key pressed while the pane was busy — an agent's
    /// spinner going, a watcher printing — waited out the rest of the frame
    /// behind it: 20ms typical, 35ms often, on every key, in exactly the panes
    /// people type into most. Measured by the `echo_latency` test.
    urgent: bool,
    /// The reader has reached the end: ship what is left, and stop.
    ended: bool,
}

/// The terminal's end of the feed: where a watched pane's output goes.
pub(super) type Feed = Channel<InvokeResponseBody>;

/// One batch as the terminal reads it: where it ends, then its bytes.
pub(super) fn framed(bytes: &[u8], end: u64) -> Vec<u8> {
    let mut frame = Vec::with_capacity(8 + bytes.len());
    frame.extend_from_slice(&end.to_be_bytes());
    frame.extend_from_slice(bytes);
    frame
}

impl Pane {
    /// Output has arrived: have the flush thread ship it, if anyone is
    /// looking. `keyed` is the echo of a key, which does not wait for the
    /// frame.
    pub(super) fn schedule_flush(&self, keyed: bool) {
        // Scrollback always records. The webview only gets a feed while the
        // pane is on screen.
        if !self.watched.load(Ordering::Acquire) {
            return;
        }
        let mut flush = self.flush.lock();
        let idle = !flush.scheduled;
        flush.scheduled = true;
        flush.urgent |= keyed;
        if idle || keyed {
            self.wake.notify_one();
        }
    }

    /// The reader is done: the flush thread ships the last of it and ends.
    pub(super) fn end_flushing(&self) {
        self.flush.lock().ended = true;
        self.wake.notify_one();
    }

    /// Ship what the window has not been sent, if it is watching.
    fn ship_unsent(&self) -> bool {
        if !self.watched.load(Ordering::Acquire) {
            return false;
        }
        let Some((bytes, end)) = self.output.lock().take_unsent() else {
            return false;
        };
        if let Some(feed) = self.feed.lock().as_ref() {
            let _ = feed.send(InvokeResponseBody::Raw(framed(&bytes, end)));
        }
        *self.last_flush.lock() = Instant::now();
        true
    }
}

/// The pane's flush thread: waits for output, holds it long enough that a
/// burst of TUI redraws leaves as one batch, and ships it.
pub(super) fn run(pane: Arc<Pane>) {
    let mut flush = pane.flush.lock();
    loop {
        pane.wake.wait_while(&mut flush, |f| !f.scheduled && !f.ended);
        if flush.ended {
            break;
        }
        let quiet = pane.last_flush.lock().elapsed();
        let mut wait = if quiet >= OUTPUT_COALESCE { OUTPUT_GATHER } else { OUTPUT_COALESCE - quiet };
        loop {
            // Interruptible: an echo arriving meanwhile cuts it short, then
            // waits `OUTPUT_GATHER` for the rest of its frame.
            pane.wake.wait_while_for(&mut flush, |f| !f.urgent && !f.ended, wait);
            if flush.urgent {
                flush.urgent = false;
                MutexGuard::unlocked(&mut flush, || std::thread::sleep(OUTPUT_GATHER));
            }
            wait = OUTPUT_COALESCE;
            // What the reader adds from here on schedules the next batch, so
            // a read landing between the take and the wait is not left behind.
            flush.scheduled = false;
            // Hidden since it was scheduled: what is waiting stays in
            // scrollback, and showing the pane again collects it.
            if !MutexGuard::unlocked(&mut flush, || pane.ship_unsent()) || flush.ended {
                break;
            }
        }
        if flush.ended {
            break;
        }
    }
    drop(flush);
    // Drain anything still held so the last paint is not lost.
    pane.ship_unsent();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_says_where_it_ends_before_its_bytes() {
        let frame = framed(b"hi", 258);
        assert_eq!(frame, [0, 0, 0, 0, 0, 0, 1, 2, b'h', b'i']);
    }
}
