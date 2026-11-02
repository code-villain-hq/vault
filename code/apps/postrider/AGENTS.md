# Postrider: notes for coding agents

Postrider is the studio's mail and calendar app: one Inbox for every
account, read over IMAP, with today's events from a CalDAV server (Radicale)
above it. [`storyboards/a-day-with-your-mail.html`](storyboards/a-day-with-your-mail.html)
is the approved storyboard; build from it, a step at a time, not beyond it.
Step 1 is built: the unified Inbox, today, Sent, Archive, Drafts, Junk,
Trash and the other folders in the sidebar, Search mail (/), Settings →
Accounts, and on a mail E to archive it, ⌫ to delete it, V to move it,
R to reply and its files to open. A folder's name in the top bar renames
it. A mail opens beside its list, in three columns: the
sidebar, the list, the mail. Esc closes it, J and K move. A narrow window
shows the list or the mail, with a way back. The desktop app notifies of
new mail and counts what's unread on the dock. Step 2 is built too: the
field at the foot of the list, where you ask Postrider something or tell
it how to sort, and it makes the rule on the mail server, or sorts by
reading on this Mac what no server rule can tell, says what it did, and
Undo puts it back. Writing a new mail and "More professional"
(3), newsletters (4) and the week (5) are still to come.

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
  IMAP (3143), Radicale for CalDAV (5232) and Mailpit for SMTP (3025),
  which keeps what's sent and sends nothing on (read it at
  http://127.0.0.1:3026). They take any user name with the password
  `dev`. `bun run db:reset` wipes them.
- `bun run seed`: the storyboard's morning: two accounts (Code Villain and
  Lazarski) with ten mails (one with a PDF) and a few folders (Sent,
  Archive, Junk, Trash, Rechnungen…), and their calendars with today's
  events. A folder that has mail isn't filled again.
- `bun run dev`: the Rust server (:7880) plus the web app (:5178, proxies
  `/api`), for the person on this Mac, as the desktop app runs it. The
  server writes this launch's session to `.dev-session` and Vite signs
  the browser's requests with it. `POSTRIDER_DEV_PASSWORD` (`dev` unless
  set) is every account's password, so nothing goes to the keychain; set
  it empty to use the keychain with your own accounts. Claude Code on this
  Mac answers the ask field, as in the desktop app (`claude-agent-acp`
  from `npm install -g @agentclientprotocol/claude-agent-acp`, signed in).
  Dovecot runs Sieve on mail it delivers over LMTP and takes scripts over
  ManageSieve (4190); IMAP's APPEND, as `bun run seed` fills folders, runs
  no rules.
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
(`Calendar`, with whose colour each of its calendars wears). Passwords
never sync and never touch Postgres: the server signs in once before it
keeps an account, then puts the password in the macOS keychain under
"Postrider", by address (`secrets.rs`).

Mail is read live. For each account the server keeps an IMAP connection
open (`mail.rs`): it copies the Inbox's summaries, waits with IDLE, and
follows new mail, expunges and flags. The web app long-polls
`GET /api/w/{id}/inbox?after=<version>`, so new mail shows within a second.
An account's state (`connecting`, `online`, `offline`, `refused`) comes
with it. Offline it tries again, waiting longer each time; refused
(a wrong password) it waits until the account is connected again.

The sync button at the sidebar's foot asks every account now
(`POST /api/w/{id}/mail/sync`, `Mail::sync`): a watcher in IDLE or between
tries looks at once, a refused one tries again, and the answer comes once
each has asked its server, or after 30 seconds. Meanwhile the status
reads "Syncing…"; then the page reads the folders, the list shown and
today again. A woken connection that doesn't end IDLE within 20 seconds
is taken as gone, as one the Mac slept through is, and opened again.

