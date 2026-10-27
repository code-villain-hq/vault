# Almanac

*A Code Villain product. Idea stage: the problem and two storyboards are
agreed; nothing is built.*

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

## Decided

- **Workspaces**, as every Code Villain app has them: Home, and Work for
  what you learn at work.
- **A topic keeps things itself**, and asks only when it's unsure. A
  question waits where it's about, not in a chat.
- **Reminders and a calendar** are a must. Almanac decides when to offer a
  reminder. Reminders only need to reach the Mac.
- **A topic opens on what it's for:** Pflanzen on its week and a card per
  plant, Haare on today's routine as numbered steps, Manage up on the
  handover as a document that grows. New parts carry the unread dot.
- **Dumps:** a shortcut anywhere on the Mac opens one field, with no topic
  to pick. Almanac says where the dump went, with Open and Undo, and
  offers a reminder when there's something to do. When two topics fit, or
  none, it asks right there. Closed without an answer, the dump still
  lands: in the likelier topic, or in the new one it proposed. A dump is
  kept like a talk.

## Storyboards

Open them in a browser. Each shows night and day.

- [`storyboards/opening-a-topic.html`](storyboards/opening-a-topic.html):
  Pflanzen, Haare and Manage up as they open.
- [`storyboards/dumping.html`](storyboards/dumping.html): a dump, where it
  went, what it asks when it's unsure, and the dump later in its topic.

## Proposed, not agreed

- A two-week trial before building: a notes folder per topic and an agent
  that keeps them.

## Open

- Whether a dump picks its own workspace. Today it goes to the one the
  panel shows, the one you used last. Picking by itself could put a
  personal note where colleagues see it.
- Which model answers, and whether a topic searches the web.
