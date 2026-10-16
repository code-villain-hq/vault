# Color

Nightshade.

Ground: night. The studio sits on #0A0B0D.
Brand: cloak. Deep violet. #5A1470.
Accent: poison. Chartreuse. #C9E265.

Grey does most of the work. Cloak is the brand, held to the strength
of a grey: the main action's fill, the selected row. Poison is an
accent, never the main thing.

## Where poison goes

- The focus ring.
- One brand moment per surface: the strike through "Villain" on the
  homepage, the bar on a social card.
- Small live signals: an unread dot, a running agent, a live sync.

Poison never colours links, labels, chips, or body copy. It never
fills a large area, and it never sits on the cloak. Links are ink,
bold, and underlined.

## Night and day

Night is the default and the only ground for studio surfaces: the
site, the blog, social cards. Day is the apps' light mode. Every
surface draws from these names, in both modes.

| Name          | Night     | Day       | Use                                     |
| ------------- | --------- | --------- | --------------------------------------- |
| bg            | `#0A0B0D` | `#F5F5F7` | The frame: sidebar, top bar, page.      |
| panel         | `#12131A` | `#FFFFFF` | Where content sits.                     |
| surface       | `#191A23` | `#F0F0F2` | Fields, notes, raised cards.            |
| hover         | `#1B1C26` | `#EAEAED` | Hover background.                       |
| selected      | `#1A0D24` | `#F3EAF7` | Selected row, current nav item.         |
| line          | `#2A2B36` | `#E3E3E7` | Tables and menus only.                  |
| fg            | `#ECECF1` | `#18181C` | Text and links.                         |
| muted         | `#9B9BA8` | `#55555D` | Secondary text, quiet buttons, chips.   |
| faint         | `#868693` | `#67676E` | Ids, counts, placeholders.              |
| primary       | `#331C40` | `#E8DEEB` | The main action's fill.                 |
| primary-hover | `#3A1A49` | `#DBCBE0` | Its hover.                              |
| on-primary    | `#ECECF1` | `#18181C` | Text on the main action.                |
| accent        | `#C9E265` | `#4C6A00` | Poison: focus, brand moment, live dots. |
| danger        | `#F27A6B` | `#B5382C` | Errors, blocked.                        |
| ok            | `#5CC9A7` | `#1B7357` | Done, passing.                          |
| warn          | `#E9AE45` | `#8A5A00` | Waiting, in review.                     |
| info          | `#7AAEF5` | `#2F62B8` | In progress, neutral notices.           |

By day, bright poison on white is 1.4:1, so the accent darkens to
deep poison. Ok is teal, so it never reads as poison. Day greys carry
no violet; violet means the main action and selection.

Every text colour meets WCAG AA (4.5:1) on panel, in both modes.

## The palette

The names above are built from these.

| Colour       | Value     |
| ------------ | --------- |
| night        | `#0A0B0D` |
| cloak        | `#5A1470` |
| cloak-hover  | `#6B1884` |
| poison       | `#C9E265` |
| poison-deep  | `#4C6A00` |
| wash         | `#1A0D24` |
| ink          | `#ECECF1` |
| muted        | `#9B9BA8` |
| panel        | `#12131A` |
| line         | `#2A2B36` |

The numbers live in [`design/tokens/color.json`](../design/tokens/color.json).
