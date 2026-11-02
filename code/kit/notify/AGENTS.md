# Notify: notes for coding agents

`code-villain-notify` shows a desktop app's notifications on the Mac, with
UserNotifications. tauri-plugin-notification posts through
NSUserNotificationCenter, which macOS no longer shows: an app that uses it
never appears in System Settings → Notifications, and nothing it sends
shows. Almanac's reminders, Forge's inbox and Layer's banners use this kit.

## Layout

- `src/lib.rs`: `ask()`, whether the app may notify, which macOS asks
  once and remembers. `show(title, body)` shows it with the sound, and
  while the app is in front too (`Presenter`, the center's delegate). False
  outside an app bundle (`tauri dev`, where UserNotifications would throw)
  and off the Mac, so the app falls back to the plugin. `show_opening`
  also carries what a click opens, in the notification's `userInfo`, and
  hands it to `on_open` (Layer's banners).

## Using it in an app

- Depend on it from the desktop crate, call `ask` in the app's setup, and
  `show` where the app notified with the plugin, keeping the plugin for
  when it returns false. A web app notifies through a command of the
  desktop app that calls it (Forge's `notify`), not the plugin's JS.
- For clicks, set `on_open` before `ask`. Copies of one app opened with
  `open -n` (Layer's profiles) share one bundle identifier, so macOS may
  hand a click to another copy.
- Try it in a build signed as releases are (`APPLE_SIGNING_IDENTITY=-
  tauri build`, as `scripts/release.ts` runs it): a plain `tauri build` is
  signed only under the binary's name, not the app's identifier.
- Docker builds need `kit/notify` copied when the desktop crate is in the
  app's Cargo workspace.

## Changing it

It's the one kit with `unsafe`, for talking to Objective-C; each block says
why it's sound. `cargo test` here. CI runs it with the other kits
(`code/kit/dev-id/check-kits.ts`).
