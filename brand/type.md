# Type

Display: Grenze Gotisch. Gothic letters from the castle with the iron mask.
Reading: Atkinson Hyperlegible Next. Plain, open, easy on night.
Code: Atkinson Hyperlegible Mono.

The gothic is the costume. Atkinson is the person inside it. Grenze
sets the splash, page titles, and section titles, and nothing smaller.
Atkinson sets every sentence meant to be read, and every label,
button, and nav item.

All three are free under the SIL Open Font License. The site serves
them, and Caster bundles them.

## Faces

| Face                       | Weights  | Use                                       |
| -------------------------- | -------- | ----------------------------------------- |
| Grenze Gotisch             | 700, 800 | Splash, page titles, section titles.      |
| Atkinson Hyperlegible Next | 400, 700 | Body, labels, buttons, card titles, meta. |
| Atkinson Hyperlegible Mono | 400      | Code and terminal text.                   |

## Rules

- Grenze never goes below 28px. Under that, use Atkinson 700.
- Grenze is never set in capitals. Blackletter capitals in a row turn unreadable.
- 800 belongs to the splash alone: the stacked Villain Studios lettering, a social card headline. One per surface.
- Titles are sentence case.
- Atkinson italic carries emphasis in prose. Atkinson 700 carries strong emphasis.

## Roles

| Role     | Face     | Weight | Size     | Leading | Use                         |
| -------- | -------- | ------ | -------- | ------- | --------------------------- |
| splash   | Grenze   | 800    | 72–184px | 0.95    | Cover lettering.            |
| display  | Grenze   | 700    | 36–56px  | 1.1     | Page and post titles.       |
| headline | Grenze   | 700    | 28–34px  | 1.18    | Section titles.             |
| title    | Atkinson | 700    | 21px     | 1.25    | Card titles, product names. |
| label    | Atkinson | 700    | 15px     | 1.2     | Buttons, chips, nav.        |
| body     | Atkinson | 400    | 17px     | 1.6     | Reading text. 66ch measure. |
| small    | Atkinson | 400    | 14px     | 1.5     | Captions, dates, meta.      |
| code     | Mono     | 400    | 14px     | 1.6     | Code blocks, terminal text. |

Ranges grow with the window. In an app window, use the largest size.
The numbers live in [`design/tokens/type.json`](../design/tokens/type.json).
