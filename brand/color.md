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
- One brand moment per surface: the strike through "Code Villain" on the
  homepage, the bar on a social card.
- Small live signals: an unread dot, a running agent, a live sync.
- The strike inside a product mark, wherever that product is shown.

Poison never colours links, labels, chips, or body copy. It never
fills a large area, and it never sits on the cloak. Links are ink,
bold, and underlined.

## Night and day

Night is the default and the only ground for studio surfaces: the
site, the blog, social cards. Day is the apps' light mode. Every
surface draws from these names, in both modes. The values below are the
current generated Nightshade output. Theme inputs and relationships live
in [`design/tokens/`](../design/tokens/theme.md), rather than independent
hand-tuned night/day colour lists.

| Name          | Night     | Day       | Use                                     |
| ------------- | --------- | --------- | --------------------------------------- |
| bg            | `#0B0B0B` | `#F5F5F5` | The frame: sidebar, top bar, page.      |
| panel         | `#141414` | `#FFFFFF` | Where content sits.                     |
| surface       | `#1B1B1B` | `#F0F0F0` | Notes, raised cards.                    |
| dialog        | `#1D1D1D` | `#FFFFFF` | A dialog above the content.             |
| field         | `#1B1B1B` | `#F0F0F0` | Form controls in content.               |
| field-hover   | `#1D1D1D` | `#EBEBEB` | Their hover.                            |
| dialog-field  | `#141414` | `#F0F0F0` | Form controls inside a dialog.          |
| dialog-field-hover | `#1B1B1B` | `#EBEBEB` | Their hover.                       |
| hover         | `#1D1D1D` | `#EBEBEB` | Hover background.                       |
| selected      | `#1B0D1F` | `#F3EFF5` | Selected row, current nav item.         |
| line          | `#2B2B2B` | `#E1E1E1` | Tables and menus only.                  |
| fg            | `#EBEBEB` | `#181818` | Text and links.                         |
| muted         | `#9B9B9B` | `#555555` | Secondary text, quiet buttons, chips.   |
| faint         | `#969696` | `#5E5E5E` | Ids, counts, placeholders.              |
| primary       | `#34183D` | `#E8DEEB` | The main action's fill.                 |
| primary-hover | `#3B1846` | `#DBCBE0` | Its hover.                              |
| on-primary    | `#EBEBEB` | `#181818` | Text on the main action.                |
| accent        | `#C9E265` | `#607000` | Poison: focus and the brand moment.     |
| signal        | `#C9E265` | `#7C9000` | Poison for live dots, at 3:1.           |
| danger        | `#F27A6B` | `#B54338` | Errors, blocked.                        |
| ok            | `#5CC9A7` | `#00775D` | Done, passing.                          |
| warn          | `#E9AE45` | `#8C6100` | Waiting, in review.                     |
| info          | `#7AAEF5` | `#396AAB` | In progress, neutral notices.           |

By day, bright poison on white is 1.4:1, so the accent darkens to
a readable shade of poison. Live dots carry no text, so `signal` stops
at WCAG's 3:1 non-text floor and stays brighter. Ok is teal, so it never reads as poison. Greys stay
achromatic in both modes; violet means the main action and selection.

Every text colour meets WCAG AA (4.5:1) on panel, in both modes.

Tinted elements blend with the surface beneath them. Status pills use the
semantic hue at 13% opacity; quiet chips use `fg` at 7%; the active option in a menu uses `fg` at 8%. Preserve that
compositing rather than substituting a solid background. Status text and
dots use separate `status-<name>-ink` tokens so the fill keeps its hue while
the ink stays readable on the resulting tint. The recipes and supported
hosts are defined in [`design/components.md`](../design/components.md).

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
