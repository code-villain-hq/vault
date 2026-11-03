# Almanac: notes for coding agents

Almanac keeps what you learn by topic. You start a topic with a name and
what it sticks to, or tell the workspace's own topic, Almanac, what you
want to keep track of and start the topic it proposes; you ask a topic
things in a talk beside its page; the page is the topic's blocks, which
keep points with their sources, and Calendar gathers the reminders the
talks settled. The README says what works and what's decided. It is built the
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
- `bun run release`, then `bun run release:publish`: a release of the
  desktop app on Forgejo and in the Homebrew tap, as Layer's
  (`scripts/release.ts`, the cask in `packaging/homebrew/almanac.rb`).
  The README says how.
- `bun run app`: the whole app in Docker at http://localhost:7790 (one image:
  server plus built web app), with the settings in `.env`. `bun run app:logs`
  follows it.
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, generated types and design
  copies up to date, Vite+ (Oxfmt, Oxlint, typecheck), the bun tests (the sync kit's too),
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
Forge. Everything a page shows syncs: `Topic`, `Block`, `Talk`, `Message`,
`Point`, `Question` and `Reminder`, and the members (`User`). Who writes what
(`crates/sync/src/rules.rs`):

- Clients make topics, offline too (`starting.ts`), with `kind` `topic`
  only; the server makes talks and messages (`POST
  /api/w/{id}/topics/{topic}/ask`). Clients may change and delete a topic
  and delete a talk; deleting a topic takes what it kept with it, and a
  talk its messages.
- Every workspace has one topic of its own, `kind` `almanac` (`own.rs`),
  which the server makes with the workspace. Clients may rename it and
  change what it sticks to, not delete it.
- An answer in it may carry a `proposal`, a topic to start: `POST
  /api/w/{id}/topics` with `from_message` starts it with the proposal's
  points and reminders, once. Its `changes` say what it changed in topics
  you named; `POST /api/w/{id}/messages/{message}/undo` puts them back.
- Clients write blocks, points and reminders, and answer questions
  (`status`). A topic's page is its blocks by `position`; a block takes
  its points with it.
- Only the server sets a reminder's `notified_for`.
- Guests see no topics: every model but `User` is hidden from them.
- A question's photos come with the ask (`photos`, base64, up to six of
  5 MB) and stay in `photos`, which doesn't sync: the message names them
  (id and size) and the app fetches each one (`GET
  /api/w/{id}/photos/{photo}`, `photos.rs`). They go with their message.
