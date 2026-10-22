"""Write the stylesheet Vaultwarden adds to its web vault at password.codevillain.eu,
from the Nightshade tokens and the mark.

Run from this folder: python3 build.py (--check writes nothing and fails if
kit/ is stale). README.md says where the file goes.

The web vault colours everything from CSS variables: day in :root, night in
.theme_dark. This file points those names at our tokens. Every name is set in
both blocks, because ours come after Bitwarden's and an unpaired :root value
would win at night too.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).parent
BRAND = HERE / "../../brand"
TOKENS = json.loads((HERE / "../tokens/color.resolved.json").read_text())
MARK = (BRAND / "mark/mark.svg").read_text()
OUT = HERE / "kit/user.vaultwarden.scss.hbs"


def rgb(hex_):
    """Bitwarden's older names hold bare channels, "R G B"."""
    h = hex_.lstrip("#")
    return " ".join(str(int(h[i:i + 2], 16)) for i in (0, 2, 4))


def tint(t, name, pct, over="panel"):
    return f"color-mix(in srgb, {t[name]} {pct}%, {t[over]})"


def names(t):
    o = TOKENS["opacity"]
    status = round(o["status-tint"] * 100)
    border_strong = tint(t, "fg", 22, "panel")
    v = {
        # Text. Links and brand-coloured text are ink, never poison.
        "fg-heading": t["fg"], "fg-body": t["fg"], "fg-dark": t["fg"],
        "fg-body-subtle": t["muted"], "fg-inactive": t["faint"],
        "fg-brand": t["fg"], "fg-brand-strong": t["fg"], "fg-brand-soft": t["muted"],
        "fg-contrast": t["on-primary"], "fg-contrast-strong": t["on-primary"],
        "fg-success": t["status-ok-ink"], "fg-success-strong": t["status-ok-ink"],
        "fg-danger": t["status-danger-ink"], "fg-danger-strong": t["status-danger-ink"],
        "fg-warning": t["status-warn-ink"], "fg-warning-strong": t["status-warn-ink"],
        # Grounds: the frame is bg, content sits on panel.
        "bg-primary": t["panel"], "bg-secondary": t["bg"], "bg-tertiary": t["bg"],
        "bg-quaternary": t["surface"], "bg-inactive": t["surface"], "bg-dark": t["surface"],
        "bg-hover": t["hover"],
        "bg-overlay": f"rgba(0, 0, 0, {o['backdrop-tint']})",
        # Contrast fills (tooltips, toasts) take fg, so on-primary ink reads on them.
        "bg-contrast": t["fg"], "bg-contrast-soft": t["muted"], "bg-contrast-strong": t["fg"],
        # Poison: the main action's fill, and a faint tint for what is selected.
        "bg-brand": t["primary"], "bg-brand-strong": t["primary-hover"],
        "bg-brand-stronger": t["primary-hover"],
        "bg-brand-softer": t["selected"], "bg-brand-soft": t["selected"],
        "bg-brand-medium": t["selected"],
        "bg-success": t["ok"], "bg-success-strong": t["ok"],
        "bg-success-soft": tint(t, "ok", status), "bg-success-medium": tint(t, "ok", status),
        "bg-danger": t["danger"], "bg-danger-strong": t["danger"],
        "bg-danger-soft": tint(t, "danger", status), "bg-danger-medium": tint(t, "danger", status),
        "bg-warning": t["warn"], "bg-warning-strong": t["warn"],
        "bg-warning-soft": tint(t, "warn", status), "bg-warning-medium": tint(t, "warn", status),
        # Lines, and the focus ring in cloak.
        "border-muted": t["line"], "border-light": t["line"], "border-base": t["line"],
        "border-strong": border_strong, "border-stronger": t["muted"],
        "border-buffer": t["panel"],
        # Bitwarden rings a focused field with its brand border: that is focus, so cloak.
        "border-brand": t["accent"], "border-brand-strong": t["accent"], "border-brand-soft": t["line"],
        "border-focus": t["accent"], "border-focus-contrast": t["accent"],
        "border-success": t["ok"], "border-danger": t["danger"], "border-warning": t["warn"],
        # The sidebar is the frame, in both modes.
        "nav-bg-primary": t["bg"], "nav-bg-secondary": t["bg"],
        "nav-fg-primary": t["fg"], "nav-fg-secondary": t["fg"],
        "nav-bg-primary-strong": t["selected"], "nav-bg-secondary-strong": t["selected"],
        "nav-bg-primary-hover": t["hover"], "nav-bg-secondary-hover": t["hover"],
        "nav-bg-primary-hover-strong": t["hover"], "nav-bg-secondary-hover-strong": t["hover"],
        "nav-border-primary": t["line"], "nav-border-secondary": t["line"],
        "nav-border-primary-focus": t["accent"], "nav-border-secondary-focus": t["accent"],
    }
    v = {f"--color-{k}": val for k, val in v.items()}
    # The older names, as bare channels.
    v.update({f"--color-{k}": rgb(val) for k, val in {
        "background": t["panel"], "background-alt": t["bg"],
        "background-alt2": t["bg"], "background-alt3": t["bg"], "background-alt4": t["bg"],
        "primary-100": t["selected"], "primary-300": t["line"],
        "primary-600": t["primary"], "primary-700": t["primary-hover"],
        "secondary-100": t["surface"], "secondary-300": t["line"],
        "secondary-500": t["muted"], "secondary-600": t["muted"], "secondary-700": t["fg"],
        "text-main": t["fg"], "text-muted": t["muted"],
        "text-contrast": t["on-primary"], "text-alt2": t["fg"],
        "success-600": t["ok"], "success-700": t["status-ok-ink"],
        "danger-600": t["danger"], "danger-700": t["status-danger-ink"],
        "warning-600": t["warn"], "warning-700": t["status-warn-ink"],
        "info-600": t["info"], "info-700": t["status-info-ink"],
        "shadow": t["line"],
    }.items()})
    return v


