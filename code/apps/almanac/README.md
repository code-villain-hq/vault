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

Almanac answers with Claude through the API. Set the key in
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

- **Starting a topic**: a name, and what it sticks to, which may stay
  empty. With a key set, Almanac writes the rest in the background: what
  it sticks to when that was empty, the icon, the group, the page's shape
  (cards or a document) and the first questions. Edit topic changes any
  of it.
- **An empty topic** says what will be there and offers its first
  questions.
- **A talk** opens beside the page and streams its answer. Each question
  sends Almanac's fixed prompt, what the topic sticks to, what the page
  keeps and the current talk, nothing older. A question off the topic gets
  a short answer and an offer to ask it in the topic it belongs to, or to
  start one. Earlier talks are behind the talks icon.
- **The page keeps short points with their sources**, grouped by theme:
  a card per plant on a cards page, sections of a document on a document
  page. New points carry the unread dot until you leave the topic.
- **Almanac asks when it's unsure**: the question waits on the card it's
  about, with Add it and Leave it out.
- **Reminders**: when a talk settles something that comes round, Almanac
  offers a reminder in the talk. A topic with reminders shows its week.
  Calendar shows the workspace's week from every topic, what's still to
  do and what comes later; a bell marks the reminders that notify the
  Mac, and the desktop app sends those notifications.

Web search is off.

## Decided, not built

- **Workspaces**, as every Code Villain app has them: Home, and Work for
  what you learn at work. Only Home is made today.
- **More shapes**: Haare on today's routine as numbered steps, Karl Marx
  on what you're reading.
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
- [`storyboards/dumping.html`](storyboards/dumping.html): a dump, where it
  went, what it asks when it's unsure, and the dump later in its topic.

## Open

- Whether a dump picks its own workspace. Today it goes to the one the
  panel shows, the one you used last. Picking by itself could put a
  personal note where colleagues see it.
- Whether Calendar also shows Work, or only the workspace you're in.
- Whether a topic searches the web.
