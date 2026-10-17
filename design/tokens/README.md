# Tokens

The numbers taken from `brand/`: color, type, shape.

| Tokens | From | Files |
| --- | --- | --- |
| Color (Nightshade, night and day) | [`brand/color.md`](../../brand/color.md) | [`color.json`](color.json), [`color.resolved.json`](color.resolved.json), [`color.css`](color.css), [`color.swift`](color.swift) |
| Type | [`brand/type.md`](../../brand/type.md) | [`type.json`](type.json), [`type.css`](type.css), [`type.swift`](type.swift) |
| Shape (corner radius) | [`components.md`](../components.md) | [`shape.json`](shape.json), [`shape.css`](shape.css), [`shape.swift`](shape.swift) |

The JSON is the source. Edit it, then run `python3 build.py` here to regenerate every CSS and Swift file. Don't edit the generated files.

In CSS, night is the default. An app's light mode sets `data-theme="day"` on its root. Studio surfaces stay night.

`color.json` also publishes shared tint opacity recipes (`status-tint`,
`chip-tint`), dialog/field aliases, and the status text contrast target.
Role references such as `{surface}` reuse an existing colour. The builder
resolves them and derives status ink from the hue, tint opacity and actual
host composites, with a 4.6:1 target to leave room above AA's 4.5:1 floor.
CSS exposes the opacities as percentages; Swift exposes fractions in
`ColorTokens.Opacity` and the target in `ColorTokens.Contrast`. Apps needing
numerical colours consume generated `color.resolved.json`, not the source
aliases. The CSS, Swift and resolved JSON share the same generated values.
Compose tints over the actual host instead of storing an opaque composite.
Use the ink tokens for status text, keeping the original semantic hue as
the tint source. See [`components.md`](../components.md) for contrast and
custom-colour rules.

Type tokens name the faces. Each surface loads the font files itself.

The space scale is not published yet. A [draft proposal](../spacing.draft.md) and [interactive study](../../brand/index.html#spacing) are available for discussion.
