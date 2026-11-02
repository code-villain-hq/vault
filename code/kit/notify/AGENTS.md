# Notify: notes for coding agents

`code-villain-notify` shows a desktop app's notifications on the Mac, with
UserNotifications. tauri-plugin-notification posts through
NSUserNotificationCenter, which macOS no longer shows: an app that uses it
never appears in System Settings → Notifications, and nothing it sends
shows. Almanac uses this kit for its reminders. Forge and Layer still use
the plugin.

## Layout

- `src/lib.rs`: `ask()`, whether the app may notify, which macOS asks
  once and remembers. `show(title, body)` shows it with the sound, and
  while the app is in front too (`Presenter`, the center's delegate). False
  outside an app bundle (`tauri dev`, where UserNotifications would throw)
  and off the Mac, so the app falls back to the plugin.

## Using it in an app

- Depend on it from the desktop crate, call `ask` in the app's setup, and
  `show` where the app notified with the plugin, keeping the plugin for
  when it returns false.
- Try it in a build signed as releases are (`APPLE_SIGNING_IDENTITY=-
  tauri build`, as `scripts/release.ts` runs it): a plain `tauri build` is
  signed only under the binary's name, not the app's identifier.
- Docker builds need `kit/notify` copied when the desktop crate is in the
  app's Cargo workspace.

## Changing it

It's the one kit with `unsafe`, for talking to Objective-C; each block says
why it's sound. `cargo test` here. CI runs it with the other kits
(`code/kit/dev-id/check-kits.ts`).
