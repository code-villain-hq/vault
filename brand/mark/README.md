# Mark

The strike. A gothic V with a poison bar behind it, cut where the
letter crosses. The homepage's strike through "Villain", made into a
mark.

| File | What it is |
| --- | --- |
| [`mark.svg`](mark.svg) | The mark on its night tile. The app icon shape and the favicon. |
| [`logo-night.svg`](logo-night.svg) | "Villain" with the strike, "Studios" under it, for night grounds. |
| [`logo-day.svg`](logo-day.svg) | The same for day grounds. |

Every letter is outlined: the V and "Villain" from Grenze Gotisch 800,
"Studios" from Atkinson Hyperlegible Next 600. The files render
without any font installed.

## Construction

On a 200-unit tile:

- **Tile**: night `#0A0B0D`, corner radius 45.
- **V**: Grenze Gotisch 800, 150 units, centred, baseline at 152. Ink `#ECECF1`.
- **Bar**: poison `#C9E265`, 15 units tall, top at 108, edge to edge. It sits behind the V.
- **Cut**: the V cuts the bar with a 7-unit gap on each side of its strokes.

The bar is low and heavy: it crosses the V below the middle, where
the strokes come together.

The logo uses the same strike at the size of "Villain": the bar is
0.11 of the letter size, its centre 0.24 of it above the baseline,
and it runs past both ends of the word. "Studios" sits under it in
Atkinson Hyperlegible Next 600, capitals, tracked 0.32em.

## Use

- On night, ink letters and bright poison. On day, `#18181C` letters and deep poison `#4C6A00`.
- The strike is the surface's one brand moment. Nothing else on the same surface uses poison as decoration.
- Clear space: a quarter of the mark's height on every side, kept empty.
- Smallest sizes: the tile works down to 16px. The logo needs 96px of width; under that, use the tile.
- Never recolour the bar, move it, tilt it, or put it over the letter.

## App icons

Each app's icon is its initial built the same way: Forge is an F, Layer
an L, Caster a C, each on the night tile with the strike behind it.

Today's icons stay until each app moves over:

| App | Icon now |
| --- | --- |
| Forge | `apps/forge/apps/web/public/icon.svg`: an F on `#5b5bd6` |
| Layer | `apps/layer/src-tauri/icons/icon.svg`: a mask on stacked layers |
| Caster | `apps/caster/Resources/AppIcon.svg`: a lens with Layer's mask |

How the mark was chosen: the [mark study](https://claude.ai/artifact/NGociCBQhTY3Qhd28DrfLr) (four concepts, then the strike refined).
