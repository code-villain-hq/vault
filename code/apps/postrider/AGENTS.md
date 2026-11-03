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
sidebar, the list, the mail. Esc closes it, J and K or ↓ and ↑ move. A narrow window
shows the list or the mail, with a way back. The desktop app notifies of
new mail and counts what's unread on the dock. Step 2 is built too: the
field at the foot of the list, where you ask Postrider something or tell
it how to sort, and it makes the rule on the mail server, or sorts by
reading on this Mac what no server rule can tell, says what it did, and
Undo puts it back. Step 3 is built: C writes a new mail, and "More
professional" writes yours again before you send it. Step 4 is built:
newsletters skip the Inbox for a page of their own (G then N), the ones
you never open first, and Unsubscribe asks nothing. The week (5) is still
to come.

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
the server. `message.rs` reads the source: who it went to, its text, its
HTML with inline pictures written in, its attachments. The page shows the
HTML in a frame where no script runs and pictures load without a referrer.
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
height in 2 seconds; a mail with neither HTML nor text says so. Each mail
opened logs how it's built, never its words (`message::structure`, at
debug; a warning when it has nothing to show).

Only the Inbox is followed and kept, and what conversations need of Sent
and Archive (below). The other folders are read when
they're shown (`folders.rs`): `GET /api/w/{id}/folders` lists every
account's folders, each once by its role (Sent, Archive, Drafts, Trash,
Junk) or its name, with what's unread; `/folders/{key}` is a folder's
newest mail; `/search?q=` asks every folder but Drafts, Trash and Junk for
the mail with every word in it (IMAP `TEXT`). Before the servers answer,
`/search?q=&here=true` finds the words in what this Mac has already, the
Inbox's copy and the lines the lists read, by sender, subject and start;
the page shows that at once, with "Asking the servers…" under it. An
account that can't be asked shows what this Mac has of it. A mail opened from them is
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
  opens; from the Inbox, all of a conversation's Inbox mail goes. Mail in
  Trash stays there; its server empties it.
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
- C writes a new mail (`POST /mails`, `reply.rs`; the sidebar's pen and
  ⌘K too). To suggests people who wrote to you; picking one sends it from
  the account they wrote to, unless you pick another, and else from the
  account the last new mail went from. Commas or semicolons part
  addresses, and one that isn't whole is named.
- More professional (`rewrite.rs`, `POST /rewrite`), beside Send in a
  reply and a new mail, has Claude Code on this Mac write yours again, in
  your language, with every fact kept and no tools. A reply's mail comes
  along for how formally you speak, as a stranger's words. Back to yours,
  or ⌘Z, puts yours back; writing again forgets it.
- A mail's files (`files.rs`) come by their place among its attachments.
  The desktop app saves one in Downloads and opens it
  (`apps/desktop/src-tauri/src/files.rs`); the browser downloads it.

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
in their new folder), to the folder each came from. The prompt names the accounts,
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
first lines, twenty at a time, with no tools. Is it about something of
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
keeps every name it wrote as): when each last wrote, how often it writes
(`rhythm.ts`) and how much of it you opened, the least read first. Each
column's head sorts by it, clicked again the other way, and the order is
kept on this device (`localStorage`). / finds a sender by any of its
names or its address; a find with no sender says so. It leads with the
ones you never open that wrote in the last month, and a button for all
of them, but not while you find one. The server keeps the headers it
read (`KeptHeaders`, by UIDVALIDITY), so each read fetches only what came
since. Reading the page is looking at it: it notes Newsletters' UIDNEXT
(`looked_uid` in `newsletters_set_aside`), and the sidebar counts the
unread newsletters that came after (`unseen_since`); when there are, the
open page reads again. It reads once otherwise, except while an
account's newsletters are still being set aside (`setting_aside`): then
it says so and reads every 5 seconds until they are. When setting them
aside or sorting them again failed, it says why (kept in memory,
`set_aside_failed`, as `setting_aside` or `not_sorted`); the account
tries again when it next connects. The page is as wide as the window.
Below its senders, one muted line names those the rule names (`named`),
and that Keep in Inbox takes one off.

A sender's name, or Enter, opens what it sent (`SenderList`, at
`/newsletters/{address}`): every account's Newsletters asked for that
sender (`GET /newsletters/sender?address=`, IMAP `FROM`), in the list
column. A mail opened from it keeps `sender` in its search, so Esc goes
back to the sender's list, and Esc there back to the page. With none of
its mail open, the reading pane (`SenderPane`, `sender.tsx`) says how much
of the list you opened and leads with Unsubscribe, Keep in Inbox beside
it; U and I do the same from the list, a mail open or not, and the pane
then shows what it did, with Undo. What was done to a sender is kept in
memory by its address, with the name it had, since its mail leaves.

- Unsubscribe (`POST /newsletters/unsubscribe`, U) asks nothing. It
  archives what the sender sent at once, and the request goes 10 seconds
  later, so Undo (`POST /newsletters/undo/{id}`) really undoes it: the
  one-click POST of RFC 8058 (no redirects followed), else a mail to its
  `mailto:` over the account's SMTP. A sender with only a page to
  unsubscribe on gets its page opened when it's the one unsubscribed, and
  a link to it when it's among several. `GET /newsletters/unsubscribed/{id}`
  waits for the requests and says which failed.
- Keep in Inbox (`POST /newsletters/keep`, I), beyond the storyboard, writes
  a rule that keeps the sender's mail in the Inbox, ahead of the
  newsletters' rule, and moves its mail back there.

Undo writes the rules back as they were and moves back the mail it moved.
Like the ask field's, what it needs is kept in memory.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Postrider's rules for the sync engine. On this Mac the
  workspace is yours alone, so they only put a new account after the
  others. Accounts and the calendar are made by the server, after it
  signs in (`POST /accounts`, `/calendar`), never by the web app.
- `crates/server`: axum server: the API, the sync feed, IMAP (`imap.rs`,
  `mail.rs`, the folders and search in `folders.rs`, conversations in
  `conversations.rs`), reading a mail
  (`message.rs`, its list line in `preview.rs`, without the `[1]` a
  text part numbers its links with), archiving, replying and
  files (`archive.rs`, `reply.rs`, `files.rs`), More professional
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
  opened mail, `compose.tsx` a reply and a new mail, `newsletters.tsx` the
  Newsletters, `ask.tsx` the ask field and its
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
