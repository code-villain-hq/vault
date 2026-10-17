"""Generate the Code Villain wallpapers from the marks, then render them.

Run from this folder: python3 build.py
Writes one SVG per design and format, and renders a PNG of each with
headless Chrome. The letters and strikes come from brand/mark; nothing is
redrawn.
"""
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).parent
MARKS = (HERE / "../../mark").resolve()
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
GROUND, INK, POISON = "#0A0B0D", "#ECECF1", "#C9E265"
# The rows pattern repeats the wordmark in a grey a step above night, so
# the lit wordmark and its strike stay the one bright moment.
TEXTURE = "#1A1B20"


def path(file, name):
    return re.search(rf'<path id="{name}" d="([^"]+)"', (MARKS / file).read_text()).group(1)


def outline_box(d):
    numbers = [float(n) for n in re.findall(r"-?\d+\.?\d*", d)]
    xs, ys = numbers[0::2], numbers[1::2]
    return min(xs), max(xs), min(ys), max(ys)


# Each design in its source's units: the letters, the strike's top and
# height, and the cut around the letters, and whether its size is a share of
# the screen's height or width. Outlines are measured from the path.
DESIGNS = {
    "mark": dict(d=path("mark.svg", "cv"), strike=(108, 15), cut=14, by="height"),
    "wordmark": dict(d=path("logo-night.svg", "lettering"), strike=(35.48, 8), cut=7, by="width"),
    "rows": dict(d=path("logo-night.svg", "lettering"), strike=(35.48, 8), cut=7, by="width", rows=True),
}

# Per design and format: (width, height, centre x, strike centre y, size).
# Size is the mark's height, or the wordmark's width, as a fraction of the screen.
FORMATS = {
    "desktop-5k": (5120, 2880),
    "macbook": (3456, 2234),
    "phone": (1320, 2868),
}
PLACEMENT = {
    # The mark sits right of centre, clear of windows' usual middle; low on a phone, under the clock.
    ("mark", "desktop-5k"): (0.66, 0.56, 0.36),
    ("mark", "macbook"): (0.66, 0.56, 0.34),
    ("mark", "phone"): (0.50, 0.64, 0.17),
    # The wordmark is centred, as on the homepage, with room around it.
    ("wordmark", "desktop-5k"): (0.50, 0.50, 0.46),
    ("wordmark", "macbook"): (0.50, 0.50, 0.50),
    ("wordmark", "phone"): (0.50, 0.58, 0.80),
    # Rows of grey wordmarks, with the lit one in a row of its own.
    ("rows", "desktop-5k"): (0.50, 0.50, 0.46),
    ("rows", "macbook"): (0.50, 0.50, 0.50),
    ("rows", "phone"): (0.50, 0.58, 0.80),
}


def rows(design, width, height, clear_top, clear_bottom):
    """Staggered rows of the wordmark, leaving the lit row's band clear."""
    left, right, top, bottom = outline_box(design["d"])
    scale = max(width, height) * 0.15 / (right - left)
    word, tall = (right - left) * scale, (bottom - top) * scale
    step, gap = tall * 1.3, word * 0.08
    uses = []
    for row in range(-1, int(height / step) + 2):
        y = row * step
        if y + tall > clear_top - step * 0.4 and y < clear_bottom + step * 0.4:
            continue
        offset = (word + gap) / 2 if row % 2 else 0
        for column in range(-1, int(width / (word + gap)) + 2):
            x = column * (word + gap) + offset
            uses.append(f'<use href="#letters" transform="translate({x - left * scale:.1f} {y - top * scale:.1f}) '
                        f'scale({scale:.4f})" fill="{TEXTURE}"/>')
    return "".join(uses)


def svg(design, width, height, cx, cy, size):
    left, right, top, bottom = outline_box(design["d"])
    strike_top, strike_height = design["strike"]
    scale = size * (height / (bottom - top) if design["by"] == "height" else width / (right - left))
    x = cx * width - (left + right) / 2 * scale
    y = cy * height - (strike_top + strike_height / 2) * scale
    place = f"translate({x:.2f} {y:.2f}) scale({scale:.4f})"
    texture = rows(design, width, height, y + top * scale, y + bottom * scale) if design.get("rows") else ""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <title>Code Villain</title>
  <defs>
    <path id="letters" d="{design["d"]}"/>
    <mask id="cut" maskUnits="userSpaceOnUse" x="0" y="0" width="{width}" height="{height}">
      <rect width="{width}" height="{height}" fill="#fff"/>
      <use href="#letters" transform="{place}" fill="#000" stroke="#000" stroke-width="{design["cut"]}" stroke-linejoin="round"/>
    </mask>
  </defs>
  <rect width="{width}" height="{height}" fill="{GROUND}"/>{texture}
  <rect y="{y + strike_top * scale:.2f}" width="{width}" height="{strike_height * scale:.2f}" fill="{POISON}" mask="url(#cut)"/>
  <use href="#letters" transform="{place}" fill="{INK}"/>
</svg>
"""


for (name, size_name), (cx, cy, size) in PLACEMENT.items():
    width, height = FORMATS[size_name]
    stem = f"code-villain-{name}-{size_name}"
    source = HERE / f"{stem}.svg"
    source.write_text(svg(DESIGNS[name], width, height, cx, cy, size))
    with tempfile.TemporaryDirectory() as profile:
        subprocess.run(
            [CHROME, "--headless=new", f"--user-data-dir={profile}", "--hide-scrollbars",
             "--force-device-scale-factor=1", f"--window-size={width},{height}",
             "--virtual-time-budget=10000",
             f"--screenshot={HERE / f'{stem}.png'}", source.as_uri()],
            check=True, capture_output=True,
        )
    print(f"{stem}: {width}×{height}")
