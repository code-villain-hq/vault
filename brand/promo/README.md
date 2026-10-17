# Promo

Images made to be shared or set: wallpapers now, and other promo images
later. They are studio surfaces, so they stay Nightshade night and follow
the rest of [`brand/`](../README.md): the strike is each image's one brand
moment, and nothing else uses poison.

Each image is generated from the brand sources by a script beside it, so
a change to the mark reaches every image. Don't edit the outputs by hand.

## Wallpapers

[`wallpapers/`](wallpapers) has two designs, each for three screens. Both
use the marks' own construction: ink letters on night, cut into a poison
strike that runs edge to edge, the way the homepage logo extends it.

- **Mark**: CV from [`mark/mark.svg`](../mark/mark.svg), right of centre on
  desktop, clear of windows' usual middle; low on the phone, under the clock.
- **Wordmark**: "Code Villain" from [`mark/logo-night.svg`](../mark/logo-night.svg),
  centred. The strike shows between the letters, as in the logo.

| Screen | Size |
| --- | --- |
| `desktop-5k` | 5120 × 2880, 16:9 displays |
| `macbook` | 3456 × 2234, MacBook Pro 16" |
| `phone` | 1320 × 2868, phones |

Files are named `code-villain-<design>-<screen>`, as SVG and PNG. To
regenerate, run `python3 build.py` from `wallpapers/`; it needs Google
Chrome to render the PNGs.
