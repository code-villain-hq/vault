# Webview: notes for coding agents

`code-villain-webview` keeps a desktop app's window drawing on the Mac.
WebKit draws a page through a helper process, `com.apple.WebKit.GPU`.
When the helper stops answering (after the Mac sleeps, say), WebKit gives
up on it and asks macOS to stop it. macOS 27 refuses an app that request:
the app's unified log says `GPUProcess with PID … became unresponsive,
terminating it`, then `Could not signal service com.apple.WebKit.GPU: 1:
Operation not permitted`. The old helper can live on, the page stays tied
to it, and the window is black until the app restarts. Layer 0.13.0 did
this on Nov 13 2026; Forge logged the same that morning.

## Layout

- `src/lib.rs`: `keep_drawing(reach)` looks every 5 seconds, because
  WebKit tells the app nothing when it gives up. A look asks the webview
  which helper it draws through (`-[WKWebView _gpuProcessIdentifier]`,
  WebKit's own, unpublished). A helper it named at the last look and names
  no longer, which still runs (same pid, same start time, still WebKit's
  GPU helper, still this user's), is stopped with SIGKILL; a plain SIGTERM
  is ignored. The page then ties itself to a fresh helper. `abandoned` is
  that decision, tested on its own.
- At the next look the page is asked for one frame (`redraw`). With its
  new helper WebKit means to draw the whole page again, but only does once
  something on it changes: in a try on Layer's dev build, a page nobody
  touched stayed black until the app was clicked.

## Using it in an app

- Depend on it from the desktop crate and, in the app's setup, hand it the
  main window:
  `keep_drawing(move |look| { let _ = window.with_webview(move |w| look(w.inner())); })`.
  Every window of one app shares one helper, so one window is enough.
- A page with WebGL loses its contexts when the helper is stopped and has
  to make them anew. xterm's WebGL addon reports it (`onContextLoss`).
- Docker builds need `kit/webview` copied when the desktop crate is in the
  app's Cargo workspace.

## Trying it

Freeze the app's helper and let WebKit give up on it: find the helper's
pid in the app's unified log (`/usr/bin/log`, as zsh has a `log` of its
own; `GPUProcess Foreground Assertion' for process with PID=…`), never by
guessing among the `com.apple.WebKit.GPU` processes, `kill -STOP` it, and
make the window draw: hide and show the app (`NSRunningApplication`'s
`hide` and `unhide`, by pid). WebKit only waits on the helper, and so only
gives up, when the window is uncovered and has something to draw; a fresh
launch in front works, a window behind others can go minutes without.
The log then says WebKit gave up and couldn't stop it
(`didBecomeUnresponsive`, `Could not signal`); within 5 seconds
`GPUProcessConnection::didClose`, a new helper starts and the window
draws. If nothing happens, `kill -9` the frozen helper yourself. Never
`kill -STOP` a helper whose app this kit isn't in: nothing stops it again.

## Changing it

It has `unsafe`, for Objective-C and the kernel; each block says why it's
sound. `cargo test` here. CI runs it with the other kits
(`code/kit/dev-id/check-kits.ts`).
