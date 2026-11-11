# Postrider: notes for coding agents

Postrider is the studio's mail and calendar app: one Inbox for every
account, read over IMAP, with today's events from a CalDAV server (Radicale)
above it. [`storyboards/a-day-with-your-mail.html`](storyboards/a-day-with-your-mail.html)
is the approved storyboard; build from it, a step at a time, not beyond it.
Step 1 is built: the unified Inbox, today, Sent, Archive, Drafts, Junk,
Trash and the other folders in the sidebar, Search mail (/), All, Unread
and Read on every list (F), Settings → Accounts, and on a mail E to archive it, ⌫ to delete it, V to move it,
R to reply, A to reply to all, F to forward, ⇧U to mark it unread
(or read), S to flag it, and its files to open. Flagged, in the
sidebar, lists every account's flagged mail. A folder's name in the top bar renames
it; a right click on a sidebar item, or ⇧F10 on it, marks it all read,
renames or deletes it, or empties Trash and Junk; one on a mail in a list, or
⇧F10 on its row, replies, marks, archives, moves or deletes it. A mail opens beside its list, in three columns: the
sidebar, the list, the mail. Esc closes it, J and K or ↓ and ↑ move. A narrow window
shows the list or the mail, with a way back. The desktop app notifies of
new mail and counts what's unread on the dock. Step 2 is built too: the
field at the foot of the list, where you ask Postrider something or tell
it how to sort, and it makes the rule on the mail server, or sorts by
reading on this Mac what no server rule can tell, says what it did, and
Undo puts it back. Step 3 is built: C writes a new mail, and "More
professional" writes yours again before you send it. Writing has its
basics too, asked for beyond the storyboard: Cc and Bcc, files, and
drafts kept in the account's Drafts. Step 4 is built:
newsletters skip the Inbox for a page of their own (G then N), the ones
you never open first, and Unsubscribe asks nothing. The week (5) is still
to come. Parcels (G then P), asked for beyond the storyboard, lists the
tracking numbers your mail gives and the orders a shop says are on their
way (below).

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
  Lazarski) with a dozen mails (one with a PDF, two about parcels) and a few folders (Sent,
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
"Postrider", by address (`secrets.rs`). A build signed ad hoc, as
`bun run build:desktop` signs it, is new to the keychain: its first start
waits, connecting, until the person has typed their Mac's password once
for each account and the calendar.

Quitting takes the window away first (`main.rs`), then stops the server,
whose Inbox long polls answer at once (`Mail::stop`), and its Postgres.

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
the server, or offline, once it's back (`queued.rs`). Only you opening a mail marks it read: every other fetch, the
lists, snippets, conversations, judging, sorting and search, uses
`BODY.PEEK`, and Postrider opens no mail by itself. `message.rs` reads the source: who it went to, its text, its
HTML with inline pictures written in, its attachments. The page shows the
HTML in a frame where no script of the mail's runs and pictures load
without a referrer. A `<meta http-equiv="refresh">` and `<noscript>` are
taken out first, read as the browser reads them, and the page hears only
what carries the token the frame's own script was given, so a page the
frame is sent to can't press keys or open links.
The frame is as tall as the mail says it is; a last style sets the mail's
`html` and `body` to `height: auto`, since newsletters that make them 100%
tall would otherwise fold the frame to nothing. The frame's script adds that
style again once the mail is read, as a comment the mail leaves open
swallows the first, and drops a height the mail's own `style` gives them; a
mail as tall as the frame and more (`100vh`) doesn't grow it for ever. A
mail that keeps itself in a box as tall as the frame, which scrolls or
clips (Mondo Robotics' `100vh`, or `position: absolute` to every edge), is
as tall as that box's content: the box grows with the frame, and one that
doesn't is a box of the mail's own size and stays. A frame 0 tall is shown
70% of the window tall until it says more, and so is one that says no
height in 2 seconds. A mail whose HTML, once loaded, shows no text and no
picture (hidden text, a pixel, a picture that didn't load) shows its text
instead, and a mail with nothing to show says so. Each mail opened logs how it's built, never its words (`message::structure`, at
debug; a warning when it has nothing to show).

Only the Inbox is followed live. Every other folder is kept on this Mac
(`kept.rs`), so the sidebar and a folder show at once, and offline: its
name and counts (`kept_folders`), its newest 100 lines (`folder_mails`)
and the whole text of its newest mail (`folder_sources`). The keeper
(`kept::keep`, started by `serve.rs`) asks each online account for its
folders (LIST) and every folder's counts at once on one connection
(`STATUS` with MESSAGES, UNSEEN, UIDNEXT, UIDVALIDITY and HIGHESTMODSEQ, `imap::counts`;
logged at debug as counts only). It reads a folder's lines again only
when those counts changed since it last did (`stamp`): new lines whole,
the flags of those it has. Then it reads ahead the text of the newest
mail, 50 of the Inbox's and 25 of each other folder's but Drafts, Trash
and Junk, each up to 1 MB. It runs when an Inbox changes, when the page
asks for the folders (`keep_now`, as the sync button does), and every
minute. A change Postrider makes (a move, a mail read, all read, a folder
emptied, renamed or deleted) changes what's kept as soon as the server
has done it, then the keeper looks again.

While an account's server can't be reached, moving mail (E, ⌫, V, a
drag), opening it, ⇧U and S, Mark all as read and its Undo are done on this Mac's
copy at once and wait in Postgres for the server (`queued.rs`,
`queued_mails` and `queued_reads`). The watcher does them as soon as it's
back in, before it reads the Inbox, and the keeper before it reads the
folders, one replay at a time. Until then the Inbox's copy and the
keeper leave what was done here as it is. A mail moved shows in its new
folder under a UID below zero, its line and text along, until the keeper
reads that folder from the server; moved back, it's where it was, and
only what else was done to it waits. All read marks, on the server, what
was unread under the UIDNEXT this Mac last knew, so mail that came since
stays unread; mail moved there while it waited is marked with it. What
the server refuses then (the mail gone meanwhile, the folder deleted) is
let go, and the mail shows where its server has it. The account's line
above a list says how many changes wait (`AccountStatus.waiting`).
Renaming, deleting, emptying or making a folder (a new one to move to,
or Archive or Trash where the account has none yet) still need the
server.

Mail you send, and drafts you keep, wait in the outbox (`outbox.rs`,
the `outbox` table) while their servers can't be reached. A mail to send
keeps what goes over SMTP until it went, then its copy for Sent until
it's there; it shows in Sent at once under a UID below zero, its line
reading "Not sent yet" (`MailSummary.unsent`). A draft shows in Drafts
the same way, and keeping it again, sending it or trashing it takes its
place in the outbox. The keeper sends what waits after it replays
`queued.rs`, oldest first, then deletes the draft each mail came from
and marks the mail it answers answered. A mail the SMTP server refuses
goes back to Drafts; a draft the IMAP server refuses stays here. A
server that can't be reached is tried again on the keeper's next round,
not at once. A reply waiting to be sent shows in its Inbox conversation
too. What waits can't be moved or marked until it's on its server, and
counts among the changes that wait. The status at the sidebar's foot
names the accounts that are offline and how much waits, "Lazarski
offline · 1 waiting", in place of Synced (`away` on the kit's
`SyncIndicator`).

`GET /api/w/{id}/folders` lists
every account's folders, each once by its role (Sent, Archive, Drafts,
Trash, Junk) or its name, with what's unread; `/folders/{key}` is a
folder's newest mail, with each account that can't be asked named above
it. Both read only Postgres (`folders.rs`). A mail opened from a folder
is `/mails/{account}/{uid}?folder=<key>`: its kept text, else read from
its server once and kept. `/search?q=` asks every folder but Drafts,
Trash and Junk for the mail with every word in it (IMAP `TEXT`). Before
the servers answer, `/search?q=&here=true` finds the words in what this
Mac has already, the Inbox's copy, the kept lines and what searches
read, by sender, subject and start; the page shows that at once, with
"Asking the servers…" under it. An account that can't be asked shows
what this Mac has of it. An account keeps up to three connections for
this (`READERS`), beside its watcher's, and a search reads that many of
its folders at once. The sidebar asks for the folders again when the Inbox
changes, when a folder or unread mail is opened, and every minute while
the app shows; the folder shown reads its mail again with it. Every
item but Sent, Drafts and Trash shows its unread count, Newsletters too,
and none when nothing is unread.