def block(selector, v):
    body = "\n".join(f"  {k}: {val};" for k, val in v.items())
    return f"{selector} {{\n{body}\n}}\n"


def svg_url(svg):
    return "data:image/svg+xml," + " ".join(svg.split()).replace("#", "%23").replace('"', "'")


mark = svg_url(MARK)
logo_day = svg_url((BRAND / "mark/logo-day.svg").read_text())
logo_night = svg_url((BRAND / "mark/logo-night.svg").read_text())
LOGOS = "header > a[href='#/'] bit-svg, bit-nav-logo bit-svg"
ACTIVE_FILTER = """.filter-heading .filter-button h3.active,
.filter-heading .filter-button button.active,
.filter-heading .filter-button h3:focus,
.filter-heading .filter-button button:hover,
.filter-heading .filter-button button:focus,
.filter-option.active > .filter-buttons .filter-button,
.filter li.active > button:first-of-type,
.filter li.active > div button:first-of-type,
.filter li.active > .bwi,
.filter li.active > div > .bwi"""
css = f"""/* Generated by studio/design/vaultwarden/build.py from the Nightshade tokens. */
{block(":root", names(TOKENS["day"]))}
{block(".theme_dark", names(TOKENS["night"]))}
/* The main action is its poison fill alone: no ink outline. */
.tw-bg-bg-brand.tw-border-border-brand {{ border-color: transparent; }}

/* The current filter is ink and bold, not the primary colour. */
{ACTIVE_FILTER} {{ color: var(--color-fg-heading); }}
.filter-heading .filter-button h3.active,
.filter-heading .filter-button button.active,
.filter-option.active > .filter-buttons .filter-button {{ font-weight: 700; }}

/* Older parts colour links with the primary channel; links are ink. Text
   links are underlined; names in lists underline on hover, as they do now. */
.tw-text-primary-600 {{ color: var(--color-fg-body) !important; }}
a[bitlink] {{ text-decoration: underline; font-weight: 700; }}

/* The Code Villain wordmark where Vaultwarden puts its own. */
{LOGOS} {{
  background: url("{logo_day}") left center / contain no-repeat;
}}
.theme_dark :is({LOGOS}) {{ background-image: url("{logo_night}"); }}
:is({LOGOS}) > svg {{ visibility: hidden; }}
bit-nav-logo bit-svg {{ aspect-ratio: 348 / 72; width: 200px; }}

/* The mark when the sidebar is folded, where Vaultwarden puts its shield. */
bit-nav-logo bit-nav-item a:before {{
  background-image: url("{mark}");
  background-size: 32px 32px;
}}
"""
if "--check" in sys.argv:
    if not OUT.exists() or OUT.read_text() != css:
        sys.exit(f"{OUT.relative_to(HERE)} is stale: run python3 build.py")
else:
    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text(css)
