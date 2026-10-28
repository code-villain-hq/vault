# Almanac: notes for coding agents

Almanac keeps what you learn by topic. You start a topic with a name and
what it sticks to, or tell the workspace's own topic, Almanac, what you
want to keep track of and start the topic it proposes; you ask a topic
things in a talk beside its page; the page keeps points with their
sources, grouped by theme, and Calendar gathers the reminders the talks
settled. The README says what works and what's decided. It is built the
way Glyph and Raven are: a Rust server on Postgres with the studio's sync
engine (`code/kit/sync`), a React app (`apps/web`) that keeps a local copy
of the workspace, and a Tauri desktop shell (`apps/desktop`). When something here works like Forge, Glyph or
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
JSON in the schema each request sends. For Claude Code on this Mac,
`crates/server/tests/claude-code.ts` stands in for its ACP adapter.

## What syncs

A workspace is a person's or a team's topics, with members and roles as in
Forge. Everything a page shows syncs: `Topic`, `Talk`, `Message`, `Point`,
`Question` and `Reminder`, and the members (`User`). Who writes what
(`crates/sync/src/rules.rs`):

- The server makes topics (`POST /api/w/{id}/topics`), talks and messages
  (`POST /api/w/{id}/topics/{topic}/ask`). Clients may change and delete a
  topic and delete a talk; deleting a topic takes what it kept with it, and
  a talk its messages.
- Every workspace has one topic of its own, `kind` `almanac` (`own.rs`),
  which the server makes with the workspace. Clients may rename it and
  change what it sticks to, not delete it.
- An answer in it may carry a `proposal`, a topic to start: `POST
  /api/w/{id}/topics` with `from_message` starts it with the proposal's
  points and reminders, once. Its `changes` say what it changed in topics
  you named; `POST /api/w/{id}/messages/{message}/undo` puts them back.
- Clients write points and reminders, and answer questions (`status`).
- Only the server sets a reminder's `notified_for`.
- Guests see no topics: every model but `User` is hidden from them.
- A question's photos come with the ask (`photos`, base64, up to six of
  5 MB) and stay in `photos`, which doesn't sync: the message names them
  (id and size) and the app fetches each one (`GET
  /api/w/{id}/photos/{photo}`, `photos.rs`). They go with their message.

Settings → Claude isn't synced. `GET /api/w/{id}/claude` says who answers:
on this Mac Claude Code (`via` `claude_code`, with whether it's ready),
and whether it may search the web (`claude_choices`, `PUT .../claude/web`
by admins); on a server the key, in `claude_key`, its last four characters
only, set or removed by admins (`PUT`, `DELETE`). On this Mac the check
starts the adapter, so the pages don't ask it on every load: only
Settings, and a talk says when Claude Code can't answer.

## Claude

Two ways to the same answers, and the same prompts:

- **On this Mac, Claude Code** (`acp.rs`), as Layer's conversations run:
  for each question Almanac starts `claude-agent-acp` from the login
  shell's PATH, in an empty folder of its own, with Almanac's prompt, none
  of the user's Claude Code settings, and none of its tools but WebSearch
  and WebFetch (when the web is on). Almanac's own tools come over MCP
  (`mcp.rs`, `POST /api/mcp`), behind a token that lasts as long as the
  question. It answers as the user is signed in to Claude Code: no key.
  A start takes a few seconds; one that hasn't opened a session in 30 is
  stopped and made again, once, with what it last printed in the log.
- **On a server, the Messages API** (`claude.rs`, with the key; Rust has no
  SDK): `claude-opus-5-5`, adaptive thinking, the server-side refusal
  fallback, web search off.

What they do:

- **A talk** (`talks.rs`): each question sends the fixed prompt, the page
  as `page.rs` writes it (what the topic sticks to, its points by theme,
  its open questions, its reminders, the other topics' names) and only the
  current talk. The answer streams back as server-sent events: `talk`,
  then `text` pieces, then `done` or `problem`. A problem is also kept as a
  message, so the talk shows it after a reload.
  The talk's photos go with every question in it, after the text.
- **Keeping** (`kept.rs`): points, questions, reminder offers, the topic an
  off-topic question belongs to, and the talk's title. Claude Code hands
  them over with tools while it answers (keep_point, ask_to_check,
  offer_reminder, fits_elsewhere, name_talk); through the API one more
  request, with structured output, reads them from the exchange. Either
  way the server writes them with the answer, through the sync engine, so
  every client sees it.
- **The workspace's own topic** (`own.rs`) reads every topic's name, group
  and what it sticks to, and has other tools: propose_topic and
  change_topic, not keep_point. A proposal waits for Start; a change is
  made at once and kept on the message for Undo.
- **Starting a topic** (`topics.rs`): in the background, Almanac writes what
  it sticks to when that was empty, picks an icon (`ICONS` in
  `crates/domain`), a group, the shape (`cards` or `document`) and the
  first questions (set_up_topic, or structured output). When Claude can't
  answer, the topic keeps its defaults.

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
  Claude (`acp.rs` and `mcp.rs` on this Mac, `claude.rs` with a key,
  `keys.rs`), topics, the workspace's own (`own.rs`), talks, what they keep
  (`kept.rs`) and the page Claude reads, reminders. Its settings:
  `config.rs`, and `almanac.example.toml`. `serve.rs` runs it on a listener until told to stop, for the server
  binary and the desktop app alike; `local.rs` is the one person of the
  desktop app's own Almanac, who signs in to nothing.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `layout.tsx` is the sidebar,
  `topic.tsx` a topic's page, `talk.tsx` the talk beside it,
  `asking.tsx` the answers on their way and each topic's open talk (above
  the pages, so they last while you look elsewhere), `calendar.tsx`
  Calendar, `settings.tsx` Settings → Claude. Its workspaces come from
  places (`server.ts`): this Mac, the server it came from, or servers the
  desktop app signed in to.
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
- Claude Code gets no tool that reads or writes the Mac: only the web's,
  and Almanac's own over MCP.

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