Every list, the Inbox, a folder, a sender's mail and a search, shows all
its mail, only the unread or only the read (`shown.tsx`): All, Unread and
Read in its top bar, or F, which goes from one to the next (with a mail
open, F forwards it). Each list
keeps its own while the app runs; all searches share one. A conversation
is unread while any of its Inbox mail is, as its dot says. What a list
showed since its filter was picked stays while you're on it, so mail you
open in Unread doesn't leave under you, and J and K on an open mail go
through what the list shows. A list the filter leaves empty says so, "No
unread mail", with Show all mail. The Newsletters page does the same with
its senders: those with mail you haven't opened, or those you opened all
of; a sender something was just done to shows either way. The list's top
bar keeps room for the filter and the search field: the breadcrumb's
parent gives way first, and under 480px it names only the list.

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
Offline, moving and reading wait for it (above).

- E archives and ⌫ deletes (`archive.rs`): the mail moves to its
  account's Archive or Trash (made when there's none), and the list comes
  back on the next one, unopened; from the Inbox, all of a conversation's
  Inbox mail goes. Mail in
  Trash stays there, until its server or Empty Trash… empties it.
- V moves it (`move.tsx`, `POST /mails/{account}/{uid}/move` with `to`):
  to the Inbox, Archive, Junk or a folder of yours, picked by typing part
  of its name, or to a new one made with that name.
