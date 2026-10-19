"""Compose the Arch wallpaper with the original outlined studio wordmark.

Run from this folder: python3 build_arch.py
Retains the generated raster's native resolution; requires Google Chrome.
"""
import base64
import pathlib
import struct
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
source = HERE / "code-villain-arch-cloak.png"
data = source.read_bytes()
width, height = struct.unpack(">II", data[16:24])
image = base64.b64encode(data).decode()
mark = (HERE / "../../mark/logo-night.svg").read_text()
# The original SVG has transparent surroundings and includes its cut strike.
mark = mark[mark.index("<svg"):]
mark_width = width * 0.32
mark_height = mark_width * 72 / 348
x = (width - mark_width) / 2
y = height * 0.53 - mark_height / 2
mark = mark.replace('<svg ', f'<svg x="{x}" y="{y}" width="{mark_width}" height="{mark_height}" ', 1)
stem = "code-villain-arch-cloak-wordmark"
svg = HERE / f"{stem}.svg"
png = HERE / f"{stem}.png"
svg.write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<title>Code Villain — Arch, with wordmark</title>
<image width="{width}" height="{height}" href="data:image/png;base64,{image}"/>
{mark}
</svg>''')
with tempfile.TemporaryDirectory() as profile:
    subprocess.run([
        CHROME, "--headless=new", f"--user-data-dir={profile}",
        "--hide-scrollbars", "--force-device-scale-factor=1",
        f"--window-size={width},{height}", "--virtual-time-budget=10000",
        f"--screenshot={png}", svg.as_uri(),
    ], check=True, capture_output=True)
print(f"{stem}: {width}×{height}")
