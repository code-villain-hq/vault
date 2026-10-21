# Color

Nightshade.

Ground: night. The studio sits on the apps' `bg`, #0B0B0B.
Brand: poison. Chartreuse. #C9E265.
Supporting: cloak. Deep violet. #5A1470.

Grey does most of the work. Poison is the brand: the colour people
remember, carried by the strike in every mark and every live signal.
Cloak supports it with depth and mood, and a lifted violet marks focus.

Until October 2026 the roles were the other way round. Poison was
already what people saw in every mark, so the brand now says so.

## Where poison goes

- One brand moment per surface: the strike through "Code Villain" on the
  homepage, the bar on a social card.
- The strike inside a product mark, wherever that product is shown.
- The main action's fill, once per screen, with dark ink.
- A faint tint for the selected row and the current nav item.
- Small live signals: an unread dot, a running agent, a live sync.
- In code, the search match: a faint tint on every match, the fill on
  the current one. It never colours a token
  ([`studio/design/tokens/theme.md`](../design/tokens/theme.md#code)).
- In a terminal, the green: success, additions, executables. A shell
  speaks in ANSI colours, and its green is the brand's. The other
  terminal colours sit calmer, so poison is the one that stands out.

Poison never colours links, labels, chips, or body copy, and it never
fills a large area beyond that one action. Links are ink, bold, and
underlined.

## Where cloak goes

- The focus ring, lifted to a readable violet on night.
- Depth on studio surfaces, such as the cube wallpaper.

Cloak never sits under poison text, and neither colour tints the
large neutral surfaces of an app.

## Marks and icons

The marks and app icons use the apps' black and white: night `bg`
`#0B0B0B` for the tile, `fg` `#EBEBEB` for the letters, and day `fg`
`#181818` on day grounds. They do not use the tinted `night` and `ink`
from the palette below. Poison is the strike and nothing else. Rules
for the mark live in [`mark/`](mark/README.md).

Interface icons are not marks. They take the text colours, never
poison or cloak; see [`studio/design/components.md`](../design/components.md#icon).

## Night and day

Night is the default and the only ground for studio surfaces: the
site, the blog, social cards. Day is the apps' light mode. Every
surface draws from these names, in both modes. The values below are the
current generated Nightshade output. Theme inputs and relationships live
in [`studio/design/tokens/`](../design/tokens/theme.md), rather than independent
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
| dialog-field-hover | `#1B1B1B` | `#EBEBEB` | Their hover.                            |
| hover         | `#1D1D1D` | `#EBEBEB` | Hover background.                       |
| selected      | `#1C1E13` | `#F6FAE6` | Selected row, current nav item: a poison tint. |
| line          | `#2B2B2B` | `#E1E1E1` | Tables and menus only.                  |
| fg            | `#EBEBEB` | `#181818` | Text and links.                         |
| muted         | `#9B9B9B` | `#555555` | Secondary text, chips, idle tabs.       |
| faint         | `#969696` | `#5E5E5E` | Ids, counts, placeholders.              |
| primary       | `#C9E265` | `#C9E265` | The main action's fill: poison.         |
| primary-hover | `#B6CE50` | `#B6CE50` | Its hover.                              |
| on-primary    | `#0B0B0B` | `#181818` | Dark text on the main action.           |
| accent        | `#AF6BC8` | `#5A1470` | Cloak, lifted: focus.                   |
| signal        | `#C9E265` | `#7C9000` | Poison for live dots, at 3:1.           |
| danger        | `#F27A6B` | `#B54338` | Errors, blocked.                        |
| ok            | `#5CC9A7` | `#00775D` | Done, passing.                          |
| warn          | `#E9AE45` | `#8C6100` | Waiting, in review.                     |
| info          | `#7AAEF5` | `#396AAB` | In progress, neutral notices.           |

Poison is light, so it is a fill in its own right: the main action
wears it as is in both modes, with dark ink at better than 12:1. By
day its edge against white is only 1.4:1, so the button reads by
colour; it is the one fill of its kind on the screen. Cloak is dark, so
at night the accent lifts it to a readable violet; by day cloak itself
is readable. Live dots carry no text, so `signal` stops at WCAG's 3:1
non-text floor: bright poison at night, `#7C9000` by day. Ok is teal, so it never reads as poison. Greys stay
achromatic in both modes.

Every text colour meets WCAG AA (4.5:1) on panel, in both modes.

Tinted elements blend with the surface beneath them. Status pills use the
semantic hue at 13% opacity; quiet chips use `fg` at 7%; the active option in a menu uses `fg` at 8%. Preserve that
compositing rather than substituting a solid background. Status text and
dots use separate `status-<name>-ink` tokens so the fill keeps its hue while
the ink stays readable on the resulting tint. The recipes and supported
hosts are defined in [`studio/design/components.md`](../design/components.md).

## The palette

Poison and cloak seed the names above: poison the main action and live
signals, cloak the focus colour. The greys are pure greys, worked out
rather than picked. Poison-deep and wash are brand hues for the marks
and images.

| Colour       | Value     |
| ------------ | --------- |
| cloak        | `#5A1470` |
| poison       | `#C9E265` |
| poison-deep  | `#4C6A00` |
| wash         | `#1A0D24` |

The numbers live in [`studio/design/tokens/color.json`](../design/tokens/color.json).
