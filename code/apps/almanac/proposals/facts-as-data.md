# Facts as data

_Proposed October 31, 2026. Follows [`one-set-of-blocks.md`](one-set-of-blocks.md)._

The pages hold their facts as words. Of the real pages' collections, 12
fields are text and 2 days; beside them are 28 toggles and 14 bullets.
"Vegan: Nein, Nubuk und Wildleder" can't be ticked or counted, and a
shoe's prices are sentences under it, so nothing draws them. Every fix
below is a rule for every page, never one for shoes.

## A. Each fact in the narrowest kind

- A field takes the narrowest kind that holds its fact: yes or no a
  check, an amount or a price a number with its unit, a date a day, a
  page a link, a few repeating words a choice. Text only for words.
- A cell holds the fact alone. A reason or a note gets a text field of
  its own: Vegan ✓/–, Material "Nubuk und Wildleder".
- A field changed to another kind reads its cells anew ("1 790" → 1790,
  "Ja" → ✓). Those it can't read go empty, and Claude is told which, to
  give them their cell and the rest of the words their own field.
- Tidying up does this too, one collection an answer, with Undo.

_Built:_ the rules in the tools and both prompts, and `change_block`
reading cells anew (`a_field_of_another_kind_reads_its_cells_anew`).
Not yet tried against Claude.

## B. A row's page is made of the same blocks

The set promised it; today a row's page holds only sentences. A block
can sit on a row's page as on a topic's: a collection there too. A
shoe's offers become a collection on its page (Laden a link, Preis a
number in kr, Tag a day), drawn as a table, and as a line once a shop
has two days. Nothing names shoes: any row can hold any block, and the
pictures pick themselves by the same rules.

## C. Only data, where words aren't the point

Today's blocks are already the seven of the set. Of those, three hold
words: Heading, Text, Callout. The other three could be collections:

| Today     | As a collection                            |
| --------- | ------------------------------------------ |
| Bullets   | rows with only a name (List)               |
| Checklist | rows with a check, and a day when it rings |
| Toggle    | a row, with its sentences on its page (B)  |

That leaves four blocks: Heading, Text, Callout, Collection. It needs B
first, since a toggle's sentences move onto a row's page. Decided after
B is tried.
