# Postrider: notes for coding agents

Postrider is the studio's mail and calendar app: one Inbox for every
account, read over IMAP, with today's events from a CalDAV server (Radicale)
above it. [`storyboards/a-day-with-your-mail.html`](storyboards/a-day-with-your-mail.html)
is the approved storyboard; build from it, a step at a time, not beyond it.
Step 1 is built: the unified Inbox, today, and Settings → Accounts.
Mails don't open yet, and composing, Sieve rules, newsletters, the ask
field and the week view are still to come.

It is built the way Raven and Almanac are: a Rust server on Postgres with
the studio's sync engine (`code/kit/sync`), a React app (`apps/web`) on
`code/kit/web`, and a Tauri desktop shell (`apps/desktop`). Unlike them it
runs only on this Mac for now: no company server, sign-in, SCIM or
settings file. When something here works like Raven or Almanac, their code
is the reference; keep them alike.

## Shared design and brand

Read [`studio/design/principles.md`](../../../studio/design/principles.md) before any UI
change and [`studio/design/behaviour.md`](../../../studio/design/behaviour.md) before
changing what Postrider does. Use the `ui-*` classes of
[`studio/design/components.css`](../../../studio/design/components.css) and token-backed
Tailwind classes; `studio/design/lint` fails on literal colours, sizes or radii.
Each account wears one data hue (`--data-1` … `--data-6`, its `hue`) on its
senders, its events and itself; status colours go only where something
needs a look.

The web app bundles snapshots of the tokens, `components.css`, the fonts
and the Postrider mark from [`studio/brand/mark/postrider.svg`](../../../studio/brand/mark/postrider.svg).
Run `bun run design:sync` after changing them in the vault.

## Commands

- `bun run db`: Postgres (5438) and the stand-ins in Docker: Dovecot for
  IMAP (3143) and Radicale for CalDAV (5232). Both take any user name with
  the password `dev`. `bun run db:reset` wipes them.
- `bun run seed`: the storyboard's morning: two accounts (Code Villain and
  Lazarski) with ten mails, and their calendars with today's events.
- `bun run dev`: the Rust server (:7880) plus the web app (:5178, proxies
  `/api`), for the person on this Mac, as the desktop app runs it. The
  server writes this launch's session to `.dev-session` and Vite signs
  the browser's requests with it. `POSTRIDER_DEV_PASSWORD` (`dev` unless
  set) is every account's password, so nothing goes to the keychain; set
  it empty to use the keychain with your own accounts.
- `bun run dev:desktop`: the Tauri app on the Vite dev server.
  `bun run build:desktop` packages it. The app runs Postrider on its own
  Postgres (7886) and server (7885). Its data is in
  `~/Library/Application Support/eu.codevillain.postrider`.
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, generated types and design
  copies up to date, Vite+ (Oxfmt, Oxlint, typecheck), the bun tests (the
  kits' too), web build. It must pass before you finish. CI runs it on
  every push that touches Postrider (`.forgejo/workflows/postrider.yml`).
  `POSTRIDER_CHECK_SKIP_DESKTOP=1` leaves the desktop app out.

## What syncs, and what's read live

A workspace syncs its settings: the mail accounts (`MailAccount`: name,
address, IMAP server, user name, hue) and the calendar server
(`Calendar`). Passwords never sync and never touch Postgres: the server
signs in once before it keeps an account, then puts the password in the
macOS keychain under "Postrider", by address (`secrets.rs`).

Mail is read live. For each account the server keeps an IMAP connection
open (`mail.rs`): it copies the Inbox's summaries, waits with IDLE, and
follows new mail, expunges and flags. The web app long-polls
`GET /api/w/{id}/inbox?after=<version>`, so new mail shows within a second.
An account's state (`connecting`, `online`, `offline`, `refused`) comes
with it. Offline it tries again, waiting longer each time; refused
(a wrong password) it waits until the account is connected again.

Today is read live too: `GET /api/w/{id}/day?from&to` asks the CalDAV
server for every calendar's events (`caldav.rs`). A calendar wears the hue
of the account it's named after.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Postrider's rules for the sync engine. On this Mac the
  workspace is yours alone, so they only put a new account after the
  others. Accounts and the calendar are made by the server, after it
  signs in (`POST /accounts`, `/calendar`), never by the web app.
- `crates/server`: axum server: the API, the sync feed, IMAP (`imap.rs`,
  `mail.rs`), CalDAV (`caldav.rs`), connecting accounts (`connect.rs`) and
  the keychain (`secrets.rs`). `local.rs` is the one person on this Mac,
  who signs in to nothing. `serve.rs` runs it until told to stop, for
  `main.rs` (development) and the desktop app alike.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `inbox.tsx` is the Inbox and
  today, `settings.tsx` the accounts, `mail.tsx` the live Inbox.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Postrider on Postgres
  for this Mac, with `code/kit/desktop`.
- `migrations`: SQL schema, applied by the server on start.
- `dev`: the stand-ins' settings.

## Rules

- Passwords go to the keychain, never to the repository, a settings file,
  Postgres or a log. The vault is public.
- Wire field names are snake_case. Times go out as ISO times, never
  formatted: the page shows them in the reader's time zone.
- Change the schema with a new numbered migration; don't edit one that
  has been committed.
- Every write to a synced model goes through `postrider_sync::apply_transaction`.
- Workspace endpoints go under `/api/w/{workspace_id}` and take the
  `Member` extractor.
- What a server says when it fails reaches the page as a sentence
  (`Failure::message`): name what's wrong in plain words.
