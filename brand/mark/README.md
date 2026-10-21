# Mark

The strike. A gothic C and V with a poison bar behind them, cut where the letters cross: Code Villain's initials as its compact mark. The full wordmark reads “Code Villain”. Naming rules live in [`../name.md`](../name.md).

| File | What it is |
| --- | --- |
| [`mark.svg`](mark.svg) | CV on its night tile. The app icon shape and the favicon. |
| [`forge.svg`](forge.svg) | Forge's outlined F on the same night tile with the strike. |
| [`layer.svg`](layer.svg) | Layer's L, built the same way. |
| [`caster.svg`](caster.svg) | Caster's C, built the same way. |
| [`glyph.svg`](glyph.svg) | Glyph's G, built the same way. |
| [`raven.svg`](raven.svg) | Raven's R, built the same way. |
| [`logo-night.svg`](logo-night.svg) | “Code Villain” with the strike, for night grounds. |
| [`logo-day.svg`](logo-day.svg) | The same for day grounds. |
| [`logo-home.svg`](logo-home.svg) | Homepage variant on night: cropped lettering with an extended strike. |

Every letter is outlined from Grenze Gotisch 800. The files render without any font installed. The wordmark is one line, with a space between Code and Villain; it has no Studios subtitle.

## Construction

On a 200-unit tile:

- **Tile**: night `bg` `#0B0B0B`, corner radius 45.
- **Letters**: Grenze Gotisch 800. A product mark is its initial at 150 units, centred on its advance, baseline at 152. The studio mark is C and V at 0.88 of that, 1 unit apart, the pair centred with its middle at y=104.5. Letters night `fg` `#EBEBEB`.
- **Bar**: poison `#C9E265`, 15 units tall, top at 108, edge to edge. It sits behind the letters.
- **Cut**: the letters cut the bar with a 7-unit gap on each side of their strokes. In CV the two cuts meet, so the bar stops between the letters and the pair reads as one.

Black and white are the apps' own `bg` and `fg` from [`color.md`](../color.md): achromatic, so a mark beside an app's frame or text matches it exactly. Only the strike carries colour.

The bar is low and heavy: it crosses the letters below the middle. `python3 products.py` writes the tile marks (`mark.svg` and the product marks) from the font. The logo files are kept by hand.

The V alone was the mark until October 2026. It said Villain but not Code, so the C joined it after the rename.

The wordmark uses Grenze Gotisch 800 at 72 units, on a `348 × 72` viewBox. The strike is 8 units tall; its bottom edge sits 17.28 units above the baseline, its centre 21.28. It runs past both ends of the name, with a 7-unit stroke around the outlined letters cutting a gap in the bar.

## Use

- On night, `#EBEBEB` letters and bright poison. On day, `#181818` letters and deep poison `#4C6A00`.
- The strike is the surface's one brand moment. Nothing else on the same surface uses poison as decoration.
- Clear space: a quarter of the mark's height on every side, kept empty.
- Smallest sizes: the tile works down to 16px. The full wordmark needs 160px of width; under that, use the tile.
- Never recolour the bar, move it, tilt it, or put it over the letters.

## Homepage variant

`logo-home.svg` preserves the night wordmark’s outlined lettering, colours, bar height, and cut. Its viewBox is cropped to the lettering, so the C aligns with the homepage gutter. The strike extends horizontally to the edges of the hero; the hero clips it there. This is the homepage’s one brand moment, following “small details, everywhere” and “display for headings” in [`design/principles.md`](../../design/principles.md).

The site references the asset’s `homepage-logo` group from an inline SVG with the same viewBox and visible overflow. Keep the artwork in this asset rather than copying the paths into the page. Serve both from the same origin.

This variant is the exception to the standard logo’s clear space at the horizontal ends of the strike. Use `logo-night.svg` for ordinary placements and `logo-day.svg` on day grounds.

## App icons

Each app's icon is its initial built the same way: Forge is an F, Layer an L, Caster a C, Glyph a G, Raven an R, each on the night tile with the strike behind it.

`python3 products.py` writes all five from the font with `outline.swift`
(Swift and CoreText, nothing to install). Each app keeps a copy and builds
its native icons from it:

| App | Copy | Native icons |
| --- | --- | --- |
| Forge | `apps/forge/apps/web/public/icon.svg` | `bun run design:sync`; desktop bundle from the same mark |
| Layer | `apps/layer/src-tauri/icons/icon.svg` | `bun tauri icon src-tauri/icons/icon.svg` (keep the macOS and Windows sizes) |
| Caster | `apps/caster/Resources/AppIcon.svg` | `make icon` |
| Glyph | `apps/glyph/apps/web/public/icon.svg` | `bun run design:sync` |
| Raven | `apps/raven/apps/web/public/icon.svg` | `bun run design:sync` |

An app icon is the product mark as drawn: the full tile, the strike edge to edge. The tile is the app's night `bg`, so on a night page it blends in and only the letter and its strike show. That is intended. Never add a border, a gradient, a shadow, or a second colour to lift it.

A product mark is that product's name in picture form, not decoration.
It keeps its strike wherever the product is shown, so a page that lists
the products shows each mark's strike beside the page's own brand moment.

How the mark was chosen: the [mark study](https://claude.ai/artifact/NGociCBQhTY3Qhd28DrfLr) (four concepts, then the strike refined).

## Menu bar glyph

A menu bar or tray icon is the product mark without its tile: the letter with the strike behind it, cut the same way, the bar no wider than the icon.

- 18 × 18 points. The letter fills 16 of them, centred. The bar, cut, and their position scale with it from the 200-unit construction.
- In colour, not a monochrome template: the strike is the product's name and stays poison. On a dark menu bar, `#EBEBEB` letters and bright poison; on a light one, `#181818` letters and deep poison `#4C6A00`. Follow the bar's appearance, not the app's theme.
- Drawn from the same outlined letter as the app icon, never retyped from the font, so the two never drift.
- Nothing else in the glyph: no badge, no count, no state colour. Status lives in the menu it opens.

