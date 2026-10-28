# Almanac: notes for coding agents

Almanac keeps what you learn by topic. You start a topic with a name and
what it sticks to; you ask it things in a talk beside its page; the page
keeps short points with their sources, grouped by theme, and Calendar
gathers the reminders the talks settled. The README says what works and
what's decided. It is built the way Glyph and Raven are: a Rust server on
Postgres with the studio's sync engine (`code/kit/sync`), a React app
(`apps/web`) that keeps a local copy of the workspace, and a Tauri desktop
shell (`apps/desktop`). When something here works like Forge, Glyph or
Raven, their code is the reference; keep them alike.

## Shared design and brand

Read [`studio/design/principles.md`](../../../studio/design/principles.md) before any UI
change and [`studio/design/behaviour.md`](../../../studio/design/behaviour.md) before
changing what Almanac does. Use the `ui-*` classes of
[`studio/design/components.css`](../../../studio/design/components.css) and token-backed
Tailwind classes; `studio/design/lint` fails on literal colours, sizes or radii.
The storyboards in `storyboards/` are the agreed look: check a page
against them before changing it.

The web app bundles snapshots of the tokens, `components.css`, the fonts
and the Almanac mark from [`studio/brand/mark/almanac.svg`](../../../studio/brand/mark/almanac.svg).
Run `bun run design:sync` after changing them in the vault.

## Commands

- `bun run db`: start Postgres (Docker, port 5437). `bun run db:reset` wipes it.
- `bun run dev`: Rust server (:7780) plus web app (:5177, proxies `/api`),
  for Acme Inc: dev-id (`code/kit/dev-id`) stands in for its account and
  signs you in as anyone in its `people.toml`, on a database of its own
  (`almanac_acme`). With your own settings (`almanac.toml`, or
  `ALMANAC_OIDC_ISSUER` in `.env`) it uses those and your `almanac` database.