- A row of a list dragged onto the sidebar (`drag.tsx`) moves there the
  same way (`useMoveMail` in `move.tsx`, which E, ⌫ and V share): onto
  the Inbox, Archive, Junk, Trash or a folder of yours, never the one
  it's in; from the Inbox, its conversation's Inbox mail. An open mail
  dragged away closes, as E closes it. The desktop window leaves drops to
  the page (`dragDropEnabled: false` in `tauri.conf.json`, or WebKit's
  drag never reaches it), and a file dropped on the window does nothing.
- A folder's name in the top bar renames it (`folders.tsx`,
  `POST /folders/{key}/rename`): in every account that has it, with the
  rules that sort into it, Sieve's and reading ones. Sent, Archive and the
  other role folders keep their names.
- Every row of a list has a menu (`mail-menu.tsx`; a right click, or
  ⇧F10 or the menu key on the focused row): Reply, Reply all and Forward
  open the mail and start the answer there; Mark as read or unread, Flag
  or Unflag, Archive, Move to… and Delete are done from the list, on a
  conversation's Inbox mail all together, as the keys beside them do on
  an open mail. A draft has no answers, Archive and Trash don't offer
  themselves, and mail in the outbox is greyed until it's on its server.
- Every sidebar item has a menu (`folder-menu.tsx`; a right click, or
  ⇧F10 or the menu key on the focused item) with what that item can do,
  greyed where it can't now: Mark all as read with nothing unread. Mark
  all as read (`POST /folders/{key}/read`), on every item, sets
  \Seen with one `UID STORE` per account (in chunks of 5,000), and the
  Inbox's copy with it; Undo, where the count was for 30 seconds,
  (`POST /marked/{id}/undo`) unsets it on the same UIDs, unless the
  folder's UIDVALIDITY changed; before a queued one reached the server,
  it takes it back here. On Newsletters it takes the whole folder,
  so its page counts all of it opened. Rename… opens the top bar's field.
  Delete folder… (`DELETE /folders/{key}`, your own folders, none with
  folders in it) moves its mail to the Trash, then deletes it in every
  account with the rules that sort into it. Empty Trash… and Empty Junk…
  (`POST /folders/{key}/empty`) delete their mail for good. Both ask
  first, Cancel focused.
- ⇧U marks a mail unread, or read, and S flags it or takes its flag
  (`marks.rs`, `marks.tsx`, `POST /mails/{account}/{uid}/mark[?folder=]`
  with `MarkMail`: `seen`, `flagged`), on a list's row and on the open
  mail, whose toolbar has both. On a conversation's row ⇧U reads all its
  unread Inbox mail, else marks its newest unread; S takes the flag off
  all its flagged mail, else flags its newest. The page shows it at
  once; the Inbox keeps that until its copy is newer than the mark
  (`mark` in `mail.tsx`), a folder's list is changed in place
  (`markListed` in `folders.tsx`), and a refusal puts it back. Offline it
  waits in `queued_mails` (`seen`, `flagged`), as reading does. Flagged
  (`GET /flagged`, `FlaggedList`) is the Inbox's flagged mail, opened as
  Inbox mail, and the kept lines' of the folders a search asks, newest
  first. The Inbox's copy keeps flagged mail however old, and the
  keeper's stamp counts HIGHESTMODSEQ, so a flag set elsewhere is read.
  Mail unflagged while Flagged shows stays in it until you leave. Inbox
  mail opened from Flagged shows its whole conversation, but marks and
  moves only itself. A row dropped on Flagged is flagged and stays where
  it is (`useFlagDrop` in `drag.tsx`).
