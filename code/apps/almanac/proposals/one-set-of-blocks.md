# One set of blocks

_Decided October 30, 2026
([`storyboards/one-set-of-blocks.html`](../storyboards/one-set-of-blocks.html)).
It takes the place of steps 3 and 4 of
[`shaping-pages.md`](shaping-pages.md)._

You spend more time tending a Notion page than adding to it, and adding
things the right way is work. Almanac is the other way round: you say
it, Claude files it, and the page draws itself. Every page on every
topic is made of the same few blocks, and a collection's fields pick its
picture, by the same rules everywhere. Nothing is designed for one page.

## The set

**Seven blocks:** Heading, Text, Bullets, Checklist, Toggle, Callout and
Collection. Any block may have a short title.

- A checklist item can repeat and ring: it's a reminder, and Calendar
  shows it with the rest. Ticking it on the page ticks it there.
- A toggle is one line with bullets inside, closed until opened. Whole
  sentences go there, or on a row's page, never on the page's face.

**A collection** is rows with fields, and each row is a page of its own,
made of the same blocks. A field is one of:

| Field | Holds |
|---|---|
| Text | a few words |
| Number | a number, with its unit and a goal when there is one |
| Day | a day, or a span of days or years |
| Choice | one of a few words the collection uses |
| Status | where a row stands, such as Offen, Bestellt, Erledigt |
| Check | yes or no |
| Photo | a photo from a talk |
| Link | a web page |

**Eight pictures.** The first rule that fits draws the collection:

| Its rows have | Picture |
|---|---|
| a day and a number | Line |
| a status | Board |
| a photo | Cards |
| days that are past | Timeline |
| days to come | Week |
| a name and a number | Bars |
| three fields or more | Table |
| anything else | List |

View as offers the other pictures its fields allow, and the choice stays.
One row with a number shows as the number; the line starts at two.

## Who does what

- **You** say things in a talk. You can still tick, edit a cell and drag
  a card, and never have to.
- **Claude** files what you say through tools: add a block, start a
  collection with its fields, add or change a row, add or tick an item,
  move, remove. It never picks a colour, a width or a picture; when you
  ask for another picture, it picks one the fields allow.
- **The app** draws every block and picks every picture. Every change
  shows under the answer, saying where it went, with Undo; a change to a
  collection shows its picture there too.

## In what order

Each step leaves the app working, and is tried before it's committed.

1. **The page draws the set.** Blocks of the seven kinds and collections
   with fields, beside today's kinds, which stay as they are. Tried on
   Acme's pages.
2. **Claude files into the set.** The tools above, and the page Claude
   reads in the new shape. Keeping a point and laying out a page go. New
   blocks are the new kinds; today's stay until step 3.
3. **Today's pages move into the set.** Chips, lists and lines become
   bullets, steps a checklist, sections toggles, cards and tables a
   collection whose rows keep their facts as fields and the rest on their
   page, trends, bars and timelines collections. The week goes; Calendar
   has it. Before and after on a copy of your pages, then the old kinds
   go.
4. **The answer shows what changed:** the picture of the collection it
   changed, and where it went, with Undo.
5. **A row opens as its own page**, beside the topic's.
6. **Tidying up** turns sentences into rows when you ask: "Lisa wiegt
   etwa 3,4 kg" becomes a row in Gewicht. In small steps, each shown.

## Open

- Adding a block yourself, with a "/" menu as in Notion, or only by
  saying it. Until it's settled, only by saying it.
