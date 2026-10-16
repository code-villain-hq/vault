# Tokens

The numbers taken from `brand/`: color, type, shape.

| Tokens | From | Files |
| --- | --- | --- |
| Color (Nightshade, night and day) | [`brand/color.md`](../../brand/color.md) | [`color.json`](color.json), [`color.css`](color.css), [`color.swift`](color.swift) |
| Type | [`brand/type.md`](../../brand/type.md) | [`type.json`](type.json), [`type.css`](type.css), [`type.swift`](type.swift) |
| Shape (corner radius) | [`components.md`](../components.md) | [`shape.json`](shape.json), [`shape.css`](shape.css), [`shape.swift`](shape.swift) |

The JSON is the source. Edit it, then run `python3 build.py` here to regenerate the color and shape CSS and Swift. Type's CSS and Swift are still written by hand.

In CSS, night is the default. An app's light mode sets `data-theme="day"` on its root. Studio surfaces stay night.

Type tokens name the faces. Each surface loads the font files itself.

The space scale is not published yet. It waits on a deliberate spacing decision.
