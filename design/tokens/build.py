"""Regenerate the CSS and Swift for color, type, and shape, and the syntax colours, from their JSON.

Run from this folder: python3 build.py
`python3 build.py --check` writes nothing and fails if an output is stale.
A stopgap until @villain/tokens builds these with Style Dictionary.
"""
import json
import re
import sys
from theme import generate
from syntax import resolve

check = "--check" in sys.argv[1:]
stale = []


def emit(name, text):
    """Write a generated file, or in check mode note it if it differs."""
    if not check:
        open(name, "w").write(text)
        return
    try:
        current = open(name).read()
    except FileNotFoundError:
        current = None
    if current != text:
        stale.append(name)

camel = lambda k: re.sub(r"-(\w)", lambda m: m.group(1).upper(), k)

c = json.load(open("color.json"))
s = json.load(open("shape.json"))
t = json.load(open("type.json"))

source = c
c = generate(c)

# Apps that need numerical colour values consume the same resolved aliases as CSS/Swift.
emit("color.resolved.json", json.dumps(c, indent=2) + "\n")

# Palette colours that share a name with a semantic token are covered by it in CSS.
palette_only = [k for k in c["palette"] if k not in c["night"]]
css = ['/* Generated from color.json. Night is the default; set data-theme="day" for the apps\' light mode. */', ":root {"]
css += [f"  --{k}: {c['palette'][k]};" for k in palette_only]
css += [f"  --{k}: {v * 100:g}%;" for k, v in c["opacity"].items()]
css += ["}", "", ":root,", '[data-theme="night"] {', "  color-scheme: dark;"]
css += [f"  --{k}: {v};" for k, v in c["night"].items()]
css += ["}", "", '[data-theme="day"] {', "  color-scheme: light;"]
css += [f"  --{k}: {v};" for k, v in c["day"].items()]
css += ["}", ""]
emit("color.css", "\n".join(css))

sw = ["import Foundation", "", "// Generated from color.json.", "enum ColorTokens {", "  enum Palette {"]
sw += [f'    static let {camel(k)} = "{v}"' for k, v in c["palette"].items()]
sw += ["  }", "", "  enum Opacity {"]
sw += [f"    static let {camel(k)}: Double = {v}" for k, v in c["opacity"].items()]
sw += ["  }", "", "  enum Contrast {"]
sw += [f"    static let {camel(k)}: Double = {v}" for k, v in c["contrast"].items()]
sw += ["  }", "", "  struct Theme {"]
sw += [f"    let {camel(k)}: String" for k in c["night"]]
sw += ["  }", ""]
for mode in ("night", "day"):
    args = ",\n    ".join(f'{camel(k)}: "{v}"' for k, v in c[mode].items())
    sw += [f"  static let {mode} = Theme(\n    {args}\n  )", ""]
sw[-1] = "}"
emit("color.swift", "\n".join(sw) + "\n")

# Code: syntax and editor colours, for the editor themes and code in the apps.
x = resolve(json.load(open("syntax.json")), source, c)
emit("syntax.resolved.json", json.dumps(x, indent=2) + "\n")
css = ['/* Generated from syntax.json. Night is the default; set data-theme="day" for the apps\' light mode. */']
for mode, selector in (("night", ':root,\n[data-theme="night"]'), ("day", '[data-theme="day"]')):
    css += [selector + " {"]
    css += [f"  --syntax-{k}: {v};" for k, v in x[mode]["syntax"].items()]
    css += [f"  --ansi-{i}: {v};" for i, v in enumerate(x[mode]["ansi"]["normal"] + x[mode]["ansi"]["bright"])]
    css += [f"  --code-{k}: {v};" for k, v in x[mode]["ui"].items()]
    css += ["}", ""]
emit("syntax.css", "\n".join(css))

emit("shape.css",
    "/* Generated from shape.json. */\n:root {\n"
    + "".join(f"  --radius-{k}: {v}px;\n" for k, v in s["radius"].items())
    + "}\n"
)
emit("shape.swift",
    "import Foundation\n\n// Generated from shape.json.\nenum ShapeTokens {\n"
    + "".join(f"  static let radius{k.capitalize()}: Double = {v}\n" for k, v in s["radius"].items())
    + "}\n"
)

# Type: faces as font stacks, roles as size, weight, leading, and tracking.
rem = lambda px: f"{px / 16:g}rem"
face_var = {"display": "display", "reading": "reading", "mono": "mono"}
css = ["/* Generated from type.json. */", ":root {"]
for name, f in t["faces"].items():
    css.append(f'  --font-{face_var[name]}: "{f["family"]}", {f["fallback"]};')
