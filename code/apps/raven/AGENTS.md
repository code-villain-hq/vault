# Raven: notes for coding agents

Raven watches the studio's running apps on three levels: Health for
anyone, Technical for developers, Product for the PO (not built yet). The
README says what each level shows and what's decided. It is built the
way Glyph is: a Rust server on Postgres with the studio's sync engine
(`code/kit/sync`), a React app (`apps/web`) that keeps a local copy of
the workspace, and a Tauri desktop shell (`apps/desktop`).
When something here works like Glyph or Forge, their code is the
reference; keep them alike.

## Shared design and brand

Read [`studio/design/principles.md`](../../../studio/design/principles.md) before any UI
change and [`studio/design/behaviour.md`](../../../studio/design/behaviour.md) before
changing what Raven does. Use the `ui-*` classes of
[`studio/design/components.css`](../../../studio/design/components.css) and token-backed
Tailwind classes; `studio/design/lint` fails on literal colours, sizes or radii.
Status colours go only where something needs a look: a calm app, bar or
row stays quiet. Charts follow the dataviz rules: one axis, thin marks,
a legend, a hover tooltip.

The web app bundles snapshots of the tokens, `components.css`, the fonts
and the Raven mark from [`studio/brand/mark/raven.svg`](../../../studio/brand/mark/raven.svg).
Run `bun run design:sync` after changing them in the vault.

## Commands

- `bun run db`: start Postgres (Docker, port 5435). `bun run db:reset` wipes it.
- `bun run dev`: Rust server (:7580) plus web app (:5175, proxies `/api`).
  It sets `RAVEN_VERSION` to the commit, so calls carry their version.
- `bun run dev:desktop`: the Tauri app against the running dev server.
  `bun run build:desktop` packages it. The app runs Raven itself, on its
  own Postgres (7586) and server (7585), for the workspaces on this Mac,
  and signs in to servers (the Rust port, 7580, not Vite's) for the
  others. Its data is in `~/Library/Application Support/eu.codevillain.raven`.
- `bun run seed`: the local account, its "Code Villain" workspace, a
  few dozen calls, and Gus Guest, a guest in Raven itself; needs `bun run dev`. The password is in `scripts/seed.ts`.
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, generated types and design
  copies up to date, Biome, typecheck, the bun tests (the sync kit's too),
  web build. It must pass before you finish. CI runs it on every push that
  touches Raven (`.forgejo/workflows/raven.yml`).
  `RAVEN_CHECK_SKIP_DESKTOP=1` leaves the desktop app out.
- `cargo run -p raven-server -- reset-password <email>` sets a new password.

Rust tests need Postgres running (`bun run db`); `DATABASE_URL` comes from
`.cargo/config.toml`.

## Workspaces and what syncs

A workspace is a team's apps, with members and roles as in Forge. Only its
settings sync: the apps it watches (`App`) and its members. Calls, logs and
checks stay on the server and are read live
(`/api/w/{id}/apps/{key}/health|technical|action|calls/{trace}`); offline
those pages say "Not available offline" and name the place they need.

Settings → Apps is where admins add, change and remove apps. An app's
`key` is the OpenTelemetry `service.name` its calls carry, so it can't
change; "settings" is taken by the settings pages. "New key"
(`POST /api/w/{id}/apps/{key}/key`) shows the key once and replaces the
one before. Keys live in `app_keys`, never in a synced record.

One app per Raven is Raven itself (`is_self`, set by the server): the
first workspace's. Its health address is the server's own listener.

## How data comes in

- Raven records its own calls: `telemetry::record_calls` makes a span per
  request named `METHOD /route`, `telemetry::timed` a child span per
  database query, and the `Capture` layer keeps the logs. A background
  writer stores them, so a request never waits on its own record.
- Other apps send OpenTelemetry over HTTP as JSON (`otlp.rs`), with a
  Bearer key; Raven stores only its hash.
- `health::watch` checks every app with a health address, in every
  workspace, every 30 seconds, and clears records older than 30 days.

## The desktop app

The same web app in a Tauri window, as Glyph's. Raven on this Mac watches
the apps on this Mac: its first workspace starts with Raven itself, the
installed Forge and Glyph (7385, 7485) and their dev servers (7380, 7480).
For a server, it keeps its session
token in the system keychain and sends it as a bearer token with
`x-raven-client: desktop`; the server allows the `tauri://localhost`
origin. Signing in with Code Villain happens in the browser, and comes
back through a `raven://signed-in` link with a one-time code only this
app can trade (`oidc.rs` says how). Deep links only reach an app macOS
has registered from a real folder: try a test build, with its own
identifier, from `~/Applications`.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Raven's rules for the sync engine: only admins write an
  `App`, and nobody sets `is_self` or removes Raven itself.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts,
  invites, OTLP intake, health checks, the pages. `serve.rs` runs it on a
  listener until told to stop, for the server binary and the desktop app
  alike; `local.rs` is the one person of the desktop app's own Raven.
- `packages/types`: generated from Rust. Never edit by hand. The pages'
  shapes stay hand-written in `apps/web/src/types.ts`.
- `apps/web`: React + Base UI + Tailwind. Its workspaces come from places
  (`server.ts`): this Mac, the server it came from, or servers the desktop
  app signed in to.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Raven on Postgres for
  the workspaces on this Mac, with `code/kit/desktop`.
- `migrations`: SQL schema, applied by the server on start.

## Rules

- Wrap every query in `timed(...)`, so Technical shows Raven's own
  queries too. sqlx 0.9 refuses SQL built at runtime: write each
  statement as a `const`.
- Wire field names are snake_case. Times go out as ISO times, never
  formatted: the page shows them in the reader's time zone.
- Change the schema with a new numbered migration; don't edit one that
  has been committed.
- Every write to a synced model goes through `raven_sync::apply_transaction`.
- Workspace endpoints go under `/api/w/{workspace_id}` and take the `Member`
  extractor, which checks membership; a page's query filters by the app's
  id. Admin-only ones call `require_admin()`.
- A level's status always names its reason, and names the action to look
  into when calls are why.

## Guests

A guest sees only the apps an admin adds them to (`app_access`, set with
`raven_sync::set_apps`): they sync those, and every page of any other app
(`apps::find`) and `GET /apps` leave it out. They see no Members.

## Groups

With `RAVEN_SCIM_TOKEN` set, the Code Villain account sends its people and
groups to `/api/scim/v2` (`code/kit/scim`; `crates/server/src/groups.rs`).
A workspace linked to `raven` (`PUT /api/w/{id}/group`) has `raven-users` as
members, `raven-admins` as admins and `raven-guests` as guests, and Members
won't change their roles. A guest's apps are still set by hand. Tests: `crates/server/tests/scim.rs`.

## Adding a synced model

1. A migration for its table, with `id`, `workspace_id`, `created_at` and
   `updated_at` columns.
2. In `crates/domain`: the record, create and patch types, a `ModelKind`
   variant, and the `match` arms the compiler asks for.
3. In `crates/sync/src/rules.rs`: who may write it, and any fields only the
   server may set. In `crates/domain`'s `guest_view`: what a guest sees of
   it.
4. `bun run gen:types`, then add it to `Schema` in `apps/web/src/sync.ts`.
