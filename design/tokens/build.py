"""Regenerate the CSS and Swift for color, type, and shape from their JSON.

Run from this folder: python3 build.py
A stopgap until @villain/tokens builds these with Style Dictionary.
"""
import json
import re

camel = lambda k: re.sub(r"-(\w)", lambda m: m.group(1).upper(), k)

c = json.load(open("color.json"))
s = json.load(open("shape.json"))
t = json.load(open("type.json"))

# Resolve role aliases before calculating ink against translucent status fills.
def resolve(theme, key, path=()):
    if key in path:
        raise ValueError(f"Circular colour alias: {path + (key,)}")
    value = theme[key]
    if value.startswith("{") and value.endswith("}"):
        return resolve(theme, value[1:-1], path + (key,))
    return value

def channels(hex):
    return [int(hex[i:i + 2], 16) for i in (1, 3, 5)]

def mix(ground, source, opacity):
    return "#" + "".join(
        f"{int(a * (1 - opacity) + b * opacity + 0.5):02X}"
        for a, b in zip(channels(ground), channels(source))
    )

def luminance(hex):
    srgb = [x / 255 for x in channels(hex)]
    linear = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in srgb]
    return sum(x * weight for x, weight in zip(linear, (0.2126, 0.7152, 0.0722)))

def contrast(a, b):
    low, high = sorted((luminance(a), luminance(b)))
    return (high + 0.05) / (low + 0.05)

for mode in ("night", "day"):
    theme = c[mode] = {key: resolve(c[mode], key) for key in c[mode]}
    hosts = [theme[key] for key in (
        "bg", "panel", "surface", "hover", "selected", "dialog", "field",
        "field-hover", "dialog-field", "dialog-field-hover",
    )]
    for status in ("ok", "warn", "danger", "info", "neutral"):
        hue = theme["muted" if status == "neutral" else status]
        composites = [mix(host, hue, c["opacity"]["status-tint"]) for host in hosts]
        for step in range(101):
            ink = mix(hue, "#FFFFFF" if mode == "night" else "#000000", step / 100)
            if all(contrast(ink, bg) >= c["contrast"]["status-text"] for bg in composites):
                theme[f"status-{status}-ink"] = ink
                break
        else:
            raise ValueError(f"No readable ink for {mode} {status}")

# Apps that need numerical colour values consume the same resolved aliases as CSS/Swift.
open("color.resolved.json", "w").write(json.dumps(c, indent=2) + "\n")

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
open("color.css", "w").write("\n".join(css))

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
open("type.css", "w").write("\n".join(css))

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
open("type.swift", "w").write("\n".join(sw) + "\n")
