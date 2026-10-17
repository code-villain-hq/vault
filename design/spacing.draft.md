# Spacing — draft proposal

Status: visual mock for discussion. No spacing tokens are published or
applied to the apps. See the [interactive study](../brand/index.html#spacing).

## One scale, two densities

Proposed scale in px:

`4 · 8 · 12 · 16 · 24 · 32 · 48 · 64 · 96`

Use rem equivalents on the web. Native apps use their logical layout
units. The numbers describe relationships, not a reason to add empty space.

| Relationship | Starting values | Examples |
| --- | --- | --- |
| Closely related details | 4, 8 | Meta and title; icon and label; image and caption. |
| A small group | 12, 16 | Field groups; app panel padding. |
| Separate groups or framing | 24, 32 | Image surround; groups in a studio composition. |
| A major section | 48, 64 | 64 on a studio page, reducing to 48 on narrow screens. |
| A change of subject | 96 | A deliberate break between major stories. Use sparingly. |

## App example

- Outer padding: 16.
- Related text: 4.
- Between groups: 12.
- Between sections: 32.

## Studio example

- Outer padding: 32, reducing to 16 on narrow screens.
- Related text: 8.
- Between groups: 24.
- Between sections: 64, reducing to 48 on narrow screens.

The visual study holds type constant so the spacing difference can be
judged on its own. Its shaded areas are measurement guides, not a new
product surface treatment.

## Boundaries

Existing control heights, type leading, and logo clear space retain their
own rules. Fluid page gutters can interpolate between scale values;
illustration geometry and hairline strokes are not spacing tokens.

If adopted, publish the agreed values as JSON and generate CSS and Swift
with the other tokens. Do not promote the study’s local CSS variables to
production by copying them.
