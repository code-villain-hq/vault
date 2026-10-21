# The studio's desktop starter: notes for coding agents

Every app's desktop build runs on its own, with no server elsewhere: it
starts its own Postgres and its own server for the workspaces on that Mac.
This crate does the starting and stopping; each app brings its server.
Forge, Glyph, Raven and Layer use it.

## Layout

- `src/postgres.rs`: `LocalPostgres`. Postgres from the archive compiled
  into the app (`postgresql_embedded`, `bundled`), with its data in a folder
  the app names (`data`, `password`, `runtime`). Loopback only, a password
  (0600), the app's preferred port or a free one. A server left running,
  or a stale pid file, from a run that didn't quit cleanly is dealt with on
  start.
- `src/server.rs`: `LocalServer`. Binds `127.0.0.1` (the app's preferred
  port, or a free one), runs the app's `serve(listener, shutdown)` and stops
  it on quit.

## Using it in an app

- Depend on `code-villain-desktop = { path = "../../kit/desktop" }` from the
  desktop shell only; the server image never builds it.
- Pin the same `POSTGRESQL_VERSION` as this crate's `.cargo/config.toml` in
  the app's `.cargo/config.toml`. The build downloads that archive once
  (a `GITHUB_TOKEN` avoids rate limits) and compiles it in.
- Docker builds need `kit/desktop` copied too, as the shell is a workspace
  member.
- Quit: stop the server, then Postgres.

## Changing it

`cargo test` here starts a real Postgres in a temporary folder, restarts it
and kills it. No CI runs it, so run it yourself. CI runs `bun run check` in
Forge, Glyph, Raven and Layer when `code/kit` changes, which builds each
app's desktop shell on this crate.
