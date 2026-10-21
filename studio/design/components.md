# Components

The few controls every Code Villain surface shares. They follow [`principles.md`](principles.md), take their colours from [`studio/brand/color.md`](../brand/color.md), and their numbers from [`tokens/`](tokens). [`components.css`](components.css) implements them once for the web, as `ui-*` classes that Forge, Layer, Glyph, Raven and the site copy and use; Caster draws the same recipes in AppKit. [`lint/`](lint/README.md) keeps every web surface on them.

## Shape

| Radius | Value | On |
| --- | --- | --- |
| `radius-sm` | 4px | Key hints, small badges. |
| `radius-md` | 6px | Buttons, fields, rows, nav items, notes. |
| `radius-lg` | 10px | Panels, the content area, dialogs. |
| `radius-full` | round | Status pills, chips. |

No borders lift a surface. Elevation is a step in tone.

## Button, link, tab or chip

Each kind of control has its own look, so a glance tells them apart:

- A **button** changes something here. It is a block: filled, 6px corners.
- A **link** goes somewhere: another view, another app, a web page. It is underlined.
- A **tab** switches what one place shows. It is text, with a line under the current one.
- A **segmented control** picks one value of a few. It is a track, with the chosen option filled.
- A **chip** or **status pill** says something. It is round. It is clickable only where it holds what it says: a record's property under its title opens that property's picker, and a pill that names a problem (a conflict count) opens its fix.

Opening a ticket, a pull request or a folder in another app is a link, not a button. The one exception: when the screen's one action happens elsewhere (merge on GitHub), it stays the primary button, with ↗. Muted text with nothing under it and no fill is never clickable, except an icon button.

## Button

In app interfaces: 28px tall, the `control` type role (13px, 600),
6px corners, no border.

- **Primary**: the one action the screen's state needs. Fill `primary`, text `on-primary`; hover `primary-hover`. One per screen.
- **Secondary**: every other action. Fill `fg` at `button-tint` (10%), text `fg`; hover `fg` at `button-hover-tint` (16%). The fill is translucent, like a chip's, so it reads on every host.
- **Danger**: a destructive secondary action. The secondary fill, text `status-danger-ink`; never a red block.
- **Icon button**: a 28px square with no fill and a `muted` icon; on hover, fill `hover` and icon `fg`. The icon marks it as a control, so it stays quiet. It carries an accessible name.
- **Focus**: a 2px `accent` ring, 2px out in app interfaces.
- **Disabled**: 45% opacity.

### Studio call to action

On the site, a main call to action uses the studio `label` role
(17px, 700, leading 1.2, tracking 0.01em), with `0.7rem 1.15rem`
padding instead of a fixed height. It keeps the same primary colours,
6px corners, and no border. Focus is a 2px `accent` ring, 3px out.
No page uses it yet. Inner pages' navigation also uses `label`; the
homepage's, beside the splash, takes `title`.

## Field

A tone step with no border, in the `ui` type role: fill `field`, text `fg`, placeholder `faint`. Hover fills `field-hover`. In the frame (top bar, sidebar), the fill is `panel`. Inside a dialog, use `dialog-field` and `dialog-field-hover` instead: a field must remain distinct from its host in both modes. Focus draws one ring on the edge: border `accent` plus a 3px glow of `accent` at 22%.

## Dialog

Fill `dialog`, `radius-lg`, over a backdrop of black at `backdrop-tint` (40%). No elevation border or shadow. By night the dialog sits above `surface` and its fields sit below it in tone; by day it is white with grey fields. These roles are explicit so a field never accidentally takes its dialog's fill. Inline title and prose editors can remain transparent; they belong to the document rather than a boxed form.

## Link

Ink, bold, underlined. The underline is `fg` at 45% and firms to `fg` on hover. Links are never coloured. A link that leaves the app, to another app or a web page, ends in ↗. A link that stands among controls, not in prose, takes the `control` size.

## Status pill

Round, in the `control` role at 700. Composite the status colour (`ok`, `warn`, `danger`, `info`, or a user-chosen hue) at `status-tint` (13%) over the actual surface. Keep this fill translucent; do not bake it against one background.

Text and the leading dot use `status-<name>-ink`; `status-neutral-ink` covers quiet states, whose tint source is `muted`. The ink is checked at 4.5:1 or better against the composite on frame, panel, surface, hover, selection, dialog, and field tones in both modes. Do not use the adjusted ink as the tint source: that would change the fill's hue and strength. Hover and open states keep the same tint and ink.

For user-chosen colours, keep the chosen hue as the tint source and adjust its ink toward black by day or white by night until it meets the same contrast requirement on all supported hosts. This changes presentation, never the stored colour.

## Chip

Round. Text `muted` in the `control` role, on a translucent `fg` tint at `chip-tint` (7%). A label is detail, not a signal.

Tints are recipes, not precomputed solid colours. Chips and status pills blend with their host; the base surface provides the ground. When a tint carries text, check the resulting composite in each supported context and adjust the ink rather than removing the tint.

## Tabs

Switch what one place shows: a task's Spec, Agents, Diff and Pull requests, or an app's views across its top bar. Tabs sit 20px apart. Each is text in the `control` role, `muted`, `fg` on hover. The current tab is `fg` at 700 with a 2px `fg` line under it, and no fill, so a tab never looks like a button or a selected row. A count beside a label is `faint` in `ui-small` at 400. Mark the strip `role="tablist"` and the current tab `aria-selected="true"`; views in a top bar are navigation, so there the current one carries `aria-current="page"`. Top-bar views take the `ui` role, like nav items, and their line sits on the bar's bottom edge.

## Segmented control

Pick one value of a few: merge or rebase, an interface scale. One track of `fg` at `chip-tint`, `radius-md`, 2px inside. Each option is text in the `control` role, `muted`, 24px tall; the chosen one is filled `fg` at `button-hover-tint` with `fg` text at 700 and `radius-sm`. Not a row of buttons with one in `selected`: beside filled buttons, the chosen one would look the weakest. Mark the chosen option `aria-pressed="true"`.

## Row

No dividers. 6px corners inside a list inset from the panel edge. Hover fills `hover`; selection fills `selected`.

The active option in a menu or command list (the one the arrow keys move) uses a translucent `fg` tint at `highlight-tint` (8%), like a chip. A solid `hover` can match its host: in night, `dialog` and `hover` are the same tone. The tint stands apart on every host, and `fg`, `muted` and `faint` are checked on its composites.

## Icon

Interface icons are line icons from one set per toolkit: Lucide on the web, SF Symbols in AppKit. A curated picker set may come from Tabler's outline icons, like Forge's service icons. Don't mix sets on one screen, and don't draw one-off icons when the set has one.

- **Size**: 15px beside text in rows, buttons, and nav, and inside the 28px icon button; 12px inside chips and pills. In AppKit, a symbol takes the point size of the text beside it.
- **Stroke**: 1.75 on the web. SF Symbols use the weight of the text beside them.
- **Colour**: `muted` by default, `fg` on hover and when current, `faint` for decoration. A status icon wears its `status-<name>-ink`. Never poison or cloak: poison is the strike and live dots, cloak is focus.
- **Name**: an icon with no visible label carries an accessible name.

Product marks and the menu bar glyph are brand, not interface icons. They follow [`studio/brand/mark/`](../brand/mark/README.md).

## Live dot

A small round `signal` dot for something happening now: unread, a running agent, a live sync. It wears the brand colour: `signal` is poison, moved only as far as the 3:1 non-text floor, so by day it stays nearer chartreuse than text would.