- R replies (`send.rs`, `compose.tsx`): to the mail's Reply-To, else its
  sender, from the account it came to, in its thread, quoting it. A
  replies to all: to them, and in Cc to whom else the mail went to and
  copied, but your accounts' addresses. F forwards it: below what you
  write, its sender, date, subject, To and Cc, and its text (an HTML mail
  forwards as its text), with its files, each to leave out. The dialog
  waits for the mail it answers to be read, for whom it went to and its
  files. To, Cc, Bcc and Subject are yours to change; Cc and Bcc show
  when there's something in them, or once asked for. Everything goes to
  `POST /mails` (`Outgoing`, with `answers` or `forwards`, and `files`:
  `kept` by their place in the mail forwarded, or `added`), over SMTP:
  the IMAP server with `imap.` read
  as `smtp.`, on 465 with TLS, with the same password; the stand-ins at
  127.0.0.1 use Mailpit on 3025. Bcc goes only in the envelope; the copy
  in Sent keeps it. The mail answered is marked answered. ⌘Enter sends;
  Esc closes it, kept in Drafts (below). With a mail
  open, F forwards it before the list beside it can take F for its
  filter (a capturing listener).
- What you write is kept in the account's Drafts (`drafts.rs`,
  `POST /drafts` with the `Outgoing`), 3 seconds after you stop typing,
  at once on Esc, so it outlasts a quit. Each keeping appends a new draft
  (`\Draft`, `\Seen`) with a Message-ID of its own and deletes the one
  before by its Message-ID (`DraftRef`), so Drafts holds one per mail you
  write. The page keeps typing in memory as before and keeps one draft at
  a time per mail (`keepNow` in `compose.tsx`). A draft carries what isn't
  plain to read back in `X-Postrider-Draft` (`DraftNote`, base64 JSON):
  To, Cc and Bcc as you wrote them, the mail it answers or forwards, and
  how much of its text is the quote below yours; the mail sent has no
  such header and no Bcc. Its files go in it, and once kept they come
  from it (`Attach::Drafted`), not from memory. A mail opened in Drafts
  opens as the mail you write (`DraftDialog`, `GET /drafts/{account}/{uid}`),
  only your words in the text; another app's draft is all yours, To and
  Cc from its header. Sending deletes the draft for good; ⇧⌘⌫ throws it
  away to Trash (`POST /drafts/trash`); erasing it all, with no files,
  deletes it for good (`POST /drafts/delete`).
- Files go with any mail you write: the paperclip (⇧⌘A), dropped on the
  dialog, or pasted into the text. Each goes ahead alone to
  `POST /uploads` (`NewUpload`, base64, up to 25 MB; the route takes
  bodies to `UPLOAD_BODY`) and waits in the server's memory, for whom
  added it, until its mail goes or a day passes. A mail's files hold
  25 MB together, checked on the page first. Send waits until they're
  there; a mail with files needs no text.
- C writes a new mail (`POST /mails`, `send.rs`; the sidebar's pen and
  ⌘K too). To suggests people who wrote to you; picking one sends it from
  the account they wrote to, unless you pick another, and else from the
  account the last new mail went from. Commas or semicolons part
  addresses, and one that isn't whole is named.
- Each account has a signature (`MailAccount.signature`, Settings →
  Accounts, kept when you leave the field; empty signs nothing). It goes
  on as the mail is sent (`signed()` in `send_mail`), below your words
  after a "-- " line and above any quote, by the account it goes from.
  The writer shows it under the text, not in it, so drafts and More
  professional keep only your words and picking another account signs
  with its.
- Pictures from the web wait (`pictures.tsx`, `pictures.rs`): a mail's
  frame gets a policy that loads only what came in the mail (`data:`),
  so a tracking pixel can't tell its sender you opened it. When the mail
  would load something, a bar above it offers Load pictures, for that
  mail while Postrider runs, and Always from this sender, which the
  server keeps (`picture_senders`, `GET`/`POST /pictures`) and Settings
  lists to remove. Your own accounts' mail always loads them; the server
  says so on `Message.pictures` as the mail opens.
- More professional (`rewrite.rs`, `POST /rewrite`), beside Send in a
  reply and a new mail, has Claude Code on this Mac write yours again, in
  your language, with every fact kept and no tools. A reply's mail comes
  along for how formally you speak, as a stranger's words. Back to yours,
  or ⌘Z, puts yours back; writing again forgets it.
- A mail's files (`files.rs`) come by their place among its attachments.
  The desktop app saves one in Downloads, quarantined as a browser's
  download is, and opens it, or shows it in Finder when it would run
  something (`.command`, `.app`, `.webloc`…), under a name without
  direction marks (`apps/desktop/src-tauri/src/files.rs`); the browser
  downloads it.

New unread mail is a notification in the desktop app (more than three at
once are one), through `code/kit/notify`, and the dock badge counts the
Inbox's unread mail (`mail.tsx`, `platform.ts`). What a server has when its
account comes online doesn't notify.

## Conversations

