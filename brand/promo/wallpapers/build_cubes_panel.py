"""Compose the Cubes panel: the engraved cubes alone, closer together.

Run from this folder: python3 build_cubes_panel.py
The clean cubes raster keeps a wide empty middle for the wordmark. This
keeps both clusters at the raster's native resolution and closes that gap,
for narrow, nearly square surfaces such as the sign-in panel. The seam runs
diagonally through empty ground, between the lower-left and upper-right
clusters. Requires Google Chrome.
"""
import base64
import pathlib
import struct
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
source = HERE / "code-villain-cubes-cloak-clean.png"
data = source.read_bytes()
source_width, height = struct.unpack(">II", data[16:24])
image = base64.b64encode(data).decode()

width = 900
# The seam runs from (TOP, 0) to (BOTTOM, height): left of it the source's
# left edge, right of it the source's right edge, blended over FEATHER px.
TOP, BOTTOM, FEATHER = 340, 560, 30
dx, dy = BOTTOM - TOP, height
length = (dx * dx + dy * dy) ** 0.5
nx, ny = dy / length, -dx / length
mx, my = (TOP + BOTTOM) / 2, height / 2
x1, y1 = mx - nx * FEATHER / 2, my - ny * FEATHER / 2
x2, y2 = mx + nx * FEATHER / 2, my + ny * FEATHER / 2

stem = "code-villain-cubes-cloak-panel"
svg = HERE / f"{stem}.svg"
png = HERE / f"{stem}.png"
svg.write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<title>Code Villain — Cubes, panel</title>
<defs>
  <image id="cubes" width="{source_width}" height="{height}" href="data:image/png;base64,{image}"/>
  <linearGradient id="seam" gradientUnits="userSpaceOnUse" x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}">
    <stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/>
  </linearGradient>
  <mask id="right"><rect width="{width}" height="{height}" fill="url(#seam)"/></mask>
</defs>
<use href="#cubes"/>
<g mask="url(#right)"><use href="#cubes" x="{width - source_width}"/></g>
</svg>''')
with tempfile.TemporaryDirectory() as profile:
    subprocess.run([
        CHROME, "--headless=new", f"--user-data-dir={profile}",
        "--hide-scrollbars", "--force-device-scale-factor=1",
        f"--window-size={width},{height}", "--virtual-time-budget=10000",
        f"--screenshot={png}", svg.as_uri(),
    ], check=True, capture_output=True)
print(f"{stem}: {width}×{height}")
