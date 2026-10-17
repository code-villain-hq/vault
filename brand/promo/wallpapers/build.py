"""Generate the Code Villain wallpapers from the mark, then render them.

Run from this folder: python3 build.py
Writes one SVG per format and renders a PNG of each with headless Chrome.
The V and its strike come from brand/mark/mark.svg; nothing is redrawn.
"""
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).parent
MARK = (HERE / "../../mark/mark.svg").resolve().read_text()
V = re.search(r'<path id="v" d="([^"]+)"', MARK).group(1)
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

# Mark construction, in its 200-unit space (brand/mark/README.md).
GROUND, INK, POISON = "#0A0B0D", "#ECECF1", "#C9E265"
V_CENTRE_X, V_HEIGHT = 97.7, 94.95  # the V's outline, measured from its path
BAR_TOP, BAR_HEIGHT, CUT = 108, 15, 14

# name: (width, height, V centre x, strike centre y, V height), as fractions of the screen.
FORMATS = {
    "desktop-5k": (5120, 2880, 0.66, 0.56, 0.44),
    "macbook": (3456, 2234, 0.66, 0.56, 0.42),
    "phone": (1320, 2868, 0.50, 0.64, 0.26),
}


def svg(width, height, cx, cy, size):
    scale = size * height / V_HEIGHT
    x = cx * width - V_CENTRE_X * scale
    y = cy * height - (BAR_TOP + BAR_HEIGHT / 2) * scale
    place = f"translate({x:.2f} {y:.2f}) scale({scale:.4f})"
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <title>Code Villain</title>
  <defs>
    <path id="v" d="{V}"/>
    <mask id="cut" maskUnits="userSpaceOnUse" x="0" y="0" width="{width}" height="{height}">
      <rect width="{width}" height="{height}" fill="#fff"/>
      <use href="#v" transform="{place}" fill="#000" stroke="#000" stroke-width="{CUT}" stroke-linejoin="round"/>
    </mask>
  </defs>
  <rect width="{width}" height="{height}" fill="{GROUND}"/>
  <rect y="{y + BAR_TOP * scale:.2f}" width="{width}" height="{BAR_HEIGHT * scale:.2f}" fill="{POISON}" mask="url(#cut)"/>
  <use href="#v" transform="{place}" fill="{INK}"/>
</svg>
"""


for name, (width, height, cx, cy, size) in FORMATS.items():
    source = HERE / f"code-villain-{name}.svg"
    source.write_text(svg(width, height, cx, cy, size))
    with tempfile.TemporaryDirectory() as profile:
        subprocess.run(
            [CHROME, "--headless=new", f"--user-data-dir={profile}", "--hide-scrollbars",
             "--force-device-scale-factor=1", f"--window-size={width},{height}",
             f"--screenshot={HERE / f'code-villain-{name}.png'}", source.as_uri()],
            check=True, capture_output=True,
        )
    print(f"{name}: {width}×{height}")