The Inbox shows a row for each conversation (`conversations.rs`), with
what you sent and archived of it. Mail joins one by its header first:
Message-ID, In-Reply-To and References. Helpdesks often break that chain
and move the subject's prefix ("[Pen Store] Re:" against "Re: [Pen
Store]"), so mail also joins by its subject read without Re:, Fwd:, Sv:,
AW: and [tags] (`mail_parser::thread_name`), but only with the same other
party (the domain, or the address at a free-mail domain, `SHARED`),
within 30 days, and when the subjects differ or either answers a mail, so
a shop's alike notices stay apart. The subject alone joins nothing.

What you sent and archived is kept in Postgres (`filed_mails`), from each
account's Sent and Archive, as far back as its Inbox's oldest mail and a
month more, at most a year and the newest 1000 of each.
`conversations::keep` reads what's new when the Inbox changes and every 5
minutes, for the accounts that are online, on reader connections. A mail
kept in two places (the Inbox and Archive, or Sent twice) shows once, by
Message-ID; the log counts what's in Sent twice. The Inbox's copy keeps
In-Reply-To, References, To, Cc and Reply-To (`inbox_mails.refs` and
beside it); mail copied before that is read again once.

A row names who wrote in it but you, counts its mail, and starts as its
last mail does ("You: …"); it's ordered by its newest mail, yours too,
and unread while any of its Inbox mail is. It opens its newest Inbox mail,
and the page shows the whole conversation, oldest first: the last mail,
the one R answers and what was unread show whole, the rest a line each,
which a click unfolds. J and K move by conversation; E, ⌫ and V take all
its Inbox mail, while yours stays in Sent; R answers its last mail but
yours. A reply sent from Postrider shows in it once it's in Sent.

## Asking Postrider, and sorting

The field at the foot of the list (`ask.tsx`) sends what you typed to
`POST /api/w/{id}/ask` (`ask.rs`). Claude Code on this Mac answers, as
you're signed in to it, through `code/kit/claude`, in an empty folder of
its own and with Postrider's tools only, reached at `/api/ai/mcp` with a
token that lasts the one question: `find_senders`, `find_mail`,
`read_mail`, `move_mail`, `sort_into`, `sort_by_reading`, `stop_sorting`
and `rename_folder`. What the field can do, the page can do too, and the
other way round: V moves a mail, a folder's name renames it, and
`move_mail` deletes, archives and moves as ⌫, E and V do, a sender's mail
or the mails `find_mail` found. It asks nothing first: the Trash keeps
what's deleted, and Undo moves back only what it moved (the UIDs it got
in their new folder), to the folder each came from. But a mail is a
stranger's words: what the tools return of mail comes inside a fence the
question makes up, and once `read_mail` has read one, every tool that
changes something refuses for the rest of that question, so the answer
says what it would do and you ask again to have it done. The prompt names the accounts,
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
for it, ten at a time, each its header and the first 64 KB of the rest
in one fetch (`imap::glances`; an opened mail from `mail_sources`), and
asks Claude Code with no tools at all, since a
mail is a stranger's words: lines like `3: 1` put mail 3 in folder 1. It
moves those with `UID MOVE` and keeps every mail it read in
`reading_read`, by Message-ID, so one you move back stays in the Inbox.
When Claude Code or a server fails, it tries again five minutes later.
`stop_sorting` takes reading rules away with the whole rule, or alone
with `reading`; Undo puts them back.

## Newsletters