A mail opens with `GET /api/w/{id}/mails/{account}/{uid}`: the server
fetches its source over IMAP once and keeps it (`mail_sources`, gone with
its line in the Inbox), so it opens again offline, and marks it read on
the server. `message.rs` reads the source: who it went to, its text, its
HTML with inline pictures written in, its attachments. The page shows the
HTML in a frame where no script runs and pictures load without a referrer.

Only the Inbox is followed and kept. The other folders are read when
they're shown (`folders.rs`): `GET /api/w/{id}/folders` lists every
account's folders, each once by its role (Sent, Archive, Drafts, Trash,
Junk) or its name, with what's unread; `/folders/{key}` is a folder's
newest mail; `/search?q=` asks every folder but Drafts, Trash and Junk for
the mail with every word in it (IMAP `TEXT`). A mail opened from them is
`/mails/{account}/{uid}?folder=<key>`, read from its server each time.
What the lists showed is kept in memory, so a folder opened again reads
only its new mail. An account keeps up to three connections for this
(`READERS`), beside its watcher's, and a search reads that many of its
folders at once. The sidebar asks for the folders again when the Inbox
changes, when a folder or unread mail is opened, and every minute while
the app shows; the folder shown reads its mail again with it.

Today is read live too: `GET /api/w/{id}/day?from&to` asks the CalDAV
server for every calendar's events (`caldav.rs`). A calendar wears the hue
of the account Settings picked for it (`Calendar.accounts`, by its href),
else of the account it's named after, else of the one at the server's
domain. Radicale names a calendar without a name by its path, so Settings
lists them (`GET /api/w/{id}/calendars`) with their next event to tell
them by. The last day read is kept (`kept_days`): when the server can't be
reached, the page shows it, "Offline · as of 10:52".

## What changes mail

Every change happens on the account's server; the Inbox follows it there.

- E archives and ⌫ deletes (`archive.rs`): the mail moves to its
  account's Archive or Trash (made when there's none) and the next one
  opens. Mail in Trash stays there; its server empties it.
- V moves it (`move.tsx`, `POST /mails/{account}/{uid}/move` with `to`):
  to the Inbox, Archive, Junk or a folder of yours, picked by typing part
  of its name, or to a new one made with that name.
- A folder's name in the top bar renames it (`folders.tsx`,
  `POST /folders/{key}/rename`): in every account that has it, with the
  rules that sort into it, Sieve's and reading ones. Sent, Archive and the
  other role folders keep their names.
- R replies (`reply.rs`, `compose.tsx`): to the mail's Reply-To, else its
  sender, from the account it came to, in its thread, quoting it. It goes
  over SMTP: the IMAP server with `imap.` read as `smtp.`, on 465 with
  TLS, with the same password; the stand-ins at 127.0.0.1 use Mailpit on
  3025. A copy goes to Sent and the mail is marked answered. Esc keeps
  what you wrote, in memory, until the page reloads; ⌘Enter sends.
- A mail's files (`files.rs`) come by their place among its attachments.
  The desktop app saves one in Downloads and opens it
  (`apps/desktop/src-tauri/src/files.rs`); the browser downloads it.

New unread mail is a notification in the desktop app (more than three at
once are one), through `code/kit/notify`, and the dock badge counts the
Inbox's unread mail (`mail.tsx`, `platform.ts`). What a server has when its
account comes online doesn't notify.

## Asking Postrider, and sorting

The field at the foot of the list (`ask.tsx`) sends what you typed to
`POST /api/w/{id}/ask` (`ask.rs`). Claude Code on this Mac answers, as
you're signed in to it, through `code/kit/claude`, in an empty folder of
its own and with Postrider's tools only, reached at `/api/ai/mcp` with a
token that lasts the one question: `find_senders`, `find_mail`,
`read_mail`, `sort_into`, `sort_by_reading`, `stop_sorting` and
`rename_folder`. What the field can do, the page can do too: V moves a
mail, a folder's name renames it. The prompt names the accounts,
their folders and their rules, so "Dependabot too" adds to the rule it
means. One question at a time; there's no talk to scroll back.
Not installed or signed out, the answer says what to do.