- A page's other pictures are photos with no message: a topic's `cover`,
  and a row's photo cell, dropped on or picked in `pictures.tsx`. The app
  names each one, sets it at once and keeps it in IndexedDB until `PUT
  /api/w/{id}/photos/{photo}` has it (`sending.ts`), offline too; sent
  again, it's kept once. Another device asks a while for one not sent
  yet (`usePhoto`). `POST /api/w/{id}/photos` stays for older apps. A talk keeps one from the web with
  `fetch_picture` (public https only, the page's og:image, 5 MB at most)
  and sets the cover with `set_cover`. Rows with a photo show it small on
  their line in every view.
- A picture block (`picture`) shows photos large, a point each: its photo
  in its one cell (`PICTURE_CELL`, `photo`) and its text the caption, which
  may be empty there only. Add a block's Picture (below) makes one with
  the photos picked, as does a photo dropped on Add a block; on the block,
  more are dropped or added, a caption written in place and a photo taken
  off with ✕ (`Pictures` in `pictures.tsx`). One photo takes half the
  page, more the whole. A talk adds them with add_items' `photo`, "photo
  2" or what fetch_picture answered.
- A collection is changed by hand too (`editing.tsx`): a click on a row's
  value changes it in place (in the table, on cards, on the row's page),
  and "+ New" under the collection adds a row. Both write the row's
  `cells` through the sync engine; guests and samples get neither. A
  change sends only the cells it changes (`changed_cells`, null taking one
  out, `cellsPatch`), which the sync rules merge into the row as it is
  then (`prepare_update`), so an offline device or a talk keeps what
  others changed meanwhile; `cells` beside them is only the row as this
  device shows it until the server answers.
  "Fields…" in its ⋯ (`fields.tsx`) changes its fields as change_block
  does: name, kind, a number's unit, a choice's or status's words, eight
  at most, in order. A renamed field takes its cells along; removing one
  that rows fill asks first, and takes their cells off.
- A page is written by hand too (`writing.tsx`, `adding.tsx`): Add a
  block at the end of a page (and of a row's page), Add below in a
  block's ⋯ and ⌘K's "Add a …" add a heading, paragraph, callout, list,
  checklist, table or picture. A block with nothing in it doesn't show,
  so a new one is written in place first and kept with its first words
  or row in one transaction (Esc, or leaving it empty, takes it off). A
  paragraph's Enter goes on to the next passage; a list, checklist or
  table opens on its title, then its first row, and stays on "+ New". A
  checklist starts with a "Done" check, a table with "Notes". A click on
  a passage, a heading or a callout writes in it; emptied, a passage
  goes. Hand-made points have no source. Guests get none of it.

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
- **On this Mac, ChatGPT instead**, when Settings → Claude chooses it
  (`claude_choices.answers`, `AppState::answerer`): the same `acp.rs`
  starts `codex-acp` with `CODEX_HOME` in Almanac's own folder, so none of
  the user's Codex config, plugins or hooks come along, and `CODEX_CONFIG`
  turns its shell, file edits and extras off (`CODEX_OFF`). Almanac's
  prompt replaces Codex's own through `model_instructions_file`, one file
  per question. It's signed in to once from Settings, in the browser
  (ACP `authenticate`, `chat-gpt`), apart from the user's Codex.
- **On a server, Claude Code on the asker's Mac** (`handed.rs`), in the
  desktop app: Claude Code there answers first, wherever the workspace is
  (`studio/design/behaviour.md`, "Claude"). The app asks with `here`
  (`AskRequest`, `StartTopicRequest`); the server writes the question as
  for its own Claude Code, opens its token and hands it over, as the
  `question` event of a talk or `set_up` of a new topic
  (`HandedQuestion`). The desktop app runs Claude Code on it (`here.rs`,
  `answer_here`) with this server's `/api/mcp`, and the web app posts what
  came of it to `POST /api/answered` with the same token (`Answered`). Not
  installed or signed out there, the key answers instead; with no key the
  talk says what to do. A question nobody answers in 11 minutes, or whose
  asker left, stops. Settings → Claude has "Answer with Claude Code on
  this Mac", kept in the app on that Mac (`here.ts`), on by default.
- **On a server, the Messages API** (`claude.rs`, with the key; Rust has no
  SDK): `claude-opus-5-5`, adaptive thinking, the server-side refusal
  fallback, web search off. It answers in the browser, and in the desktop
  app when Claude Code there can't or is turned off.

The other way round, other apps read Almanac (`reading.rs`,
`POST /api/mcp/read`): `list_topics`, `read_topic` and `search`, nothing
that writes. Each member makes one token of their own in Settings → Claude;
it shows once, a new one replaces it, and only its hash is kept.

What they do:

- **A talk** (`talks.rs`): each question sends the fixed prompt, the page
  as `page.rs` writes it (what the topic sticks to, its blocks in order
  with their points by theme, its open questions, its reminders, the other
  topics' names) and only the
  current talk. The answer streams back as server-sent events: `talk`,
  then `text` pieces, then `done` or `problem`. A problem is also kept as a
  message, so the talk shows it after a reload.
  The talk's photos go with every question in it, after the text. A
  question asked from a section's own page names it (`about`), and the
  prompt says which section they're looking at: what's kept goes under
  it, and a layout asked from there moves and draws only what's under it.
  A row's own page is the section of its name, and its question names
  the row too (`row`): what fits a field changes the row's cells, and
  what it has parts of goes on its page.
- **A row's page** is made of the same blocks: a block with a `row_id`
  (migration 0017) sits on that collection row's page, not the topic's.
  add_block's `row` (`p3`) puts it there; `after` stays on one page.
  The page Claude reads lists those blocks after the topic's, each
  naming its row (`in_order` in `page.rs`), and the app shows them under
  the row's fields (`onThePage`). Removing a row takes its page's blocks
  (the sync rules' cascade, and `Kept::take_off`), and Undo puts them
  back with their `row_id`.
- **Keeping** (`kept.rs`): Claude files what it learns into the one set
  of blocks (`SET_KINDS`, migration 0015, `proposals/one-set-of-blocks.md`):
  `heading`, `paragraph`, `callout`, `collection` and `picture`. Lists are
  collections (`proposals/facts-as-data.md`, C): a list's rows have only
  a name, a checklist's a check, and the parts of a subject are rows
  with their sentences in a paragraph on the row's page (`row_id`). Claude Code does it with tools while it answers:
  add_block (with a collection's fields), change_block (its title, its
  fields given whole, or a picture its fields allow, only when asked),
  add_items, add_rows (a collection's rows with their cells),
  change_points, tick, move (a block, or points to a block), remove,
  ask_to_check, offer_reminder, change_brief, change_topic, read_topic (another topic's page,
  only to read), propose_sub_topic, move_to_sub_topic, name_talk and fits_elsewhere. Through the API one more request, with structured
  output (`KEEP` in `talks.rs`), reads the same from the exchange. Either
  way `Kept` is a working copy of the page the answer changes, and the
  server writes it with the answer, through the sync engine, so every
  client sees it. What's added, changed, moved and removed is one
  `TopicChange` with a `PageLayout`, so Undo (`layout.rs`) puts the
  blocks and points back and takes off what it started ("Kept a point
  under Gewicht. Changed a point and removed Plants."); only ticks are
  the message's `kept` words. The layout names the collections it
  changed (`collections`), which the answer draws as their pictures, and
  the rows in them it made, changed or moved in (`rows`), which those
  pictures show alone (older answers have none, and show every row).
  Asked to tidy up, a talk moves sentences into a collection and gives
  them a name and cells (the API: a row and the sentence removed), one
  collection an answer; a sentence moved into a collection reads "Made
  a sentence a row in Gewicht."
  An answer starts at most four blocks and adds at most twelve points.
  Claude never picks a colour, a width or a picture: the fields pick a
  collection's picture, and asked for another it picks one they allow.
  - A collection names its `fields` (`FIELD_KINDS`: text, number with its
    unit and goal, day, choice, status with its words in order, check,
    photo, link; eight at most), and its points are its rows: a row's
    text is its name and its `cells` the rest, by field name (a number, a
    yes or no, or words; a day is `YYYY-MM-DD`, or a span or a year as a
    timeline reads it). The fields pick its picture by the first rule
    that fits (`pictureOf` in `collection.ts`, `pictures` in `kept.rs`
    for the page Claude reads): a day with a number a line, when each
    day has one measurement or each name one a day and comes again (a
    line per name, `measures`); one check with at most a day besides a
    checklist (`checkOf`, `check_of`); a status a board, a photo cards, past
    days a timeline, days to come a week, unless every row shares the day
    (`tellsApart`); a number with at most one field more bars, a name and
    two fields or more a table, the rest a list.
    So that explaining draws too, the prompts (`SET_SAYS`, the talk's
    and the keep prompt) ask for two or more events with their years as
    rows of a collection of events, a short name and its year or span,
    and two or more amounts to compare as rows with a number; the
    paragraph keeps what they mean. A row named as a share of the one
    above ("davon …", "of which …", `isPart`) sits indented under it
    and stays out of a total (`sumOf`); the keep prompt names shares so.
    Only fields its rows fill count, but a table shows anyone who can
    fill them every field, one no row fills yet too. View as offers the others its fields
    allow (`picturesFor`). A callout is a warning when a point's `look` is `warn`.
    A block draws one way of its own; only a collection has more, its
    pictures, and only a person picks among them. A paragraph is prose:
    a term with what it means is a collection's row, the term its name
    and the rest a text field, which a list shows under the name and
    cards without its field's name (`BESIDE_MOST`). add_items and
    change_points refuse a paragraph that opens with a term in bold
    (`as_prose` in `kept.rs`). A text cell holds 2000 characters, as a
    point does. Cards and a board
    card with nothing else to say show what a row's page keeps as chips
    (its collections' row names). A timeline's axis has a lane per choice; a
    click on one picks its rows for the list under it, and past ten rows
    the list shows the first eight, with All (`AxisTimeline`). A tally's
    words do the same for the bars, table, cards or list under it (`Told`).
    A topic wears one of the six data hues (`hues.ts`), its sidebar
    group's: its icon, bars, lines, step marks and figures,
    and on the home page its tile (icon, figures, checklist bar) and its
    icon on Today's chips.
    A choice's options take the other five, in order: pills, board
    lanes, the tally, bars and tinted cards (`choiceTint`). Status and
    chrome keep their own colours.
    A topic's tile on the home page shows the first two rows of its first
    figures collection, value over name (`figureField`), or else its count.
  - A checklist's row can ring: offer_reminder with an `item` makes a
    reminder with its `point_id` (migration 0016). Ticking a row writes
    its check cell; a row a reminder rings for ticks the reminder instead (`markDone`): one
    that repeats comes due its rhythm on, and the item stays ticked until
    then, its rhythm beside it. Unticking puts it back.
  - The page Claude reads (`page.rs`) names each block, point and
    reminder (`b2`, `p3`, `r2`, `page::Refs`), a collection with its
    fields and the picture they pick. A point or reminder may `replace`
    some: a point takes the first one's place (unread again) and the rest
    go; a reminder offer puts the ones on the calendar in its `replaces`,
    which the app takes off when it's accepted, and offers not yet taken
    go at once. A question asked from a section's own page keeps what it
    finds under that section.
  - What a topic sticks to is its brief: what it's about and how its
    talks answer (`STICKS_TO_MAX`). A talk changes it only when told how
    to answer from now on, or corrected twice on the same thing, and
    gives it whole (`Kept::change_brief`): the change is kept on the
    message for Undo, as the own topic's are. Its name, group or icon it
    changes only when asked, with change_topic. Both reach its sub-topics
    too, by name (`this_or_sub` in `mcp.rs`), and so do the page tools
    (`PAGE_TOOLS`), each on a copy of the sub-topic's page that the answer
    writes and undoes with its own (`Kept::sub_pages`); other topics it
    only reads, and into its sub-topics it moves.
  - A topic can sit under another, one level deep (`parent_id`, migration
    0019; `may_sit_under` in the sync rules): never under itself, Almanac's
    own topic or a topic that sits under one. Its talks keep to the parent's
    brief too, and the parent's page for Claude names its sub-topics
    (`page.rs`). The sidebar indents them under the parent, the parent's
    page opens with their tiles (`SubTopics`), and About moves a topic
    under another (`TopicParent`). Deleting the parent leaves them, at the
    top. A topic's talk proposes them under its own topic with
    propose_sub_topic, as the own topic proposes a topic: a card each
    (`sub_topics` on the message, migration 0021, started by their place,
    `sub_topic`), with Start, its `parent_id` set. What the parent's page keeps about it goes in
    its `takes` (blocks, and rows with a page of their own) and moves on
    Start (`take` in `topics.rs`): made again in the sub-topic, a row's page
    as its own page, and gone from the parent, so nothing is kept twice. A
    sub-topic that takes something gets no template blocks but You. Later
    the parent's talk moves more there with move_to_sub_topic, the only
    change a talk makes outside its topic: off the parent's page as a
    removal and onto the sub-topic's as what it made (`in_sub_topics` on
    `Kept`, and a `TopicChange` for the sub-topic), so one Undo puts both
    back. Both moves take along what they empty (`left_behind`): a block
    no row stays in, and a choice's lanes no row stays in; in words, only
    those are removed, not what moved.

  The app draws a block as its kind
  (`views.ts`: `viewOf` and `widthOf`), on a grid of twelve columns, at
  the width a person picked with the block's ⋯ (`width`, half or the
  whole), or else its own; a
  collection as the picture its fields pick, which View as changes
  (`view`, `BLOCK_VIEWS`) and only a person sets. View as shows each
  view's icon (`VIEW_ICONS`) and the one showing its name, in the
  selected tint; a pick also writes the width the block is drawn at, so
  it keeps its place, and the picture glides to its new height and fades
  in (`Glide`, none with reduced motion). `you` is what they told
  about themselves, which the topic's About shows and its page doesn't.
- **The workspace's own topic** (`own.rs`) reads every topic's name, group
  and what it sticks to, and has other tools: propose_topic and
  change_topic, not add_items. A proposal's points go in its template's
  blocks by title, and a collection's rows come with their cells. A proposal waits for Start; a change is
  made at once and kept on the message for Undo.
- **Starting a topic** (`topics.rs`): a template (`TEMPLATES` in
  `crates/domain`: `routine`, `collection`, `study`) names the blocks it
  starts with; the blocks are the topic's from then on. The app makes the
  topic and a picked template's blocks through sync (`startTopic` in
  `starting.ts`, which copies `TEMPLATES`), offline too, and keeps it on
  this device until the server has it; then `POST
  /api/w/{id}/topics/{topic}/set_up` sets it up (`followSetUps`), with
  `here` as an ask. `POST /api/w/{id}/topics` still starts one on the
  server, as a proposal does (`from_message`). In the background, Almanac writes the
  brief, with what was written kept word for word at its start
  (`with_theirs`), picks an icon (`ICONS`), a group, the
  template when none was picked, a collection's card title, and the first
  questions (set_up_topic, or structured output). When Claude can't
  answer, the topic keeps its defaults and, its page still empty, starts
  as a collection. What was set before the server had it stays.

Change the prompts in the same commit as what reads their answers. A
prompt says what to do and why, in plain sentences; the schema says the
shape.

## Reminders on the Mac

`reminders.rs` looks every 30 seconds for reminders that are on, notify,
and are due (at their time, or at nine), marks them told for that date,
and sends them on `AppState::due`. The desktop app (`this_mac.rs`) turns
them into notifications, with `code/kit/notify`: the notification plugin's
never show on macOS. A server with no Mac listening marks nothing.

A server's reminders ring from the desktop app's copy (`ringing.tsx`), offline
too. The app keeps every server workspace you're signed in to in sync, not
only the one on screen: a workspace's copy is held (`copies.ts`), one per
workspace, by the page and the ringer. Every 30 seconds the ringer rings
what has come due with the `ring` command. Each Mac keeps in localStorage the
date it rang each reminder for, and the day it began. What was due before
that day stays quiet. The server isn't told, so every Mac you use rings once,
for every reminder in the workspace.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Almanac's rules for the sync engine: who writes what, the
  fields only the server sets, and what a topic or talk takes with it.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts, the
  company's sign-in (`oidc.rs`) and people (`groups.rs`, `company.rs`),
  Claude (`acp.rs` and `mcp.rs` on this Mac, `claude.rs` with a key,
  `keys.rs`), topics, the workspace's own (`own.rs`), talks, what they keep
  (`kept.rs`), the page Claude reads and lays out (`page.rs`,
  `layout.rs`), reminders. Its settings:
  `config.rs`, and `almanac.example.toml`. `serve.rs` runs it on a listener until told to stop, for the server
  binary and the desktop app alike; `local.rs` is the one person of the
  desktop app's own Almanac, who signs in to nothing.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `layout.tsx` is the sidebar,
  `topic.tsx` a topic's page (the one set's blocks and pictures in
  `set.tsx`, the rules for a collection's picture in `collection.ts`), a section's own (`?section=`, what's
  kept under that theme from every block; with `&row=` a collection's
  row, its fields above, which its name opens in every picture, `Opens`) and its About (`?about`), `talk.tsx` the talk beside it,
  `copies.ts` each workspace's local copy, shared by whatever holds it,
  `ringing.tsx` a server's reminders rung on this Mac,
  `asking.tsx` the answers on their way and each topic's open talk (above
  the pages, so they last while you look elsewhere), `calendar.tsx`
  Calendar, `today.tsx` what's due today on the workspace's own page (a ring and chips to tick), `settings.tsx` Settings → Claude and Blocks, every kind drawn
  look-only with the page's own components from `samples.ts`. Keep
  `samples.ts` up to date with the kinds and views. Its workspaces come from
  places (`server.ts`): this Mac, the server it came from, or servers the
  desktop app signed in to.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Almanac on Postgres for
  the workspaces on this Mac, with `code/kit/desktop`, and shows due
  reminders as notifications, a server's too (`ring`).
- `migrations`: SQL schema, applied by the server on start.

## Rules

- sqlx 0.9 refuses SQL built at runtime: write each statement as a `const`.
- Wire field names are snake_case. Times go out as ISO times, never
  formatted: the page shows them in the reader's time zone. Days are
  `YYYY-MM-DD`.
- Change the schema with a new numbered migration; don't edit one that
  has been committed. One that gives a synced model a field ends with
  `UPDATE workspaces SET sync_epoch = gen_random_uuid();`, as Forge's do:
  otherwise clients keep the records they have, without it.
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
