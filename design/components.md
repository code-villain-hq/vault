# Components

The few controls every Code Villain surface shares. They follow [`principles.md`](principles.md), take their colours from [`brand/color.md`](../brand/color.md), and their numbers from [`tokens/`](tokens). `@villain/ui` will implement them.

## Shape

| Radius | Value | On |
| --- | --- | --- |
| `radius-sm` | 4px | Key hints, small badges. |
| `radius-md` | 6px | Buttons, fields, rows, nav items, notes. |
| `radius-lg` | 10px | Panels, the content area, dialogs. |
| `radius-full` | round | Status pills, chips. |

No borders lift a surface. Elevation is a step in tone.

## Button

In app interfaces: 28px tall, the `control` type role (13px, 600),
6px corners, no border.

- **Primary**: the one action the screen's state needs. Fill `primary`, text `on-primary`; hover `primary-hover`. One per screen.
- **Secondary**: every other action. No fill, text `muted`; on hover, fill `hover` and text `fg`.
- **Focus**: a 2px `accent` ring, 2px out in app interfaces.
- **Disabled**: 45% opacity.

### Studio call to action

On the site, a main call to action uses the studio `label` role
(15px, 700, leading 1.2, tracking 0.01em), with `0.7rem 1.15rem`
padding instead of a fixed height. It keeps the same primary colours,
6px corners, and no border. Focus is a 2px `accent` ring, 3px out.
The homepage’s “Read the log” link is this variant. Site navigation
also uses `label`; it follows the layout’s navigation styling.

## Field

A tone step with no border, in the `ui` type role: fill `field`, text `fg`, placeholder `faint`. Hover fills `field-hover`. In the frame (top bar, sidebar), the fill is `panel`. Inside a dialog, use `dialog-field` and `dialog-field-hover` instead: a field must remain distinct from its host in both modes. Focus draws one ring on the edge: border `accent` plus a 3px glow of `accent` at 22%.

## Dialog

Fill `dialog`, `radius-lg`, with a dimmed backdrop. No elevation border or shadow. By night the dialog sits above `surface` and its fields sit below it in tone; by day it is white with grey fields. These roles are explicit so a field never accidentally takes its dialog's fill. Inline title and prose editors can remain transparent; they belong to the document rather than a boxed form.

## Link

Ink, bold, underlined. The underline is `fg` at 45% and firms to `fg` on hover. Links are never coloured.

## Status pill

Round, in the `control` role at 700. Composite the status colour (`ok`, `warn`, `danger`, `info`, or a user-chosen hue) at `status-tint` (13%) over the actual surface. Keep this fill translucent; do not bake it against one background.

Text and the leading dot use `status-<name>-ink`; `status-neutral-ink` covers quiet states, whose tint source is `muted`. The ink is checked at 4.5:1 or better against the composite on frame, panel, surface, hover, selection, dialog, and field tones in both modes. Do not use the adjusted ink as the tint source: that would change the fill's hue and strength. Hover and open states keep the same tint and ink.

For user-chosen colours, keep the chosen hue as the tint source and adjust its ink toward black by day or white by night until it meets the same contrast requirement on all supported hosts. This changes presentation, never the stored colour.

## Chip

Round. Text `muted` in the `control` role, on a translucent `fg` tint at `chip-tint` (7%). A label is detail, not a signal.

Tints are recipes, not precomputed solid colours. Chips and status pills blend with their host; the base surface provides the ground. When a tint carries text, check the resulting composite in each supported context and adjust the ink rather than removing the tint.

## Row

No dividers. 6px corners inside a list inset from the panel edge. Hover fills `hover`; selection fills `selected`.

The active option in a menu or command list (the one the arrow keys move) uses a translucent `fg` tint at `highlight-tint` (8%), like a chip. A solid `hover` can match its host: in night, `dialog` and `hover` are the same tone. The tint stands apart on every host, and `fg`, `muted` and `faint` are checked on its composites.

## Icon

Interface icons are line icons from one set per toolkit: Lucide on the web, SF Symbols in AppKit. A curated picker set may come from Tabler's outline icons, like Forge's service icons. Don't mix sets on one screen, and don't draw one-off icons when the set has one.

- **Size**: 15px beside text in rows, buttons, and nav, and inside the 28px icon button; 12px inside chips and pills. In AppKit, a symbol takes the point size of the text beside it.
- **Stroke**: 1.75 on the web. SF Symbols use the weight of the text beside them.
- **Colour**: `muted` by default, `fg` on hover and when current, `faint` for decoration. A status icon wears its `status-<name>-ink`. Never poison or cloak: poison is the strike and live dots, cloak is focus.
- **Name**: an icon with no visible label carries an accessible name.

Product marks and the menu bar glyph are brand, not interface icons. They follow [`brand/mark/`](../brand/mark/README.md).

## Live dot

A small round `signal` dot for something happening now: unread, a running agent, a live sync. It wears the brand colour: `signal` is poison, moved only as far as the 3:1 non-text floor, so by day it stays nearer chartreuse than text would.
