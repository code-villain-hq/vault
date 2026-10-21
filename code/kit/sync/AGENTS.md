# The studio's sync engine: notes for coding agents

Every app with a server keeps its workspaces in sync with this engine. Forge
and Glyph use it; the next app does too. The engine knows the protocol, the
tables and the feed; each app brings its models and its rules.

## Layout

- `src`: the Rust crate `code-villain-sync`. It applies transactions,
  appends to `sync_log`, serves the bootstrap snapshot and the log, follows
  it over the WebSocket (`follow`) and wakes the feed on Postgres NOTIFY
  (`listen`).
- `client`: the TypeScript package `@code-villain/sync-client`: the local
  store in IndexedDB, the outbox, the feed and a `FakeServer` for tests. It
  also holds the protocol's types (`Sync`).
- `schema.sql`: the tables the engine needs, for reference. Each app keeps
  them in its own migrations, which sqlx has already applied in its
  databases, so they aren't moved here.

## Using it in an app

- Rust: the app's domain implements `Model` for its `ModelKind`; its sync
  crate has `pub struct <App>; impl Rules for <App>` with the hooks it needs
  (who may write, server-only fields, cascades, what a change sets off, the
  words for broken constraints). Every hook has a default that does nothing.
  Wrap the engine's calls with the app's types, as `forge_sync` does, so the
  server doesn't name the rules.
- TypeScript: a workspace member `"../../kit/sync/client"` in the app's
  `package.json`, and `packages/types/src/sync.ts` naming `Sync`'s types
  with the app's `ModelKind`.
- Docker builds from `code/`, so the image can copy `kit/sync`.

## Changing it

A change here changes every app. Keep the protocol as it is unless every app
moves with it, and keep app rules out: they go behind `Rules`. Check with
`bun run check` in Forge and in Glyph; both run the client's tests and build
the crate (CI runs them when `code/kit/sync` changes).
