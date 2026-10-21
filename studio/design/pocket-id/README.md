# Pocket ID

Pocket ID at id.codevillain.eu is the one sign-in page every app sends
people to. It isn't ours to restyle: it has no custom CSS, and its fonts,
greys and layout stay its own. What it does let us set, we set from
`studio/brand/`, so the page reads as Code Villain.

`python3 build.py` writes `kit/` from the brand sources. It needs Google
Chrome and macOS `sips`.

## Settings

Admin → Application Configuration → General:

| Setting | Value |
| --- | --- |
| Application name | `Code Villain` |
| Accent color | `#C9E265`, poison. Pocket ID puts dark ink on it by itself. |
| Disable animations | On |

Admin → Application Configuration → Images:

| Slot | File | What it is |
| --- | --- | --- |
| Light mode logo | `kit/logo-light.svg` | CV without its tile: day `fg` letters, deep poison strike |
| Dark mode logo | `kit/logo-dark.svg` | CV without its tile: night `fg` letters, poison strike |
| Email logo | `kit/logo-email.png` | The tile, 112px; emails show it at 28px |
| Favicon | `kit/favicon.svg` | The tile, as everywhere |
| Background image | `kit/background.jpg` | The engraved cubes panel, from `studio/brand/promo/wallpapers` |

The logos are cropped close to the letters because Pocket ID draws them at
40px inside its own 64px tile.

## What stays Pocket ID's

- Fonts, greys, radius and layout.
- The focus ring takes the accent colour, so it is poison, not cloak.
- The background is a raster at 900 × 941. Large or high-density screens
  scale it up and soften the engraving.
