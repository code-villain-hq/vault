# Almanac

_A Code Villain product. A first version runs as a desktop app: topics,
talks, the kept page, Calendar and Mac reminders._

What you learn in chats is lost in the scroll. When to water a plant, a
hair routine, research for a handover at work, how an industry works:
each was said once, in one of too many chats and tools. Finding it again
means scrolling, or asking a bot that has lost track of a long talk.
Almanac keeps it by topic.

## What's different

- **A topic just works.** You name it and say what it sticks to, like a
  bot that only talks about Karl Marx. It answers from what the model
  knows. There are no sources to collect first, so it's not NotebookLM.
- **Topic first, not chat first.** You open a topic and see what it has
  kept. Chat is for asking and adding; past talks are behind an icon.
  Projects in Claude or ChatGPT give each topic a bot, but open on a chat.

## Run it

```bash
bun install
bun run db             # Postgres in Docker, on 127.0.0.1:5437
bun run dev            # Almanac for Acme Inc: server on :7780, web app on http://localhost:5177
bun run build:desktop  # the desktop app, which runs Almanac on this Mac
```

`bun run dev` runs Almanac for a made-up company, Acme Inc: dev-id
(`code/kit/dev-id`) stands in for its account, so you sign in as anyone in
its `people.toml`. Acme's data has a database of its own, `almanac_acme`.
With an `almanac.toml`, or `ALMANAC_OIDC_ISSUER` in `.env`, it runs with
those settings and the `almanac` database instead.

The packaged desktop app runs Almanac on this Mac, on its own Postgres,
and signs in to nothing. Its data is in
`~/Library/Application Support/eu.codevillain.almanac`.

On this Mac Almanac answers with Claude Code, as you're signed in to it,
through its ACP adapter, as Layer's conversations run. There's no key.
Install the adapter once:

```bash
npm install -g @agentclientprotocol/claude-agent-acp
```

Claude Code gets what the topic sticks to, its page and the talk, and
only Almanac's own tools: not your files, your shell, or your Claude Code
settings. It may search the web, which Settings → Claude turns off.

A company server answers with Claude through the API. Set the key in
Settings → Claude: it stays on the server, in a table that never syncs,
and the page shows only its last four characters. `ANTHROPIC_API_KEY`, or
`[claude] api_key` in the settings file, stands in for it.

## Running it for a company

As every Code Villain app: copy `almanac.example.toml` to `almanac.toml`,
fill in the company, Almanac's address, the database, the company's
account and a SCIM token, and run the image (`Dockerfile`, built from
`code/`). In the account, register Almanac as a confidential client with
the callback `<public_url>/api/auth/oidc/callback`, make the groups
`almanac-users`, `almanac-admins` and `almanac-guests`, and turn on SCIM
with `<public_url>/api/scim/v2` and the settings' token. Guests see no
topics.

## What works

- **The workspace's own topic, Almanac**: every workspace starts with it,
  above Calendar, and opens on it. Tell it what you want to keep track
  of, and it proposes a topic: the name, what it sticks to, the template
  its page starts from, and what it already heard from you. Start it with a click (⌘↩),
  or change it first. Its page is the index of the other topics, by
  group, with what's new. It also changes a topic you name, says what it
  changed, and offers Undo. It can be renamed, not deleted.
- **Starting a topic**: a name, what it sticks to, which may stay short
  or empty, and what it starts as: Routine, Collection, Study, or
  Almanac's pick. Once Claude answers, Almanac writes the rest in the
  background: the brief, what the topic is about and how it answers,
  after what you wrote, which it keeps word for word; the icon, the
  group, the template when you left it to Almanac, and the first
  questions. The page doesn't show the brief, and lists show its first
  sentence.
- **A topic's About**: the name at the top of its page opens it, as does
  About in ⋯. The name, icon and group change where they are, with
  nothing to save. What it sticks to shows whole, and changes in place:
  ⌘↩ or clicking away keeps it, Esc drops it. What it knows about you is
  a line per fact, with the talk it came from, and × takes one out: the
  page doesn't show these. On the page lists the blocks, with ⋯ as on the
  page; a block's name opens the page at it. The workspace's own topic
  has only its name and what it sticks to.
- **A topic is its blocks**, in order, each one of the set: Heading,
  Text, Bullets, Checklist, Toggle, Callout and Collection, with a title
  of the topic's own ("Advice" in Haare, "Terms" in Marx), and You, what
  it knows about you, which About shows. Settings → Blocks shows every
  block and picture with sample points. A template only names the blocks
  a topic starts with: Routine has a Routine checklist, Your shelf and
  Advice; Collection To do and Notes; Study Reading, Terms and What it
  says; each has You. ⋯ beside a block's title moves it up or down,
  renames it or removes it with what it keeps. A block that keeps
  nothing stays off the page, and a topic with nothing kept yet offers
  its first questions.
