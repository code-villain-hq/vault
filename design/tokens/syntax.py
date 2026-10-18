"""Nightshade for code: syntax and editor colours, from the resolved theme.

Code sits on `panel`. Six hues carry what a reader scans for: keywords,
properties (with operators and built-in types), strings, constants,
functions, and the language's own names like `this`. All share one OKLCH
lightness, so no token shouts. Names and type names stay in `fg`; grey
steps back for brackets, then punctuation and comments. Poison and cloak
keep their meanings: cloak is the cursor and focus, poison the search match
and live signals. Neither colours a token.
"""
from theme import contrast, mix, oklch, readable, tone

SEMANTIC = ("danger", "ok", "warn", "info")


def resolve(source, theme_source, color):
    """`color` is the generated colour result; `theme_source` its color.json."""
    def seed(value):
        if value.startswith("{") and value.endswith("}"):
            name = value[1:-1]
            value = theme_source["semantics"].get(name) or theme_source["palette"][name]
        oklch(value)
        return value

    hues = {role: seed(value) for role, value in source["hues"].items()}
    minimum = color["contrast"]["text"]
    result = {
        "name": source["name"],
        "contrast": minimum,
        "opacity": source["opacity"],
        "styles": {role: spec["style"] for role, spec in source["roles"].items() if "style" in spec},
        "scopes": {role: spec["scopes"] for role, spec in source["roles"].items()},
    }
    for mode in ("night", "day"):
        night = mode == "night"
        theme, levels = color[mode], source["levels"][mode]
        ground = theme["panel"]
        # An opacity is one number, or one per mode where the colour under it
        # differs in weight: poison is light, so its tint shows more by night.
        opacity = {key: value[mode] if isinstance(value, dict) else value
                   for key, value in source["opacity"].items()}
        ui = {
            "background": ground,
            "frame": theme["bg"],
            "float": theme["dialog"],
            "cursorline": mix(ground, theme["fg"], opacity["cursorline"]),
            "selection": mix(ground, theme["fg"], opacity["selection"]),
            # The arrow keys' row in a list or menu, as components.md has it.
            "highlight": mix(theme["dialog"], theme["fg"], color["opacity"]["highlight-tint"]),
            # The one poison moment in an editor: the search match.
            "search": mix(ground, theme["primary"], opacity["search"]),
            "search-current": theme["primary"],
            "on-search-current": theme["on-primary"],
            "selected": theme["selected"],
            "guide": theme["line"],
            "signal": theme["signal"],
        }
        for status, kind in (("ok", "added"), ("danger", "removed"), ("info", "changed")):
            ui[f"diff-{kind}"] = mix(ground, theme[status], opacity["diff"])
            ui[f"diff-{kind}-text"] = mix(ground, theme[status], opacity["diff-text"])
        # Every token must read on every background code is drawn on.
        hosts = [ui[key] for key in ("background", "float", "cursorline", "selection", "search",
                                     "diff-added", "diff-removed", "diff-changed",
                                     "diff-added-text", "diff-removed-text", "diff-changed-text")]

        inks = {"fg": readable(theme["fg"], hosts, mode, minimum)}
        inks["quiet"] = readable(tone(levels["quiet"], 0, 0), hosts, mode, minimum)
        # A higher contrast floor lifts the quiet grey towards the structure
        # grey. Structure keeps a visible step from it, and from the names.
        direction = 1 if night else -1
        quiet = oklch(inks["quiet"])[0]
        structure = levels["structure"]
        if (structure - quiet) * direction < .04:
            structure = quiet + direction * .04
        inks["structure"] = readable(tone(structure, 0, 0), hosts, mode, minimum)
        if (oklch(inks["fg"])[0] - oklch(inks["structure"])[0]) * direction < .04:
            inks["fg"] = tone(oklch(inks["structure"])[0] + direction * .04, 0, 0)
        # Even colour: the hues share one lightness, the first where all of them
        # read on every host. Each keeps its hue; chroma is fitted to sRGB.
        angles = {role: oklch(value)[2] for role, value in hues.items()}
        lightness = levels["hue"]
        end = 1 if night else 0
        for _ in range(1000):
            candidate = {role: tone(lightness, levels["chroma"], angle) for role, angle in angles.items()}
            if all(contrast(ink, host) >= minimum for ink in candidate.values() for host in hosts):
                break
            lightness += (end - lightness) / 200
        else:
            raise ValueError(f"No shared readable lightness for the {mode} syntax hues")
        inks.update(candidate)
        for name in SEMANTIC:
            inks[name] = readable(theme[name], hosts, mode, minimum)

        syntax = {role: inks[spec["tone"]] for role, spec in source["roles"].items()}
        ui.update({
            "line-number": inks["quiet"],
            "line-number-current": inks["fg"],
            "cursor": theme["accent"],
            "match": readable(theme["accent"], hosts, mode, minimum),
            "muted": readable(theme["muted"], hosts, mode, minimum),
            **{name: inks[name] for name in SEMANTIC},
        })
        # The terminal's 16 colours, one table for every terminal: the syntax
        # hues, so the shell and the editor speak one set. Black and white are
        # greys that read in each mode; bright black is the comment grey.
        terminal = [inks["danger"], syntax["string"], syntax["constant"], syntax["function"],
                    syntax["keyword"], syntax["property"]]
        ansi = {
            "normal": [theme["surface"] if night else theme["fg"], *terminal, syntax["bracket"]],
            "bright": [ui["line-number"], *terminal, syntax["text"]],
        }
        result[mode] = {"syntax": syntax, "ui": ui, "ansi": ansi}
    return result
