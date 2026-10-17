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

A tone step with no border, in the `ui` type role: fill `surface`, text `fg`, placeholder `faint`. In the frame (top bar, sidebar), the fill is `panel`. Hover fills `hover`. Focus draws one ring on the edge: border `accent` plus a 3px glow of `accent` at 22%.

## Link

Ink, bold, underlined. The underline is `fg` at 45% and firms to `fg` on hover. Links are never coloured.

## Status pill

Round. The status colour (`ok`, `warn`, `danger`, `info`) as text and a leading dot, on a 13% tint of itself. The `control` role at 700.

## Chip

Round. Text `muted` in the `control` role, on `fg` at 7%. A label is detail, not a signal.

## Row

No dividers. 6px corners inside a list inset from the panel edge. Hover fills `hover`; selection fills `selected`.

## Live dot

A small round `accent` dot for something happening now: unread, a running agent, a live sync. One of the three places poison goes.
