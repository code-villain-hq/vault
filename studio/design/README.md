# Design

`studio/brand/` says what Code Villain feels like. This folder turns that into principles, numbers, and reference layouts.

Caster is AppKit. Forge, Layer, Glyph, and the site are web. The Code Villain theme (`code/apps/theme`) carries the same colours into VS Code, Cursor, Neovim, WezTerm and Starship. Each one builds its screens in its own toolkit, from the same principles, tokens, and layouts.

| | |
| --- | --- |
| [`principles.md`](principles.md) | How a screen is designed, and why |
| [`components.md`](components.md) | Button, field, link, pill, chip, row, icon |
| [`behaviour.md`](behaviour.md) | What every app does alike: sign-in, errors, empty states, keys, settings, sync |
| [`components.css`](components.css) | Those controls for the web, as shared `ui-*` classes |
| [`lint/`](lint/README.md) | The CI guard against literal colours, sizes and radii |
| [`tokens/`](tokens) | The numbers: color, type, shape |
| [`layouts/`](layouts) | Reference layouts: the app window and the blog; a social card to come |
| [`pets/`](pets) | Animated studio companions, source artwork, and previews |
| [`pocket-id/`](pocket-id/README.md) | The settings and images that dress the studio sign-in page |

A [spacing proposal](spacing.draft.md) is available as a draft, with [interactive examples](../brand/index.html#spacing). No space tokens are published yet.
