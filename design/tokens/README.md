# Tokens

The numbers taken from `brand/`: color, type, shape.

| Tokens | From | Files |
| --- | --- | --- |
| Color (Nightshade, night and day) | [`brand/color.md`](../../brand/color.md) | [`color.json`](color.json), [`color.resolved.json`](color.resolved.json), [`color.css`](color.css), [`color.swift`](color.swift) |
| Type | [`brand/type.md`](../../brand/type.md) | [`type.json`](type.json), [`type.css`](type.css), [`type.swift`](type.swift) |
| Shape (corner radius) | [`components.md`](../components.md) | [`shape.json`](shape.json), [`shape.css`](shape.css), [`shape.swift`](shape.swift) |

The JSON is the source. Edit it, then run `python3 build.py` here to regenerate every CSS and Swift file. Don't edit the generated files.

In CSS, night is the default. An app's light mode sets `data-theme="day"` on its root. Studio surfaces stay night.

`color.json` defines Nightshade through three inputs under `theme`: `base`,
`accent`, and `contrast` (0–100; default 50). `{cloak}` and `{poison}` refer
to studio palette colours; a custom theme can supply six-digit hex values.
Change these inputs, not individual output roles. [`theme.py`](theme.py)
calculates the neutral hierarchy, primary/selection tints, text, focus and
semantic colours, and status ink for both modes. The fixed recipes belong
to the design system and are shared by all themes.

The source also publishes tint opacities (`status-tint`, `chip-tint`,
`highlight-tint`),
semantic hue seeds and a minimum text contrast of 4.6:1, leaving room above
AA's 4.5:1 floor. Contrast 100 raises that target to 7:1. Live dots use
`signal`, held at the 3:1 non-text floor.
CSS exposes the opacities as percentages; Swift exposes fractions in
`ColorTokens.Opacity` and the target in `ColorTokens.Contrast`. Apps needing
numerical colours consume generated `color.resolved.json`, not the source
inputs. The CSS, Swift and resolved JSON share the same generated values.
Compose tints over the actual host instead of storing an opaque composite.
Use the ink tokens for status text, keeping the original semantic hue as
the tint source. See [`components.md`](../components.md) for contrast and
custom-colour rules.

[`theme.md`](theme.md) explains the calculation and how to change a theme.
Run `python3 -m unittest discover -s . -p 'test_*.py'` here to test the
generator against alternate hues, black/white seeds and contrast extremes.

Type tokens name the faces. Each surface loads the font files itself.

The space scale is not published yet. A [draft proposal](../spacing.draft.md) and [interactive study](../../brand/index.html#spacing) are available for discussion.