for role, r in t["roles"].items():
    size = r["size"]
    value = f"clamp({rem(size['min'])}, {size['fluid']}, {rem(size['max'])})" if isinstance(size, dict) else rem(size)
    css += ["", f"  --type-{role}-size: {value};", f"  --type-{role}-weight: {r['weight']};", f"  --type-{role}-leading: {r['lineHeight']};"]
    if r["tracking"]:
        css.append(f"  --type-{role}-tracking: {r['tracking']}em;")
    if "measure" in r:
        css.append(f"  --type-{role}-measure: {r['measure']};")
css += ["}", ""]
emit("type.css", "\n".join(css))

# Swift: app windows use the largest size of each fluid range.
swift_role = lambda k: {"display": "displayTitle"}.get(k, camel(k))
sw = ["import Foundation", "", "// Generated from type.json. App windows use the largest size of each fluid range.", "enum TypeTokens {"]
for name, f in t["faces"].items():
    sw.append(f'  static let {name} = "{f["family"]}"')
sw += ["", "  struct Role {", "    let face: String", "    let weight: Int", "    let size: Double", "    let lineHeight: Double", "    let trackingEm: Double", "  }", ""]
for role, r in t["roles"].items():
    size = r["size"]["max"] if isinstance(r["size"], dict) else r["size"]
    sw.append(f"  static let {swift_role(role)} = Role(face: {r['face']}, weight: {r['weight']}, size: {size}, lineHeight: {r['lineHeight']}, trackingEm: {r['tracking']})")
sw.append("}")
emit("type.swift", "\n".join(sw) + "\n")

# DESIGN.md at the vault root: its frontmatter (the DESIGN.md format design
# tools read) is generated here from the night tokens, so it can't drift; the
# prose under it is written by hand and kept.
def design_md():
    night = c["night"]
    names = ["bg", "panel", "surface", "dialog", "field", "hover", "selected", "line", "fg", "muted", "faint",
             "primary", "primary-hover", "on-primary", "accent", "signal", "danger", "ok", "warn", "info"]
    out = ["---", "name: Code Villain",
           "description: Nightshade. Grey does the work, poison is the brand and the main action, cloak supports it and marks focus. Night by default, day for the apps.",
           "colors:"]
    out += [f'  {n}: "{night[n]}"' for n in names]
    out += [f'  {n}: "{source["palette"][n]}"' for n in ("cloak", "poison")]
    out.append("typography:")
    family = {k: f'{f["family"]}, {f["fallback"]}' for k, f in t["faces"].items()}
    for role, r in t["roles"].items():
        size = r["size"]
        value = f"clamp({rem(size['min'])}, {size['fluid']}, {rem(size['max'])})" if isinstance(size, dict) else rem(size)
        out += [f"  {role}:", f'    fontFamily: "{family[r["face"]]}"', f'    fontSize: "{value}"',
                f"    fontWeight: {r['weight']}", f"    lineHeight: {r['lineHeight']}"]
        if r["tracking"]:
            out.append(f'    letterSpacing: "{r["tracking"]}em"')
    md, lg = s["radius"]["md"], s["radius"]["lg"]
    out += ["components:",
            "  button-primary:", '    backgroundColor: "{colors.primary}"', '    textColor: "{colors.on-primary}"',
            '    height: "28px"', '    padding: "0 0.7rem"', f'    rounded: "{md}px"',
            "  button-primary-hover:", '    backgroundColor: "{colors.primary-hover}"', '    textColor: "{colors.on-primary}"',
            "  button-secondary:", '    backgroundColor: "transparent"', '    textColor: "{colors.muted}"',
            '    height: "28px"', '    padding: "0 0.7rem"', f'    rounded: "{md}px"',
            "  button-secondary-hover:", '    backgroundColor: "{colors.hover}"', '    textColor: "{colors.fg}"',
            "  field:", '    backgroundColor: "{colors.field}"', '    textColor: "{colors.fg}"', f'    rounded: "{md}px"',
            "  chip:", '    textColor: "{colors.muted}"', '    rounded: "999px"',
            "  dialog:", '    backgroundColor: "{colors.dialog}"', '    textColor: "{colors.fg}"', f'    rounded: "{lg}px"',
            "  card-panel:", '    backgroundColor: "{colors.panel}"', '    textColor: "{colors.fg}"', f'    rounded: "{lg}px"',
            "---", ""]
    path = "../../DESIGN.md"
    body = open(path).read().split("\n---\n", 1)[1].lstrip("\n")
    return path, "\n".join(out) + "\n" + body

emit(*design_md())

if stale:
    sys.exit("Stale generated tokens: " + ", ".join(stale) + ". Run python3 build.py from design/tokens.")
