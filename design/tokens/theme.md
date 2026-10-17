# Calculated themes

Forge is the proving ground for the shared system. A theme changes through
three inputs in `color.json`, rather than an independently edited list of
night and day colours. The calculation follows the approach described in
[Linear's redesign](https://linear.app/now/how-we-redesigned-the-linear-ui):
a small set of inputs, shared operations, and consistent role aliases.
These are Code Villain's recipes, not Linear's implementation.

```json
"theme": {
  "name": "Nightshade",
  "base": "{poison}",
  "accent": "{cloak}",
  "contrast": 50
}
```

| Input | Controls |
| --- | --- |
| `base` | The primary action and selection colour family. Neutral surfaces and text stay achromatic in both modes. A light base (OKLCH lightness 0.7 and up, like poison) is a fill in its own right: the action wears it as is with dark ink; a darker base is mixed into the surfaces. Accepts a six-digit hex or a palette reference such as `{poison}`. |
| `accent` | Focus and live signals. Never mixed into neutral surfaces, body text or large fills. Accepts hex or a palette reference. |
| `contrast` | A number from 0 to 100. Widens surface steps and text separation around the default of 50. The text contrast floor remains at least 4.6:1; from 50 to 100 the target rises to 7:1. |

The studio palette remains the identity source. A theme's resolved values
are application roles; they do not replace the canonical mark or studio
palette. Semantic meanings (teal done, amber waiting, red error, blue
information) have one seed each, shared by both modes. They are not new
inputs every time someone changes a theme.

## The recipes

`theme.py` owns the relationships, in this order:

1. Build the neutral scale in OKLCH with zero chroma. The lightness curve
   defines frame, panel, surface, hover and line. Base and accent colours
   never tint these large surfaces. Contrast widens the steps while keeping the frame anchored
   and the day panel white.
2. Derive primary, primary hover and selection from the base. A dark base
   is mixed into the appropriate ground; a light base is the primary fill
   itself, lifted if needed so dark ink reads on it, and only a faint tint
   of it marks selection. Neither turns into a large, fully saturated area.
3. Derive dialog and field aliases from that hierarchy. Fields inset into
   a dialog keep a different tone from the dialog itself.
4. Calculate text, icon and accent colours against every supported host;
   `signal`, for live dots, against the 3:1 non-text floor.
   Move perceptual lightness only as far as needed for contrast, preserving
   hue. Reduce chroma to fit sRGB rather than clipping channels. Keep primary
   fills readable even with extreme input colours.
5. Derive status colours from their semantic seeds, then check status ink
   on the actual 13% fill composites. Check quiet chip text on its 7% fill
   composites, and all text on the 8% active-option highlight, too. Components keep these fills translucent at runtime.

The lightness curve and mix strengths are authored system rules. Theme
inputs change their result, not the hierarchy or the meanings of the roles.
The conversion uses OKLab/OKLCH as described in
[CSS Color 4](https://www.w3.org/TR/css-color-4/#color-conversion-code).

## Build and verify

From this directory:

```sh
python3 -m unittest discover -s . -p 'test_*.py'
python3 build.py
```

The builder emits `color.css`, `color.swift` and `color.resolved.json` from
the same calculation. Never edit them independently. Forge then bundles the
outputs with `bun run design:sync` from `apps/forge`; its existing role names
continue to work. Theme editing is a build-time capability, not a new
runtime theme picker in Forge.

Drift fails the build. `python3 build.py --check` writes nothing and names
any output that differs from a fresh build; `bun scripts/sync-design.ts
--check` does the same for Forge's bundled copy. Forge's `bun run check`
runs both, and the `design` workflow on Forgejo runs them, with the theme
tests, on every push that touches the tokens, the mark, the fonts or
Forge's copy.

Test alternate hues and contrast extremes, then inspect real Forge lists,
boards, detail views, settings and dialogs in night and day. A numerical
contrast pass establishes readability; the real views establish whether
hierarchy and restraint still feel right. Bring broader visual changes back
for discussion. Mobile redesign remains separate work.