- **A talk** opens beside the page and streams its answer. Each question
  sends Almanac's fixed prompt, what the topic sticks to, what the page
  keeps and the current talk, nothing older. A question off the topic gets
  a short answer and an offer to ask it in the topic it belongs to, or to
  start one. Earlier talks are behind the talks icon.
- **A topic learns how to answer**: tell a talk how to answer from now
  on ("from now on, in English"), or correct the same thing twice, and
  the topic adds it to its brief. The answer says what it changed, with
  Undo. A talk never changes the topic's name, group or icon.
- **Photos in a talk**: paste, drop or pick up to six with a question, of a
  shampoo, a plant, a page. Claude reads them with that question and every
  later one in the talk; they stay in the talk, not on the page.
- **The page keeps points with their sources**, each in a block of the
  set: text, bullets, a checklist, toggles, a callout, or a collection
  whose rows fill its fields (a row per plant, with its spot and light).
  Claude keeps each point in the block that holds it, and starts a block
  when you ask for one or none fits ("keep what I need to buy"), a
  collection with the fields its rows need. New points carry the unread dot until you leave the topic.
  The page is kept up to date, not added to: a point that changes one it
  has takes that one's place, in the same block. It runs the window's
  width: cards take as many columns as fit, and writing and lists keep a
  length you can read across. The sidebar's
  edge drags wider for long topic names, the talk's edge wider for long
  answers, and this Mac keeps both widths.
- **A collection draws the picture its fields pick**, the same rules on
  every page: a day and a number a line, once each day has one, or each
  name one a day and comes again (a line per shop, named at its end); a
  status a board, a photo cards, days past a timeline and days to come a
  week (a day every row shares picks neither), a number with at most one
  field more bars, three fields or more a table, fewer a list. A toggle opens on its points; a
  checklist's ticked items go last, faint. A colour code shows its
  colour.
- **View as** beside a collection's title, on hover, switches to another
  picture its fields can draw, and the app's own pick among them. The
  choice stays for everyone in the workspace while the fields can draw
  it, and a talk never changes it on its own; picking the app's own
  leaves it to the app again.
- **The page is a grid** of twelve columns: a heading and a callout take
  the whole row, writing half, and a collection what its picture needs.
  A row's blocks grow to fill it, and one that would get too narrow to
  read takes a row of its own, as every block does on a phone. A
  block's ⋯ makes it half the page or the whole, for everyone, until
  it's changed back; a heading stays whole.
- **A section opens on its own page**: the arrow beside a toggle's line
  opens it where the page was, with the talk still beside it: everything
  kept under it from every block, its open questions and the talks it
  came from. Asking from there asks about that section.
- **A row opens as its own page**: a row's name, in any picture, opens
  its fields, a line each, the empty ones too, the blocks on its own
  page, and what's kept under its name. A row's page is made of the same
  blocks as a topic's: a shoe's offers are a collection there, drawn as a
  table, and as a line a shop once a shop has two days. Asking from there
  is about that row: what fits a field fills its cell, and the rest goes
  on its page, in blocks of its own. Removing the row takes its page. A button above its title
  goes back to the topic, as do the breadcrumb and Esc, to where you
  were on the page.
- **Pages from before the set** move into it once, when the server
  starts: sections, questions, shelves and cards become toggles; cards
  with facts, tables, timelines, bars and trends collections; the rest
  bullets or a checklist. The week goes; Calendar has it.
- **A talk changes the page when you ask**: "tidy this up", "put the
  shopping list first", "show my plants as a table". It rewords, moves and
  removes points and blocks, changes a collection's fields, or picks
  another picture its fields allow, at once. It never picks a colour or a
  width.
- **The answer shows what changed**: what it kept, changed, moved or
  removed, and where, with one Undo. A collection it changed shows under
  it as its picture, Lisa's weight as its line, until it's undone: only
  the rows it made or changed, so on one shoe's page it's that shoe.
