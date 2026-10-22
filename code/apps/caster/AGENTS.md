# Caster: notes for coding agents

Caster is the AppKit launcher: a menu bar app with one panel on ⌘Space. Run commands from this folder. The README covers features, building, and permissions.

## Look

Caster wears Nightshade in night and day, following the system's appearance. Read `../../../studio/design/principles.md` and `../../../studio/design/components.md` before any UI change, and `../../../studio/design/behaviour.md` before changing what Caster does: errors, empty states, keys, settings.

- `Sources/VillainCaster/Design/` holds copies of the generated Swift tokens. Never edit them. Change `studio/design/tokens/*.json`, run `python3 build.py` there, then `make design` here.
- `Nightshade.swift` turns the tokens into dynamic `NSColor`s and Atkinson fonts. Take colours, type, and radii from it. Don't use system label colours, system fonts, or hand-typed hex values.
- `Resources/Fonts/` holds Atkinson Hyperlegible Next and its license, copied from `code/apps/site/assets/fonts`. `make app` bundles them, and `Nightshade.registerFonts()` loads them at launch.
- The panel is a solid `dialog` tone with `radius-lg` corners: no glass, no dividers, no window shadow (it brings a light rim). Its layout is Forge's command menu (`code/apps/forge/apps/web/src/command-menu.tsx`) scaled up a step for the desktop: 680 wide, a 60pt input in the `query` role, one-line 44pt rows at the `body` size in an 8pt inset. The active row is `fg` at `highlight-tint`. Keep the two in step.
- The menu bar glyph is the C from `studio/brand/mark/caster.svg` with its strike, drawn in colour (see "Menu bar glyph" in `studio/brand/mark/README.md`). If the mark changes, run `make design` and copy the new C path into `MenuBarIcon.swift`. Then run `make icon` for the app icon.

`make design-check` writes nothing and fails if any copy has drifted from the vault. CI runs it.

## Seeing a change

The panel can't be driven from a session (⌘Space belongs to the user). Render it instead:

```sh
make build && cp -R Resources/Fonts .build/release/
.build/release/VillainCaster -preview "help" -previewTheme day -previewOut /tmp/panel.png
```

`-preview` takes a query, `settings`, or `menubar`. Nothing is shown on screen, the keyboard is never taken, and no hot key is registered, so it runs beside an installed Caster. The site's captures in `code/apps/site/assets/captures/caster-*.png` are rendered this way. Leave out queries that show personal data, such as weather (your city) or time (your offset).
