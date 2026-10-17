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
# Seigaiha, the traditional wave pattern, as quiet texture: grey rings a
# step above night, so the mark and its strike stay the one bright moment.
WAVE = "#17181C"


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
    "waves": dict(d=path("mark.svg", "cv"), strike=(108, 15), cut=14, by="height", waves=True),
    "wordmark": dict(d=path("logo-night.svg", "lettering"), strike=(35.48, 8), cut=7, by="width"),
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
    # The waves wallpaper keeps the mark's placement; the letters' cut clears the waves around them.
    ("waves", "desktop-5k"): (0.66, 0.56, 0.36),
    ("waves", "macbook"): (0.66, 0.56, 0.34),
    ("waves", "phone"): (0.50, 0.64, 0.17),
    # The wordmark is centred, as on the homepage, with room around it.
    ("wordmark", "desktop-5k"): (0.50, 0.50, 0.46),
    ("wordmark", "macbook"): (0.50, 0.50, 0.50),
    ("wordmark", "phone"): (0.50, 0.58, 0.80),
}


def waves(width, height):
    """Rows of overlapping fans, each lower row drawn over the one above."""
    radius = max(width, height) / 26
    rings = "".join(f'<circle r="{radius * f:.1f}" fill="none" stroke="{WAVE}" stroke-width="{radius * 0.07:.1f}"/>'
                    for f in (0.93, 0.7, 0.47))
    motif = f'<g id="wave"><circle r="{radius:.1f}" fill="{GROUND}"/>{rings}</g>'
    uses = []
    for row in range(int(height / (radius / 2)) + 3):
        y = row * radius / 2
        offset = radius if row % 2 else 0
        for column in range(-1, int(width / (2 * radius)) + 2):
            uses.append(f'<use href="#wave" x="{column * 2 * radius + offset:.1f}" y="{y:.1f}"/>')
    return motif, "".join(uses)


def svg(design, width, height, cx, cy, size):
    left, right, top, bottom = outline_box(design["d"])
    strike_top, strike_height = design["strike"]
    scale = size * (height / (bottom - top) if design["by"] == "height" else width / (right - left))
    x = cx * width - (left + right) / 2 * scale
    y = cy * height - (strike_top + strike_height / 2) * scale
    place = f"translate({x:.2f} {y:.2f}) scale({scale:.4f})"
    motif, texture = waves(width, height) if design.get("waves") else ("", "")
    if texture:
        # Clear the waves around the letters by drawing the cut in the ground
        # colour: a mask over thousands of shapes renders unreliably.
        texture += (f'<use href="#letters" transform="{place}" fill="{GROUND}" stroke="{GROUND}" '
                    f'stroke-width="{design["cut"]}" stroke-linejoin="round"/>')
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <title>Code Villain</title>
  <defs>
    <path id="letters" d="{design["d"]}"/>{motif}
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
