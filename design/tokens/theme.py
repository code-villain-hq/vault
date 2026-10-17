"""Nightshade's shared theme calculations. No app-specific colour overrides.

OKLab conversion follows CSS Color 4's reference conversion matrices.
https://www.w3.org/TR/css-color-4/#color-conversion-code
"""
import math
import re

HOSTS = (
    "bg", "panel", "surface", "hover", "selected", "dialog", "field",
    "field-hover", "dialog-field", "dialog-field-hover",
)


def channels(hex):
    if not isinstance(hex, str) or not re.fullmatch(r"#[0-9a-fA-F]{6}", hex):
        raise ValueError(f"Expected a six-digit sRGB colour, got {hex!r}")
    return [int(hex[i:i + 2], 16) for i in (1, 3, 5)]


def linear(value):
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def encoded(value):
    return 12.92 * value if value <= 0.0031308 else 1.055 * value ** (1 / 2.4) - 0.055


def hex_color(values):
    return "#" + "".join(f"{int(max(0, min(1, x)) * 255 + 0.5):02X}" for x in values)


def oklch(hex):
    r, g, b = [linear(c / 255) for c in channels(hex)]
    l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s
    a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s
    b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s
    return lightness, math.hypot(a, b), math.atan2(b, a)


def rgb(lightness, chroma, hue):
    a, b = chroma * math.cos(hue), chroma * math.sin(hue)
    l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s = (lightness - 0.0894841775 * a - 1.2914855480 * b) ** 3
    return [encoded(c) for c in (
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    )]


def tone(lightness, chroma, hue):
    """Fit sRGB by reducing chroma, retaining hue and perceptual lightness."""
    values = rgb(lightness, chroma, hue)
    if all(-1e-7 <= c <= 1 + 1e-7 for c in values):
        return hex_color(values)
    low, high = 0, chroma
    for _ in range(24):
        middle = (low + high) / 2
        values = rgb(lightness, middle, hue)
        if all(0 <= c <= 1 for c in values):
            low = middle
        else:
            high = middle
    return hex_color(rgb(lightness, low, hue))


def mix(ground, source, opacity):
    return hex_color([(a * (1 - opacity) + b * opacity) / 255
                      for a, b in zip(channels(ground), channels(source))])


def luminance(hex):
    return sum(linear(c / 255) * weight
               for c, weight in zip(channels(hex), (0.2126, 0.7152, 0.0722)))


def contrast(a, b):
    low, high = sorted((luminance(a), luminance(b)))
    return (high + 0.05) / (low + 0.05)


def readable(hue, hosts, mode, minimum):
    """Nearest readable lightness; keep the hue, gamut-map chroma if needed."""
    lightness, chroma, angle = oklch(hue)
    end = 1 if mode == "night" else 0
    for step in range(1001):
        ink = tone(lightness + (end - lightness) * step / 1000, chroma, angle)
        if all(contrast(ink, ground) >= minimum for ground in hosts):
            return ink
    raise ValueError(f"No readable {mode} ink for {hue}")


