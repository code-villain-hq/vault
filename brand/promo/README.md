# Promo

Images made to be shared or set: wallpapers now, and other promo images
later. They are studio surfaces, so they stay Nightshade night and follow
the rest of [`brand/`](../README.md): the strike is each image's one brand
moment, and nothing else uses poison.

The original wallpaper set is generated from the brand sources by a script
beside it, so a change to the mark reaches every image. Don't edit those
outputs by hand. The engraved editions below use generated raster sources;
the Fold wordmark is composed from the original outlined brand lettering.

## Wallpapers

[`wallpapers/`](wallpapers) has four designs, each for three screens (the
cubes in two palettes). The
first three use the marks' own construction: `fg` letters on `bg`, cut
into a poison strike that runs edge to edge, the way the homepage logo
extends it.

- **Mark**: CV from [`mark/mark.svg`](../mark/mark.svg), right of centre on
  desktop, clear of windows' usual middle; low on the phone, under the clock.
- **Wordmark**: "Code Villain" from [`mark/logo-night.svg`](../mark/logo-night.svg),
  centred. The strike shows between the letters, as in the logo.
- **Rows**: the wordmark design on a wall of staggered grey wordmarks, a
  step above night. They run up to the lit name and its strike, with the
  brand's thin cut around both, so it reads as one name called out of many.
- **Cubes**: a pattern alone, with no letters. Tumbling blocks lit from
  the left, in two palettes: `cubes-poison` (the brand colour fading into
  dark olive) and `cubes-cloak` (the supporting violet fading into wash).
  The top is one engraved face, its stripes parallel to the back-right
  edge and thinning towards the front; the lit face turns into shade
  through a fan of rays from the bottom corner.

| Screen | Size |
| --- | --- |
| `desktop-5k` | 5120 × 2880, 16:9 displays |
| `macbook` | 3456 × 2234, MacBook Pro 16" |
| `phone` | 1320 × 2868, phones |

Files are named `code-villain-<design>-<screen>`, as SVG and PNG. To
regenerate, run `python3 build.py` from `wallpapers/`; it needs Google
Chrome to render the PNGs.

## Engraved editions

The selected Fold and cube studies live directly in `wallpapers/`:

| File | Size | Design |
| --- | --- | --- |
| [`code-villain-fold-poison.png`](wallpapers/code-villain-fold-poison.png) | 1672 × 941 | Engraved violet folds with a poison line |
| [`code-villain-fold-poison-wordmark.png`](wallpapers/code-villain-fold-poison-wordmark.png) | 1672 × 941 | The same image with the wordmark on the left |
| [`code-villain-cubes-cloak-engraved.png`](wallpapers/code-villain-cubes-cloak-engraved.png) | 1560 × 1008 | The original cube geometry with Fold's engraved texture |

These retain their generated resolution. Their source image and imagegen
prompts are in [`wallpapers/sources/`](wallpapers/sources/). Rebuild the
Fold wordmark's SVG and PNG with `python3 build_fold.py` from `wallpapers/`;
it needs Google Chrome. The original `build.py` does not overwrite these
editions.
