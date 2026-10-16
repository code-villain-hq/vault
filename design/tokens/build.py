"""Regenerate color.css/.swift and shape.css/.swift from their JSON.

Run from this folder: python3 build.py
A stopgap until @villain/tokens builds these with Style Dictionary.
type.css and type.swift are still written by hand.
"""
import json
import re

camel = lambda k: re.sub(r"-(\w)", lambda m: m.group(1).upper(), k)

c = json.load(open("color.json"))
s = json.load(open("shape.json"))

# Palette colours that share a name with a semantic token are covered by it in CSS.
palette_only = [k for k in c["palette"] if k not in c["night"]]
css = ['/* Generated from color.json. Night is the default; set data-theme="day" for the apps\' light mode. */', ":root {"]
css += [f"  --{k}: {c['palette'][k]};" for k in palette_only]
css += ["}", "", ":root,", '[data-theme="night"] {', "  color-scheme: dark;"]
css += [f"  --{k}: {v};" for k, v in c["night"].items()]
css += ["}", "", '[data-theme="day"] {', "  color-scheme: light;"]
css += [f"  --{k}: {v};" for k, v in c["day"].items()]
css += ["}", ""]
open("color.css", "w").write("\n".join(css))

sw = ["import Foundation", "", "// Generated from color.json.", "enum ColorTokens {", "  enum Palette {"]
sw += [f'    static let {camel(k)} = "{v}"' for k, v in c["palette"].items()]
sw += ["  }", "", "  struct Theme {"]
sw += [f"    let {camel(k)}: String" for k in c["night"]]
sw += ["  }", ""]
for mode in ("night", "day"):
    args = ",\n    ".join(f'{camel(k)}: "{v}"' for k, v in c[mode].items())
    sw += [f"  static let {mode} = Theme(\n    {args}\n  )", ""]
sw[-1] = "}"
open("color.swift", "w").write("\n".join(sw) + "\n")

open("shape.css", "w").write(
    "/* Generated from shape.json. */\n:root {\n"
    + "".join(f"  --radius-{k}: {v}px;\n" for k, v in s["radius"].items())
    + "}\n"
)
open("shape.swift", "w").write(
    "import Foundation\n\n// Generated from shape.json.\nenum ShapeTokens {\n"
    + "".join(f"  static let radius{k.capitalize()}: Double = {v}\n" for k, v in s["radius"].items())
    + "}\n"
)