- `bun run dev:desktop`: the Tauri app against the running dev server.
  `bun run build:desktop` packages it. The app runs Almanac itself, on its
  own Postgres (7786) and server (7785), for the workspaces on this Mac,
  and signs in to servers (the Rust port, 7780, not Vite's) for the
  others. Its data is in `~/Library/Application Support/eu.codevillain.almanac`.
- `bun run app`: the whole app in Docker at http://localhost:7790 (one image:
  server plus built web app), with the settings in `.env`. `bun run app:logs`
  follows it.
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, generated types and design
  copies up to date, Biome, typecheck, the bun tests (the sync kit's too),
  web build. It must pass before you finish. CI runs it on every push that
  touches Almanac (`.forgejo/workflows/almanac.yml`).
  `ALMANAC_CHECK_SKIP_DESKTOP=1` leaves the desktop app out.

Rust tests need Postgres running (`bun run db`); `DATABASE_URL` comes from
`.cargo/config.toml`.

To try the pages without spending on Claude, point `claude.url`
(`ALMANAC_CLAUDE_URL` in `.env`, with any `ALMANAC_CLAUDE_API_KEY`) at a
stand-in that answers the Messages API's shapes: a streamed answer, and
JSON in the schema each request sends.

## What syncs

A workspace is a person's or a team's topics, with members and roles as in
Forge. Everything a page shows syncs: `Topic`, `Talk`, `Message`, `Point`,
`Question` and `Reminder`, and the members (`User`). Who writes what
(`crates/sync/src/rules.rs`):

- The server makes topics (`POST /api/w/{id}/topics`), talks and messages
  (`POST /api/w/{id}/topics/{topic}/ask`). Clients may change and delete a
  topic and delete a talk; deleting a topic takes what it kept with it, and
  a talk its messages.
- Clients write points and reminders, and answer questions (`status`).
- Only the server sets a reminder's `notified_for`.
- Guests see no topics: every model but `User` is hidden from them.

The Claude key is not a synced record: it's in `claude_key`, read through
`GET /api/w/{id}/claude` (its last four characters only) and set or removed
by admins (`PUT`, `DELETE`).

## Claude

`claude.rs` speaks Anthropic's Messages API over HTTP (Rust has no SDK):
`claude-opus-5-5`, adaptive thinking, the server-side refusal fallback,
web search off.

- **A talk** (`talks.rs`): each question sends the fixed prompt, the page
  as `page.rs` writes it (what the topic sticks to, its points by theme,
  its open questions, its reminders, the other topics' names) and only the
  current talk. The answer streams back as server-sent events: `talk`,
  then `text` pieces, then `done` or `problem`. A problem is also kept as a
  message, so the talk shows it after a reload.
- **Keeping**: after the answer, one more request, with structured output,
  reads the exchange for points, questions, reminder offers, the topic an
  off-topic question belongs to, and the talk's title. The server writes
  what comes back through the sync engine, so every client sees it.
- **Starting a topic** (`topics.rs`): in the background, Almanac writes what
  it sticks to when that was empty, picks an icon (`ICONS` in
  `crates/domain`), a group, the shape (`cards` or `document`) and the
  first questions. Without a key the topic keeps its defaults.

Change the prompts in the same commit as what reads their answers. A
prompt says what to do and why, in plain sentences; the schema says the
shape.

## Reminders on the Mac

`reminders.rs` looks every 30 seconds for reminders that are on, notify,
and are due (at their time, or at nine), marks them told for that date,
and sends them on `AppState::due`. The desktop app (`this_mac.rs`) turns
them into notifications. A server with no Mac listening marks nothing.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Almanac's rules for the sync engine: who writes what, the
  fields only the server sets, and what a topic or talk takes with it.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts, the
  company's sign-in (`oidc.rs`) and people (`groups.rs`, `company.rs`),
  Claude (`claude.rs`, `keys.rs`), topics, talks and the page Claude reads,
  reminders. Its settings: `config.rs`, and `almanac.example.toml`.
  `serve.rs` runs it on a listener until told to stop, for the server
  binary and the desktop app alike; `local.rs` is the one person of the
  desktop app's own Almanac, who signs in to nothing.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `layout.tsx` is the sidebar,
  `topic.tsx` a topic's page, `talk.tsx` the talk beside it,
  `calendar.tsx` Calendar, `settings.tsx` Settings → Claude. Its
  workspaces come from places (`server.ts`): this Mac, the server it came
  from, or servers the desktop app signed in to.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Almanac on Postgres for
  the workspaces on this Mac, with `code/kit/desktop`, and shows due
  reminders as notifications.
- `migrations`: SQL schema, applied by the server on start.

## Rules

- sqlx 0.9 refuses SQL built at runtime: write each statement as a `const`.
- Wire field names are snake_case. Times go out as ISO times, never
  formatted: the page shows them in the reader's time zone. Days are
  `YYYY-MM-DD`.
- Change the schema with a new numbered migration; don't edit one that
  has been committed.
- Every write to a synced model goes through `almanac_sync::apply_transaction`.
- Workspace endpoints go under `/api/w/{workspace_id}` and take the `Member`
  extractor, which checks membership. Admin-only ones call
  `require_admin()`, and asking or starting a topic `require_not_guest()`.
- The key never goes into a synced record, a log or a response.

## One company

A server runs for one company (`almanac.example.toml`): `company`,
`public_url`, its account's sign-in (`[oidc]`) and its SCIM token
(`[scim]`) are required, and a missing or unknown setting stops the server,
naming it. On its first start it makes the company's workspace
(`company.rs`). `[claude] api_key` and `url` are optional.

Nobody signs up and there are no invites or passwords: the company's account
sends its people and groups to `/api/scim/v2` (`code/kit/scim`;
`crates/server/src/groups.rs`). `almanac-users` are members, `almanac-admins`
admins and `almanac-guests` guests (other names under `[groups]`), and Members
won't change their roles. Someone who signs in and isn't in one is told to
ask the company's admin (`oidc_no_access`). Once the account has sent
groups, people in none of them leave the workspace. Tests:
`crates/server/tests/scim.rs`.

## Adding a synced model

1. A migration for its table, with `id`, `workspace_id`, `created_at` and
   `updated_at` columns.
2. In `crates/domain`: the record, create and patch types, a `ModelKind`
   variant, and the `match` arms the compiler asks for.
3. In `crates/sync/src/rules.rs`: who may write it, and any fields only the
   server may set. In `crates/domain`'s `guest_view`: what a guest sees of
   it.
4. `bun run gen:types`, then add it to `Schema` in `apps/web/src/sync.ts`.
