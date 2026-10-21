# Type

Display: Grenze Gotisch. Gothic letters from the castle with the iron mask.
Reading: Atkinson Hyperlegible Next. Plain, open, easy on night.
Code: Atkinson Hyperlegible Mono.

The gothic is the costume. Atkinson is the person inside it. Grenze
sets the splash, page titles, and section titles, and nothing smaller.
Atkinson sets every sentence meant to be read, and every label,
button, and nav item.

All three are free under the SIL Open Font License. The site serves
them from [`apps/site/assets/fonts/`](../apps/site/assets/fonts/), with
the licenses alongside them. The brand page reuses the same local
font stylesheet. Forge, Layer and Glyph bundle all three; Caster bundles
Atkinson Hyperlegible Next, the only face it sets.

## Faces

| Face                       | Weights       | Use                                       |
| -------------------------- | ------------- | ----------------------------------------- |
| Grenze Gotisch             | 700, 800      | Splash, page titles, section titles.      |
| Atkinson Hyperlegible Next | 400, 600, 700 | Body, labels, buttons, card titles, meta. |
| Atkinson Hyperlegible Mono | 400           | Code and terminal text.                   |

## Rules

- Grenze never goes below 28px. Under that, use Atkinson 700.
- Grenze is never set in capitals. Blackletter capitals in a row turn unreadable.
- 800 belongs to the splash alone: the logo set large on the homepage, a social card headline. One per surface.
- Titles are sentence case.
- Atkinson italic carries emphasis in prose. Atkinson 700 carries strong emphasis. 600 is for controls.

## Roles

Studio surfaces (the site, the blog, social cards) read at a larger
size. App windows are denser and use the interface roles.

### Studio

| Role     | Face     | Weight | Size     | Leading | Use                          |
| -------- | -------- | ------ | -------- | ------- | ---------------------------- |
| splash   | Grenze   | 800    | 72–184px | 0.95    | Cover lettering.             |
| display  | Grenze   | 700    | 36–56px  | 1.1     | Page and post titles.        |
| headline | Grenze   | 700    | 28–34px  | 1.18    | Section titles.              |
| title    | Atkinson | 700    | 21px     | 1.25    | Card titles, product names, the homepage nav. |
| label    | Atkinson | 700    | 17px     | 1.2     | Site buttons, inner-page nav. |
| body     | Atkinson | 400    | 17px     | 1.6     | Reading text. 66ch measure.  |
| small    | Atkinson | 400    | 14px     | 1.5     | Captions, dates, meta.       |
| code     | Mono     | 400    | 14px     | 1.6     | Code blocks, terminal text.  |

### Interface

| Role     | Face     | Weight | Size | Leading | Use                                  |
| -------- | -------- | ------ | ---- | ------- | ------------------------------------ |
| wordmark | Grenze   | 700    | 28px | 1.1     | The app's name in the sidebar (in the browser; the desktop apps show the traffic lights there), an empty state's headline. |
| title    | Atkinson | 700    | 21px | 1.25    | View and issue titles.               |
| query    | Atkinson | 400    | 21px | 1.25    | The query in a launcher or command menu. |
| ui       | Atkinson | 400    | 14px | 1.45    | Rows, nav, descriptions, fields.     |
| ui-small | Atkinson | 400    | 13px | 1.4     | Meta, ids, counts, captions.         |
| control  | Atkinson | 600    | 13px | 1       | Buttons, chips. Status pills at 700. |
| ui-code  | Mono     | 400    | 13px | 1.6     | Code, terminal, ids in mono.         |

A launcher sits alone on the desktop rather than inside an app, so
Caster's rows and answers take the studio `body` size under its `query`.

Grenze appears in an app only at 28px and up: the app's name in the
sidebar, an empty state's headline.

Studio ranges grow with the window. The numbers live in
[`design/tokens/type.json`](../design/tokens/type.json).