A newsletter is mail sent to many, and Postrider tells it by what needs
no language (`newsletters.rs`). It offers a way to unsubscribe: it
carries `List-Unsubscribe`, or a mailing service sent it
(`SERVICE_FIELDS`: SparkPost's `X-MSFBL`, SendGrid's, Amazon SES's,
Mailgun's, Mailchimp's, Mailjet's, `X-CSA-Complaints`) with "unsubscribe"
in its text (`UNSUBSCRIBE_WORDS`, English, German, Swedish; the rule reads
the text only where the server's ManageSieve offers `body`, `Sieve::can`,
and IMAP SEARCH reads it with `BODY`). And its header says it went to
many: `List-Id`, `Precedence: bulk` or `list`, a Postmark broadcast
stream (`X-PM-Message-Stream`), or Mailchimp's own `X-MC-User` (not
Mandrill's, nor SES or SendGrid alone: they send orders too). Mail an
`Auto-Submitted` other than `no` (RFC 3834) says a machine sent by itself
is a notice, unless it also says it went to many with one click to leave
or `Precedence: bulk`, as newsletters do to keep auto-replies away. The
rule's Sieve (`newsletters::test`) and the Rust that reads a header
(`told`: `Said::Many`, `Notice` or `Unsure`) say the same.

The header sorts, but doesn't decide: big senders put `List-Id` or
`Precedence: bulk` on notices about your account too (Google's "You
shared some Google Account data with …"). So Claude Code reads that mail
too, as below. In the Inbox, what its header says went to many goes
unless Claude Code finds it's yours. In Newsletters, what came in the
last week that the rule sorted there by its header is read every 5
minutes (`NEWSLETTERS_EVERY`), and the notices go back to the Inbox; the
log says how many, and by which header and sender's domain they'd come.

What offers to unsubscribe but says neither stays in the Inbox, and
Claude Code on this Mac reads it, and what the header sorted
(`judging.rs`, started by `serve.rs` beside the sorter): the sender, how it was sent, the subject and its
first lines, twenty at a time, read as the sorter reads them, with no
tools. Is it about something of
yours (an order, a delivery, a booking, a code, a sign-in, your account, a
payment, a person writing you) or sent to many? What a service has to
tell every customer is yours too: new terms or a privacy policy, a change
to your account or the service, how to delete your account, a security or
data-sharing notice. Mail that's both is yours. Lines like `3: many` say;
what it isn't sure of stays. Mail sent to many moves to Newsletters. The
verdict is kept by sender and kind of mail (`newsletter_verdicts`; the
kind is the subject in lower case, numbers left out), so the same isn't
asked twice, and each mail is judged once (`newsletter_judged`, by
Message-ID), so one you move back stays where you put it. Whether Claude
Code read it, not only looked at it, is `read`: in Newsletters only what
it read is left alone, since a header used to send mail there judged but
unread (Google's notices). A sender found
writing to many in two kinds of mail, and never about something of
yours, is named in the newsletters' rule (`Rule::senders`, `address :is
"from"` beside the header test): from then on the server sorts its mail
at once, with the Mac off, and its new kinds of mail aren't asked while
its verdicts are on record. One found writing about something of yours
comes off it again, and a sender that writes both stays with Claude Code.
Only verdicts on mail signed by the sender's own organisation count
(`proven`, `proof.rs`: Postrider checks the DKIM signature itself, with
DNS, since Uberspace leaves no Authentication-Results on most mail), so a
stranger writing a bank's address in From can't get the bank named. A
named sender with verdicts but none signed comes off the rule, and a
verdict found on unsigned mail is asked again when the sender's signed
mail of that kind comes.
When the question changes, a migration forgets what it found sent to many
and has Newsletters read again (`0017_question.sql`), so the notices it
put there go back.
Keep in Inbox wins: it takes the sender off the rule and forgets its
verdicts. Without Claude Code, such mail stays in the Inbox.

The first time an account comes online, Postrider adds a rule to its
Sieve script that files newsletters in its Newsletters folder (made where
there's none), after the account's other rules, so a rule of yours still
wins. It moves the Inbox's newsletters there too, and notes the account in
`newsletters_set_aside`, so a rule you take away stays away, with what
made a newsletter or a notice then (`sorted_by`: the rule's test in
Sieve, after `PASSES`, counted up when the passes change). When that
changes, the account's mail is sorted again by it, once (`sort_again`):
its rule is written again, the notices in Newsletters go back to the
Inbox, and the Inbox's newsletters go to Newsletters, but those from
senders kept in the Inbox, or judged already (it's where that, or you,
put it). What in Newsletters isn't a notice is marked
(`judge_again`) for Claude Code to read once, but what it read already:
the notices among it go back too. It reads all of Newsletters, but what a rule of yours sorts
there, so a notice comes back however it got there and whatever the
server's SEARCH finds; the log says how many it read, found and moved.
Change the test, and every account follows on its next start; there's
nothing else to note. Both passes subscribe the folders Postrider puts mail in (Archive, Trash, Junk,
Newsletters and those its rules sort into), since the account's other
mail apps show only subscribed ones; a folder Postrider makes is
subscribed as it's made. The folder is a role folder
(`FolderRole::Newsletters`), named "Newsletters".

The page (`newsletters.tsx`, `GET /newsletters`) reads all of each
account's Newsletters by sender, so nothing in the folder is left off it
(named as its newest mail names it, or by its organisation when that
name is none or says nothing, "Notification": `sender_name`; `names`
keeps every name it wrote as). A sender is that name at the domain its
owner registered (`sender_of`, `example.co.uk` of `news.example.co.uk` by
the public suffix list, `psl`), so the addresses it writes from are one
row (`addresses`, newest first, named in its tooltip): their counts add
up. Never by the name alone, and not at a domain many send from
(`SERVICES`, a mailing service's such as teamtailor-mail.com, or a free
mail one, `conversations::SHARED`): there each address is its own. Rows
stay one per account. The page shows when each last wrote, how often it writes
(`rhythm.ts`) and how much of it you opened, the least read first. A
sender with mail you haven't opened comes first, whatever the order, with
how much of it beside its name, as the sidebar shows what's unread in
Newsletters: both count `\Seen`, so they add up. Each column's head sorts
by it, clicked again the other way, and the order is kept on this device
(`localStorage`). / finds a sender by any of its
names or addresses; a find with no sender says so. It leads with the
ones you never open that wrote in the last month, and a button for all
of them, but not while you find one. The server keeps the headers it
read (`KeptHeaders`, by UIDVALIDITY), so each read fetches only what came
since. When what's unread in Newsletters changes, the open page reads
again (`looked_uid` in `newsletters_set_aside` is no longer used). It
reads once otherwise, except while an account's newsletters are still being set aside (`setting_aside`): then
it says so and reads every 5 seconds until they are. When setting them
aside or sorting them again failed, it says why (kept in memory,
`set_aside_failed`, as `setting_aside` or `not_sorted`); the account
tries again when it next connects. The page is as wide as the window.
Below its senders, one muted line names those the rule names (`named`),
and that Keep in Inbox takes one off.

A sender's name, or Enter, opens what it sent (`SenderList`, at
`/newsletters/{address}`): every account's Newsletters asked for that
sender, from every address it writes from (`GET
/newsletters/sender?address=`, IMAP `FROM`, `addresses_of`), in the list
column. Unsubscribe goes to each address by its own `List-Unsubscribe`,
and Keep in Inbox names them all in the rule. A mail opened from it keeps `sender` in its search, so Esc goes
back to the sender's list, and Esc there back to the page. With none of
its mail open, the reading pane (`SenderPane`, `sender.tsx`) says how much
of the list you opened and leads with Unsubscribe, Keep in Inbox beside
it; U and I do the same from the list. A mail open from Newsletters, from
the sender's list or a search, has both in its toolbar, ahead
of Reply, and U and I with them (`message.tsx`): they go to the sender's
list and do it there once it's read (`want`, `Doing.wanted`), so the pane
shows what was done, with Undo. What was done
to a sender is kept in memory by its address, with the name it had and
its rows on the page, since its mail leaves. Once the list is empty as
read since it showed (kept, unsubscribed, or its last mail archived,
deleted or moved), it goes back to the page by itself (`cameBack`): the
page is scrolled where it was, the sender's rows say what was done, with
Undo, and J/K are on the row below them (`nearest`). Should the page not
have read that sender yet, the pane shows what it did, with Undo, as
before. The page shows what it read last at once (`lastRead`), and the
sender's list, its pane and an open mail's back link name the sender as
the page does (`pageName`), else as its mail does.

- Unsubscribe (`POST /newsletters/unsubscribe`, U) asks nothing. It
  archives what the sender sent at once, and the request goes 10 seconds
  later, so Undo (`POST /newsletters/undo/{id}`) really undoes it: the
  one-click POST of RFC 8058 (no redirects followed; only to a public
  address, checked by `code/kit/net` on the address the name resolves to,
  or from the stand-ins on this Mac to it too, never to cloud metadata), else a mail to its
  `mailto:` over the account's SMTP. A sender with only a page to
  unsubscribe on gets its page opened when it's the one unsubscribed, and
  a link to it when it's among several. `GET /newsletters/unsubscribed/{id}`
  waits for the requests and says which failed.
- Keep in Inbox (`POST /newsletters/keep`, I), beyond the storyboard, writes
  a rule that keeps the sender's mail in the Inbox, ahead of the
  newsletters' rule, and moves its mail back there.

Undo writes the rules back as they were and moves back the mail it moved.
Like the ask field's, what it needs is kept in memory.

## Parcels

The Parcels page (`parcels.tsx`, `GET /parcels`, `parcels.rs`) lists the
tracking numbers in the mail this Mac keeps: the Inbox's kept sources and
the folders' (those a search asks, but Sent). Nothing asks a carrier, and
no key or account is needed. A number comes from a `ParcelDelivery` the
mail declares (schema.org JSON-LD), a link to a carrier's page with the
number in it, a link of any shop's that names it (`track_sn=…`) in a mail
about a parcel, a label in German, English, Swedish, Danish, Norwegian or
French ("Sendungsnummer:", "Tracking number", "Spårningsnummer",
"Paket-ID", "Numéro de suivi"; grouped digits joined), the word right
before it ("Package 0012…", "Paket", "Colis"), or a format only one
carrier writes (`1Z…` UPS, `JJD…` DHL, `TBA…` Amazon, a UPU number like
`RR123456785DE` whose check digit is right, its country naming the post:
DE DHL, SE and DK PostNord, NO Bring, FR Colissimo, GB Royal Mail, US
USPS) in a mail about a parcel. The carrier is the sender's when a
carrier (`CARRIERS`: the German ones, PostNord, Instabox, Budbee, Airmee,
Early Bird, Bring, InPost, Chronopost, Colissimo, Royal Mail, USPS…)
wrote the mail, else the linked page's, else the number's, else the one
the mail alone names.

A shop's order is a parcel too: the number after "Order", "Bestellung",
"Beställning", "Commande"… (anywhere in the subject, a few words after it
in the text, else a link's `orderId=…`), once the subject says it left
(shipped, on its way, på väg, versendet, expédié, delivered…); a
confirmation is not. The row says "Order 1234…" until a mail from the
same shop (its registrable domain) gives a number with that order, which
then takes its place. Two numbers for one shipment (a shop's and the
carrier's) stay two rows.

Where it is comes from the mail's words, the subject first and decisive
(a "på väg" subject is on its way, whatever the text says will happen
next): on its way, out for delivery, ready to pick up, delivered; a
parcel keeps the furthest any of its mails said, and its newest mail,
which the row names and opens. Track opens the mail's own link with the
number, else the carrier's page, else the shop's link for the order (one
with "track" in it first), else Parcels' page (parcelsapp.com), which
finds the carrier itself; an order whose mail links nowhere for it has
no Track. Mark delivered, on a row's hover (always shown without a
pointer), says a parcel came that no mail said came
(`POST /parcels/delivered`, `delivered_parcels`): by its number, else its
shop and order (`Parcel.key`), so a mark made on an order holds once its
number comes. The row then reads Delivered, at when you said so, and Not
delivered takes it back; a mail that says delivered is the last word and
has no button. Delivered parcels go 7 days after their last mail or your
mark, others after 60 without mail. What each mail gave is kept in
memory, so a mail is read once while the server runs.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Postrider's rules for the sync engine. On this Mac the
  workspace is yours alone, so they only put a new account after the
  others. Accounts and the calendar are made by the server, after it
  signs in (`POST /accounts`, `/calendar`), never by the web app.
- `crates/server`: axum server: the API, the sync feed, IMAP (`imap.rs`,
  `mail.rs`, the folders and search in `folders.rs`, what this Mac keeps
  of them in `kept.rs`, what waits for a server that can't be reached in
  `queued.rs` and what's sent or kept meanwhile in `outbox.rs`, read and flagged marks in `marks.rs`, conversations in
  `conversations.rs`), reading a mail
  (`message.rs`, its list line in `preview.rs`, without the `[1]` a
  text part numbers its links with, HTML tags or entities; a migration
  cleaned the lines kept before, `0018_snippet_tags.sql`), archiving, sending,
  drafts and files (`archive.rs`, `send.rs`, `drafts.rs`, `files.rs`), More professional
  (`rewrite.rs`), CalDAV (`caldav.rs`),
  connecting accounts (`connect.rs`), the keychain (`secrets.rs`), the
  ask field's answers (`ask.rs`), sorting rules (`rules.rs`, written
  over ManageSieve by `sieve.rs`), sorting by reading (`reading.rs`) and
  newsletters (`newsletters.rs`, read by Claude Code in `judging.rs`).
  `local.rs` is the one person on this Mac, who signs in to nothing.
  `serve.rs` runs it until told to stop, for `main.rs` (development) and
  the desktop app alike.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `mailbox.tsx` is the list
  column beside the open mail, `inbox.tsx` the Inbox and today,
  `folders.tsx` a folder, a search and the search field, `message.tsx` an
  opened mail, `compose.tsx` a reply, a forward and a new mail, `newsletters.tsx` the
  Newsletters, `ask.tsx` the ask field and its
  answer, `settings.tsx` the accounts and calendars, `pictures.tsx` held-back pictures, `mail.tsx` the live
  Inbox, notifications and the folder list.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Postrider on Postgres
  for this Mac, with `code/kit/desktop`, and Claude Code to answer; `main.rs` notifies with
  `code/kit/notify`; `files.rs` opens a mail's files. Its window's
  Content-Security-Policy is in `tauri.conf.json`. A mail's frame has it
  too, so the frame's script is allowed there by its hash
  (`REPORTER_HASH` in `message.tsx`; a test fails when they part). A new
  kind of load (another host, a worker, wasm) needs its directive there,
  or the installed app blocks it. `dev:desktop` shows Vite's page, which
  gets no policy, so try a built app.
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