- **Tidying up turns sentences into rows** when you ask: "Lisa wiegt
  etwa 3,4 kg" becomes Lisa's row in Gewicht, with its source. One
  collection an answer, shown as its picture with Undo ("Made a sentence
  a row in Gewicht."); the answer says what's left, and goes on when you
  say so.
- **⌘K searches what the topics keep**: sections, points, reminders and
  talks, the talks' messages too, with every word you type in any order.
  Up to five a topic show under its name; a point opens its section's
  page, a fact about you the topic's About, and a talk opens beside its
  topic.
- **Almanac asks when it's unsure**: the question waits on the card it's
  about, with Add it and Leave it out. A question it asks in the talk
  waits for your reply there.
- **Reminders**: when a talk settles something that comes round, or you
  tell it a routine you keep, such as your wash days, Almanac offers a
  reminder in the talk. When a wash day moves, the new reminder replaces
  the old one once you say yes. A checklist item can ring: ticking it
  ticks its reminder, and one that repeats shows its rhythm beside it and
  comes back unticked when it's due.
  Calendar shows the workspace's week from every topic and what's still
  to do from before it; what you tick stays on its day, ticked, until you
  untick it. A bell marks the reminders that notify the Mac, and the
  desktop app sends those notifications.

On this Mac a topic searches the web when a question needs it, and links
what it read. A company server's doesn't.

## Decided, not built

- **One set of blocks**, what's left of it: the answer showing the
  picture of the collection it changed, a row opening as its own page,
  and tidying sentences into rows when you ask.
  [`proposals/one-set-of-blocks.md`](proposals/one-set-of-blocks.md) has
  the order of work.
- **Workspaces**, as every Code Villain app has them: Home, and Work for
  what you learn at work. Only Home is made today.
- **Blocks that go quietly**: one that stays empty after a few talks.
- **Adding a block yourself**: today only Claude adds one, when you ask.
- **Dumps:** a shortcut anywhere on the Mac opens one field, with no topic
  to pick. Almanac says where the dump went, with Open and Undo, and
  offers a reminder when there's something to do. When two topics fit, or
  none, it asks right there. Closed without an answer, the dump still
  lands: in the likelier topic, or in the new one it proposed. A dump is
  kept like a talk.

## Storyboards

Open them in a browser. Each shows night and day.

- [`storyboards/starting-a-topic.html`](storyboards/starting-a-topic.html):
  naming a topic, its empty page, a first talk, the topic a month later,
  and the Calendar.
- [`storyboards/opening-a-topic.html`](storyboards/opening-a-topic.html):
  Pflanzen, Haare and Manage up as they open.
- [`storyboards/the-page.html`](storyboards/the-page.html): Haare's page
  three ways, and the routine page taken from two of them. Night only.
- [`storyboards/dumping.html`](storyboards/dumping.html): a dump, where it
  went, what it asks when it's unsure, and the dump later in its topic.
- [`storyboards/blocks.html`](storyboards/blocks.html): topics made of
  blocks: Haare from Routine, Pflanzen from Collection, Marx from Study,
  starting from a template, and a new topic's empty blocks. Night only.
- [`storyboards/the-workspaces-own-topic.html`](storyboards/the-workspaces-own-topic.html):
  a new workspace on its own topic, the topic it proposes, its page as the
  index, and Settings → Claude with Claude Code.
- [`storyboards/how-a-topic-answers.html`](storyboards/how-a-topic-answers.html):
  a topic's brief, how Almanac writes it, and how it learns from its talks.
- [`storyboards/a-section-opens.html`](storyboards/a-section-opens.html):
  Geschichte's Intifada section on its own page, and a Timeline block.
- [`storyboards/a-better-layout.html`](storyboards/a-better-layout.html):
  asking a talk to lay the page out anew, what moved and its Undo, and a
  timeline made from what's kept.
- [`storyboards/a-timeline-per-section.html`](storyboards/a-timeline-per-section.html):
  Geschichte's timeline as a line per section, Intifada's own, and asking
  from Intifada.
- [`storyboards/the-page-fills-the-window.html`](storyboards/the-page-fills-the-window.html):
  Preise finden's cards across the window, Geschichte's lines at their
  length, and the sidebar dragged wider.
- [`storyboards/a-topics-about.html`](storyboards/a-topics-about.html):
  Haar-Routine without You on its page, its About, and the button back
  from a section.
- [`storyboards/the-page-at-a-glance.html`](storyboards/the-page-at-a-glance.html):
  Geschichte as tiles, Pflanzen's card faces, Katzen's one cat as a
  profile, and Karriere's bars, questions and ticked list.
- [`storyboards/charts-tables-and-a-grid.html`](storyboards/charts-tables-and-a-grid.html):
  Preise finden as a table, Lisa's weight as a trend, where Karriere's
  ads are as parts, and the page as a grid of twelve columns.
- [`storyboards/claude-writes-code-draws.html`](storyboards/claude-writes-code-draws.html):
  Karriere's job ads and Code Villain's Marke und Launch as they are,
  then what Claude writes and what the app draws from it, and who
  decides what. Night only.
- [`storyboards/one-set-of-blocks.html`](storyboards/one-set-of-blocks.html):
  the seven blocks and eight pictures, Katzen, Wohnung reparieren and
  Geschichte made from them, and a talk filing Lisa's weight.

## Open

- Whether a dump picks its own workspace. Today it goes to the one the
  panel shows, the one you used last. Picking by itself could put a
  personal note where colleagues see it.
- Whether Calendar also shows Work, or only the workspace you're in.
- Whether a company server's topics search the web too.
- Other agents than Claude Code over ACP, as Layer has them.
