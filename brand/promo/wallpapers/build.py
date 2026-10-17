"""Generate the Code Villain wallpapers from the marks, then render them.

Run from this folder: python3 build.py
Writes one SVG per design and format, and renders a PNG of each with
headless Chrome. The letters and strikes come from brand/mark; nothing is
redrawn.
"""
import math
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
# The cube pattern: tumbling blocks, no letters. Each palette lights the
# left face, fans it into shade, and engraves a grey top over night.
CUBES = {
    # Poison, the brand colour, fading into a dark olive.
    "cubes-poison": dict(lit="#C9E265", shade="#252919", dark=GROUND, top_line="#2E2F3A"),
    # Cloak, the supporting colour, fading into wash.
    "cubes-cloak": dict(lit="#5A1470", shade="#1A0D24", dark=GROUND, top_line="#2E2F3A"),
}


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
    # Rows of grey wordmarks running up to the lit one and its strike.
    ("rows", "desktop-5k"): (0.50, 0.50, 0.46),
    ("rows", "macbook"): (0.50, 0.50, 0.50),
    ("rows", "phone"): (0.50, 0.58, 0.80),
}


def rows(design, width, height):
    """Staggered rows of the wordmark across the whole screen."""
    left, right, top, bottom = outline_box(design["d"])
    scale = max(width, height) * 0.15 / (right - left)
    word, tall = (right - left) * scale, (bottom - top) * scale
    step, gap = tall * 1.3, word * 0.08
    uses = []
    for row in range(-1, int(height / step) + 2):
        y = row * step
        offset = (word + gap) / 2 if row % 2 else 0
        for column in range(-1, int(width / (word + gap)) + 2):
            x = column * (word + gap) + offset
            uses.append(f'<use href="#letters" transform="translate({x - left * scale:.1f} {y - top * scale:.1f}) '
                        f'scale({scale:.4f})" fill="{TEXTURE}"/>')
    return "".join(uses)


def cubes(width, height, c):
    """Tumbling blocks. Hatched halves are engraved: stripes run parallel to
    the face's edge and thin out towards the opposite corner."""
    edge = min(width, height) * 0.16

    def solid(fill, points):
        # A 1px outline in the same fill closes anti-aliasing seams between neighbours.
        d = "M" + "L".join(f"{x:.1f} {y:.1f}" for x, y in points)
        return f'<path d="{d}Z" fill="{fill}" stroke="{fill}" stroke-width="1"/>'

    lerp = lambda p, q, t: (p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t)
    shape = lambda fill, pts: f'<path d="M{"L".join(f"{x:.1f} {y:.1f}" for x, y in pts)}Z" fill="{fill}"/>'

    def engraved(ground, line, near_a, near_b, far_b, far_a, stripes=14):
        # A parallelogram engraved with stripes parallel to its near edge.
        # Each stripe's share of its band falls from 0.85 at the near edge to
        # 0.05 at the far one, so the face fades from line to ground.
        out = [solid(ground, (near_a, near_b, far_b, far_a))]
        for i in range(stripes):
            start = i / stripes
            end = start + (0.85 - 0.8 * i / (stripes - 1)) / stripes
            out.append(shape(line, (lerp(near_a, far_a, start), lerp(near_b, far_b, start),
                                    lerp(near_b, far_b, end), lerp(near_a, far_a, end))))
        return "".join(out)

    def fan(ground, line, apex, lit_side, dark_side, rays=16):
        # Rays from one corner, thick beside the lit face and thinning out, so
        # the shading turns like a cone into the dark.
        out = [solid(ground, (apex, lit_side, dark_side))]
        for i in range(rays):
            start = i / rays
            end = start + (0.85 - 0.8 * i / (rays - 1)) / rays
            out.append(shape(line, (apex, lerp(lit_side, dark_side, start), lerp(lit_side, dark_side, end))))
        return "".join(out)

    def cube(cx, cy):
        p = {k: (cx + edge * math.cos(math.radians(a)), cy + edge * math.sin(math.radians(a)))
             for k, a in (("T", -90), ("UR", -30), ("LR", 30), ("B", 90), ("LL", 150), ("UL", 210))}
        p["C"] = (cx, cy)
        # The top is one engraved face, its stripes parallel to the back-right
        # edge. The left face is lit, then fans into shade; the right stays dark.
        return (engraved(c["dark"], c["top_line"], p["T"], p["UR"], p["C"], p["UL"])
                + solid(c["lit"], (p["UL"], p["LL"], p["B"]))
                + fan(c["shade"], c["lit"], p["B"], p["UL"], p["C"])
                + solid(c["shade"], (p["UR"], p["C"], p["B"]))
                + solid(c["dark"], (p["UR"], p["LR"], p["B"])))

    across, down = edge * math.sqrt(3), edge * 1.5
    # Centre the grid so the blocks sit evenly on every screen.
    columns, rows = int(width / across) + 3, int(height / down) + 3
    x0 = (width - (columns - 1) * across) / 2
    y0 = (height - (rows - 1) * down) / 2
    tiles = "".join(cube(x0 + column * across + (across / 2 if row % 2 else 0), y0 + row * down)
                    for row in range(rows) for column in range(-1, columns))
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <title>Code Villain</title>
  <rect width="{width}" height="{height}" fill="{GROUND}"/>{tiles}
</svg>
"""


def svg(design, width, height, cx, cy, size):
    left, right, top, bottom = outline_box(design["d"])
    strike_top, strike_height = design["strike"]
    scale = size * (height / (bottom - top) if design["by"] == "height" else width / (right - left))
    x = cx * width - (left + right) / 2 * scale
    y = cy * height - (strike_top + strike_height / 2) * scale
    place = f"translate({x:.2f} {y:.2f}) scale({scale:.4f})"
    texture = rows(design, width, height) if design.get("rows") else ""
    if texture:
        # The rows run up to the lit wordmark and its strike: the brand's cut,
        # drawn in the ground colour, clears a thin gap around both.
        half_cut = design["cut"] / 2 * scale
        texture += (f'<rect y="{y + strike_top * scale - half_cut:.2f}" width="{width}" '
                    f'height="{strike_height * scale + 2 * half_cut:.2f}" fill="{GROUND}"/>'
                    f'<use href="#letters" transform="{place}" fill="{GROUND}" stroke="{GROUND}" '
                    f'stroke-width="{design["cut"]}" stroke-linejoin="round"/>')
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


# The cube pattern has no letters, so it needs no placement.
JOBS = [(name, size_name, svg(DESIGNS[name], *FORMATS[size_name], cx, cy, size))
        for (name, size_name), (cx, cy, size) in PLACEMENT.items()]
JOBS += [(name, size_name, cubes(*FORMATS[size_name], palette))
         for name, palette in CUBES.items() for size_name in FORMATS]

for name, size_name, drawing in JOBS:
    width, height = FORMATS[size_name]
    stem = f"code-villain-{name}-{size_name}"
    source = HERE / f"{stem}.svg"
    source.write_text(drawing)
    with tempfile.TemporaryDirectory() as profile:
        subprocess.run(
            [CHROME, "--headless=new", f"--user-data-dir={profile}", "--hide-scrollbars",
             "--force-device-scale-factor=1", f"--window-size={width},{height}",
             "--virtual-time-budget=10000",
             f"--screenshot={HERE / f'{stem}.png'}", source.as_uri()],
            check=True, capture_output=True,
        )
    print(f"{stem}: {width}×{height}")
