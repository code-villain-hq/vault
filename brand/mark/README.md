# Mark

The strike. A gothic C and V with a poison bar behind them, cut where the letters cross: Code Villain's initials as its compact mark. The full wordmark reads “Code Villain”. Naming rules live in [`../name.md`](../name.md).

| File | What it is |
| --- | --- |
| [`mark.svg`](mark.svg) | CV on its night tile. The app icon shape and the favicon. |
| [`forge.svg`](forge.svg) | Forge's outlined F on the same night tile with the strike. |
| [`layer.svg`](layer.svg) | Layer's L, built the same way. |
| [`caster.svg`](caster.svg) | Caster's C, built the same way. |
| [`logo-night.svg`](logo-night.svg) | “Code Villain” with the strike, for night grounds. |
| [`logo-day.svg`](logo-day.svg) | The same for day grounds. |
| [`logo-home.svg`](logo-home.svg) | Homepage variant on night: cropped lettering with an extended strike. |

Every letter is outlined from Grenze Gotisch 800. The files render without any font installed. The wordmark is one line, with a space between Code and Villain; it has no Studios subtitle.

## Construction

On a 200-unit tile:

- **Tile**: night `#0A0B0D`, corner radius 45.
- **Letters**: Grenze Gotisch 800. A product mark is its initial at 150 units, centred on its advance, baseline at 152. The studio mark is C and V at 0.88 of that, 1 unit apart, the pair centred with its middle at y=104.5. Ink `#ECECF1`.
- **Bar**: poison `#C9E265`, 15 units tall, top at 108, edge to edge. It sits behind the letters.
- **Cut**: the letters cut the bar with a 7-unit gap on each side of their strokes. In CV the two cuts meet, so the bar stops between the letters and the pair reads as one.

The bar is low and heavy: it crosses the letters below the middle. `python3 products.py` writes every mark from the font.

The V alone was the mark until October 2026. It said Villain but not Code, so the C joined it after the rename.

The wordmark uses Grenze Gotisch 800 at 72 units, on a `348 × 72` viewBox. The strike is 8 units tall and centred 17.28 units above the baseline. It runs past both ends of the name, with a 7-unit stroke around the outlined letters cutting a gap in the bar.

## Use

- On night, ink letters and bright poison. On day, `#18181C` letters and deep poison `#4C6A00`.
- The strike is the surface's one brand moment. Nothing else on the same surface uses poison as decoration.
- Clear space: a quarter of the mark's height on every side, kept empty.
- Smallest sizes: the tile works down to 16px. The full wordmark needs 160px of width; under that, use the tile.
- Never recolour the bar, move it, tilt it, or put it over the letters.

## Homepage variant

`logo-home.svg` preserves the night wordmark’s outlined lettering, colours, bar height, and cut. Its viewBox is cropped to the lettering, so the C aligns with the homepage gutter. The strike extends horizontally to the edges of the hero; the hero clips it there. This is the homepage’s one brand moment, following “small details, everywhere” and “display for headings” in [`design/principles.md`](../../design/principles.md).

The site references the asset’s `homepage-logo` group from an inline SVG with the same viewBox and visible overflow. Keep the artwork in this asset rather than copying the paths into the page. Serve both from the same origin.

This variant is the exception to the standard logo’s clear space at the horizontal ends of the strike. Use `logo-night.svg` for ordinary placements and `logo-day.svg` on day grounds.

## App icons

Each app's icon is its initial built the same way: Forge is an F, Layer an L, Caster a C, each on the night tile with the strike behind it.

`python3 products.py` writes all three from the font with `outline.swift`
(Swift and CoreText, nothing to install). Each app keeps a copy and builds
its native icons from it:

| App | Copy | Native icons |
| --- | --- | --- |
| Forge | `apps/forge/apps/web/public/icon.svg` | `bun run design:sync`; desktop bundle from the same mark |
| Layer | `apps/layer/src-tauri/icons/icon.svg` | `bun tauri icon src-tauri/icons/icon.svg` (keep the macOS and Windows sizes) |
| Caster | `apps/caster/Resources/AppIcon.svg` | `make icon` |

A product mark is that product's name in picture form, not decoration.
It keeps its strike wherever the product is shown, so a page that lists
the products shows each mark's strike beside the page's own brand moment.

How the mark was chosen: the [mark study](https://claude.ai/artifact/NGociCBQhTY3Qhd28DrfLr) (four concepts, then the strike refined).
