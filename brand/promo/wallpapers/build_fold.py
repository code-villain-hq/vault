"""Compose the Fold wordmark wallpaper from the selected raster and brand mark.

Run from this folder: python3 build_fold.py
The poison-only wallpaper is the source; its original pixels are preserved.
"""
import base64
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
MARK = (HERE / "../../mark/logo-night.svg").resolve()
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
WIDTH, HEIGHT = 1672, 941
# The source's poison line occupies rows 495 through 501.
SCALE = 7 / 8
LINE_CENTER = 498.5
letters = re.search(r'<path id="lettering" d="([^"]+)"', MARK.read_text()).group(1)
image = base64.b64encode((HERE / "code-villain-fold-poison.png").read_bytes()).decode()
# Keep the lettering in the open left field, clear of the folded form.
x = WIDTH * 0.12 - 10 * SCALE
y = LINE_CENTER - (35.48 + 8 / 2) * SCALE
place = f"translate({x:.4f} {y:.4f}) scale({SCALE})"
stem = "code-villain-fold-poison-wordmark"
source = HERE / f"{stem}.svg"
output = HERE / f"{stem}.png"
source.write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<title>Code Villain — Fold, with wordmark</title>
<defs>
  <path id="letters" d="{letters}"/>
  <mask id="cut" maskUnits="userSpaceOnUse" x="0" y="0" width="{WIDTH}" height="{HEIGHT}">
    <rect width="{WIDTH}" height="{HEIGHT}" fill="white"/>
    <use href="#letters" transform="{place}" fill="black" stroke="black" stroke-width="7" stroke-linejoin="round"/>
  </mask>
</defs>
<rect width="{WIDTH}" height="{HEIGHT}" fill="#0B0B0B"/>
<image width="{WIDTH}" height="{HEIGHT}" href="data:image/png;base64,{image}" mask="url(#cut)"/>
<use href="#letters" transform="{place}" fill="#EBEBEB"/>
</svg>''')
with tempfile.TemporaryDirectory() as profile:
    subprocess.run([
        CHROME, "--headless=new", f"--user-data-dir={profile}",
        "--hide-scrollbars", "--force-device-scale-factor=1",
        f"--window-size={WIDTH},{HEIGHT}", "--virtual-time-budget=10000",
        f"--screenshot={output}", source.as_uri(),
    ], check=True, capture_output=True)
print(f"{stem}: {WIDTH}×{HEIGHT}")
