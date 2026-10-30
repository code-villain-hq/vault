# Shaping a topic's page

_Proposed October 29, 2026. Decided October 30: Claude writes, code
draws ([`storyboards/claude-writes-code-draws.html`](../storyboards/claude-writes-code-draws.html))._

A topic's page should read at a glance: a table where things are
compared, a line where something is measured, chips for short facts.
Ours mostly don't. This says why, and what changes.

## What goes wrong

**Laying out.** "Lay this page out anew" has Claude rewrite the whole
page in one go: every block's kind, order and width, and every point
it moves or draws.

- It's slow. Preise finden took 3½ minutes, and a refusal for one
  character too many meant writing it all again.
- Claude can't see the page. So each problem the page showed became a
  rule for Claude: chips hold 48 characters, a table with three
  columns takes the whole width, the week too. Every try found another.
- Claude picks what the app should decide. It chooses widths, but how a
  page looks comes from how the app draws it. A table hid two of its
  columns even at full width. Cards sat three to a row, so a fourth sat
  alone, and every card was as tall as the tallest.

**Keeping.** Every point goes into a block Claude picked the look of.
When the point doesn't fit the look, it goes there anyway: Code
Villain's chips hold 23 sentences, and another topic's cards are
categories, not the things it compares, so one thing's facts sit in
different cards.
Claude reads about 1,260 words on how blocks look with every question.

**The import.** I brought the twelve Grok chats in on October 28. An
agent read each chat and wrote what was worth keeping into the blocks
the pages had then: text and cards, two to four sentences a point.
Tables, numbers and days came later. So the 541 points it brought are
sentences. None came with a number, and Katzen and most other topics
have no days. Every shape has to be dug out of them afterwards.

## What changes

**1. Claude writes what's true and where it goes.** Sections; the
things in them, each with its fields (a plant: Licht, Wasser, Platz);
steps in order; a day, a number with its unit; done; needs a
look. Never a block's kind, width or icon. What Claude is told about
how blocks look, widths and the fit rules goes.

**2. The app draws the page, the same way every time.**

- The view comes from the shape of what a block keeps: things with
  the same fields are cards, or a table past eight; steps in order a
  checklist; short facts chips; sentences a list; paragraphs section
  tiles; days a timeline; days with numbers a trend. A colour code
  gets its swatch.
- A block is as wide as its content needs, and the app fills the
  rows. A table shows every column; a cell that doesn't fit wraps.
  Cards fill their row and are as tall as what they hold.
- What's due and what needs a look come first. A block with nothing in
  it doesn't show.
- Today, the week and You stay as they are: reminders and what a topic
  knows about you draw them.

**3. You switch a view.** View as on a block's title: table, cards,
list, timeline. The choice stays, and Claude never changes it.

**4. Tidy up reorganizes, never redraws.** Asking a talk to tidy up
merges, splits, renames and moves, and takes a number or a day out of a
sentence. It works in small steps, each checked and shown at once, so a
refusal costs one step, not the page. One Undo under the answer takes
back all of them.

**5. The Grok chats come in again, shaped.** The same agent reads each
chat, now keeping each thing the way a talk would. Try it on one topic
first, Katzen or Preise finden, and compare the two pages. If the new page is
better, the rest follow.

## In what order

1. The app picks each block's view and width from what it keeps today.
   A block's kind becomes the view you pinned, empty for the app's
   pick. Before and after with real pages.
2. View as on a block's title.
3. What Claude is told: what each point is, not how it looks. Laying
   out anew becomes tidying up, in small steps.
4. One topic imported again, beside the old page. Then the others.

## What stays from what's built

- Tables, trends, bars, rings, the timeline, cards and profiles: the
  app draws with them.
- The refusals that keep a page true: no made-up number or day, and a
  trend needs a weighing with its day.

## Open

- Today's blocks: do they start on the app's pick, or keep the kind
  Claude gave them as if you had pinned it? Proposed: the app's pick.
- A field's icon (Licht, Wasser): from a fixed list by its name, or
  none?
- A topic imported again: does it replace the imported points, or sit
  beside them until you choose? What you've said in talks since stays
  either way.