Rules live on each account's mail server, in a Sieve script named
`postrider` (`rules.rs`, over ManageSieve in `sieve.rs`), so the server
sorts while the Mac is shut and every mail app sees the same folders.
Each rule is written with a `# rule {…}` comment, which is how Postrider
reads its rules back; nothing about them is kept on the Mac. IMAP names a
folder in modified UTF-7 and Sieve in UTF-8, so `fileinto` gets the
folder's name decoded (`imap::folder_name`), or mail for "Gebühren"
lands in the Inbox. When the
account ran a script of its own, `postrider-and-yours` runs that one,
then Postrider's. A rule is a folder and what goes there: from a sender,
to an address, words in the subject, a mailing list. `sort_into` writes
it in every account that has such mail, makes the folder where it's
missing, and moves what the Inbox has now with `UID MOVE`.

The answer says what changed in words, with the names in bold, the
rule's chips, Undo and the folder to open. Undo
(`POST /answers/{answer}/undo`) writes each account's rules back as they
were, moves the mail back to the Inbox, and takes away a folder it made
once it's empty again. What Undo needs is kept in memory, so it lasts
until the server stops.

Sorting that needs reading a mail, "anything that looks like an invoice",
is no rule a server can run. `sort_by_reading` keeps it on this Mac
instead (`reading_rules`: a folder and, in words, what its mail looks
like), moves the Inbox mail the answer found already, and the answer says
it sorts only while Postrider is open. The sorter (`reading.rs`, started
by `serve.rs` with Claude Code) wakes when an Inbox changes, takes the mail
that came since the rule was made (its INTERNALDATE) and hasn't been read
for it, ten at a time, and asks Claude Code with no tools at all, since a
mail is a stranger's words: lines like `3: 1` put mail 3 in folder 1. It
moves those with `UID MOVE` and keeps every mail it read in
`reading_read`, by Message-ID, so one you move back stays in the Inbox.
When Claude Code or a server fails, it tries again five minutes later.
`stop_sorting` takes reading rules away with the whole rule, or alone
with `reading`; Undo puts them back.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Postrider's rules for the sync engine. On this Mac the
  workspace is yours alone, so they only put a new account after the
  others. Accounts and the calendar are made by the server, after it
  signs in (`POST /accounts`, `/calendar`), never by the web app.
- `crates/server`: axum server: the API, the sync feed, IMAP (`imap.rs`,
  `mail.rs`, the folders and search in `folders.rs`), reading a mail
  (`message.rs`, its list line in `preview.rs`), archiving, replying and
  files (`archive.rs`, `reply.rs`, `files.rs`), CalDAV (`caldav.rs`),
  connecting accounts (`connect.rs`), the keychain (`secrets.rs`), the
  ask field's answers (`ask.rs`), sorting rules (`rules.rs`, written
  over ManageSieve by `sieve.rs`) and sorting by reading (`reading.rs`).
  `local.rs` is the one person on this Mac, who signs in to nothing.
  `serve.rs` runs it until told to stop, for `main.rs` (development) and
  the desktop app alike.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `mailbox.tsx` is the list
  column beside the open mail, `inbox.tsx` the Inbox and today,
  `folders.tsx` a folder, a search and the search field, `message.tsx` an
  opened mail, `compose.tsx` a reply, `ask.tsx` the ask field and its
  answer, `settings.tsx` the accounts and calendars, `mail.tsx` the live
  Inbox, notifications and the folder list.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Postrider on Postgres
  for this Mac, with `code/kit/desktop`, and Claude Code to answer; `main.rs` notifies with
  `code/kit/notify`; `files.rs` opens a mail's files.
- `migrations`: SQL schema, applied by the server on start. They're
  embedded in `crates/sync` when it builds; its `build.rs` rebuilds it
  when one is added.
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
