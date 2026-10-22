# The studio's sync engine: notes for coding agents

Every app with a server keeps its workspaces in sync with this engine.
Forge, Glyph, Raven and Layer use it; the next app does too. The engine knows the protocol, the
tables and the feed; each app brings its models and its rules.

## Layout

- `src`: the Rust crate `code-villain-sync`. It applies transactions,
  appends to `sync_log`, serves the bootstrap snapshot and the log, follows
  it over the WebSocket (`follow`) and wakes the feed on Postgres NOTIFY
  (`listen`).
- `client`: the TypeScript package `@code-villain/sync-client`: the local
  store in IndexedDB, the outbox and the feed. It also holds the
  protocol's types (`Sync`). `FakeServer` is for the client's own tests,
  and isn't exported.
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

## Guests

A guest syncs only what they've been added to; admins and members sync the
whole workspace and pay nothing for it.

- `Model::guest_view` says what a guest sees of each model: `All`,
  `Hidden`, or `Where(sql)`, a condition on the record as `t`, with the
  guest as `$viewer`. Bootstrap, the log and the feed all use it. A delete
  goes to everyone: a client ignores an id it doesn't have.
- `Rules::guest_access` says whether the viewer is a guest, and the
  version of what they may see (`workspace_members.access_version`). The
  epoch a guest is served mixes it in, so when it goes up their client
  starts over from a fresh snapshot.
- A guest's update or delete of a record they can't see is NotFound; a
  create or update that leaves the record outside their view is Forbidden.
  So a model a guest can't see is one they can't write, without a rule.
- Whatever changes what a guest sees (their grants, their role, a record
  a `Where` depends on) bumps their `access_version`, then calls
  `notify::<R>()` if nothing was logged; a logged change notifies on its
  own. Forge's `team_members.rs`, Glyph's, Raven's and Layer's `access.rs`
  do this.

## Changing it

A change here changes every app. Keep the protocol as it is unless every app
moves with it, and keep app rules out: they go behind `Rules`. Check with
`bun run check` in Forge, Glyph, Raven and Layer; each runs the client's
tests and builds the crate (CI runs all four when `code/kit` changes).