def generate(source):
    """Resolve palette + base/accent/contrast into stable night/day role names."""
    settings = source["theme"]
    strength = settings["contrast"]
    if isinstance(strength, bool) or not isinstance(strength, (int, float)) or not math.isfinite(strength) or not 0 <= strength <= 100:
        raise ValueError("Theme contrast must be a number from 0 to 100")
    def seed(name):
        value = settings[name]
        if value.startswith("{") and value.endswith("}"):
            value = source["palette"][value[1:-1]]
        channels(value)
        return value
    base, accent = seed("base"), seed("accent")
    base_lightness, base_chroma, base_hue = oklch(base)
    # Above this lightness a base can't be mixed into dark surfaces as a fill;
    # it becomes the fill itself (Nightshade's poison is 0.87).
    vivid = base_lightness >= 0.7
    amount = (strength - 50) / 50
    minimum = source["contrast"]["text"] + max(0, amount) * (7 - source["contrast"]["text"])
    result = {key: value for key, value in source.items() if key != "semantics"}
    result["contrast"] = {**source["contrast"], "text": minimum}
    for mode in ("night", "day"):
        night = mode == "night"
        # Authored hierarchy, shared by every theme. Contrast widens the steps.
        levels = ({"bg": .15, "panel": .19, "surface": .22, "hover": .23, "line": .29}
                  if night else {"bg": .97, "panel": 1, "surface": .955, "hover": .94, "line": .91})
        theme = {}
        for role, level in levels.items():
            # Keep the frame anchored, and the day content white.
            shift = 0 if role in ("bg", "panel") else amount * (.012 if night else -.012)
            theme[role] = tone(level + shift, 0, 0)
        if vivid:
            # A light base is a fill in its own right: the action wears it as
            # is, with dark ink, and selection takes a faint tint of it.
            theme["primary"] = base
            theme["primary-hover"] = tone(base_lightness - .06, base_chroma, base_hue)
            theme["selected"] = mix(theme["bg"] if night else theme["panel"], base, .09 if night else .16)
        else:
            theme["primary"] = mix(theme["surface"] if night else theme["panel"], base, .40 if night else .14)
            theme["primary-hover"] = mix(theme["surface"] if night else theme["panel"], base, .50 if night else .22)
            theme["selected"] = mix(theme["bg"] if night else theme["panel"], base, .20 if night else .07)
        theme.update({
            "dialog": theme["hover"] if night else theme["panel"],
            "field": theme["surface"], "field-hover": theme["hover"],
            "dialog-field": theme["panel"] if night else theme["surface"],
            "dialog-field-hover": theme["surface"] if night else theme["hover"],
        })
        hosts = [theme[key] for key in HOSTS]
        for role, level, change in (("fg", .94 if night else .21, .045),
                                    ("muted", .69 if night else .45, .10),
                                    ("faint", .63 if night else .52, .12)):
            ink = tone(level + amount * change * (1 if night else -1), 0, 0)
            text_hosts = hosts
            if role in ("muted", "faint"):
                text_hosts = hosts + [mix(host, theme["fg"], source["opacity"][tint])
                                      for tint in ("chip-tint", "highlight-tint") for host in hosts]
            theme[role] = readable(ink, text_hosts, mode, minimum)
        # At the contrast extremes the floor can push muted and faint to one
        # tone. Faint keeps the floor; muted steps further from the hosts until
        # it stands a visible step above faint (Nightshade's default gap).
        direction = 1 if night else -1
        faint_lightness = oklch(theme["faint"])[0]
        step = 0
        while (oklch(theme["muted"])[0] - faint_lightness) * direction < .015:
            step += 1
            theme["muted"] = readable(tone(faint_lightness + direction * step / 1000, 0, 0),
                                      text_hosts, mode, minimum)
        if vivid:
            # Dark ink on the light fill, in both modes. A fill too dark for
            # black ink at this contrast rises in lightness until it carries it.
            for role, start in (("primary", base_lightness), ("primary-hover", base_lightness - .06)):
                level = start
                while contrast("#000000", theme[role]) < minimum and level < 1:
                    level += .005
                    theme[role] = tone(level, base_chroma, base_hue)
            dark = theme["bg"] if night else theme["fg"]
            theme["on-primary"] = readable(dark, [theme["primary"], theme["primary-hover"]], "day", minimum)
        else:
            # Extreme seed colours must not make the primary action unreadable.
            ground = theme["surface"] if night else theme["panel"]
            for role in ("primary", "primary-hover"):
                original = theme[role]
                for step in range(101):
                    theme[role] = mix(original, ground, step / 100)
                    if contrast(theme["fg"], theme[role]) >= minimum:
                        break
            theme["on-primary"] = readable(theme["fg"], [theme["primary"], theme["primary-hover"]], mode, minimum)
        # The accent marks focus, never a large fill. Both modes derive from one hue.
        theme["accent"] = readable(accent, hosts, mode, minimum)
        # Live dots wear the brand colour. They carry no text, so WCAG's 3:1
        # non-text floor keeps them nearer the seed than text would be.
        theme["signal"] = readable(base, hosts, mode, source["contrast"]["signal"])
        for name, color in source["semantics"].items():
            theme[name] = readable(color, hosts, mode, minimum)
        for status in (*source["semantics"], "neutral"):
            color = theme["muted" if status == "neutral" else status]
            composites = [mix(host, color, source["opacity"]["status-tint"]) for host in hosts]
            theme[f"status-{status}-ink"] = readable(color, composites, mode, minimum)
        result[mode] = theme
    return result
