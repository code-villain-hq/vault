"""Write the images Pocket ID shows at id.codevillain.eu, from brand/.

Run from this folder: python3 build.py
Needs Google Chrome for the email logo and macOS sips for the background.
README.md says where each file goes.
"""
import pathlib
import shutil
import subprocess
import tempfile

HERE = pathlib.Path(__file__).parent
BRAND = HERE / "../../brand"
MARK = (BRAND / "mark/mark.svg").read_text()
CUBES = BRAND / "promo/wallpapers/code-villain-cubes-cloak-panel.png"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
OUT = HERE / "kit"

TILE = '  <rect width="200" height="200" rx="45" fill="#0B0B0B"/>\n'


def glyph(letters, strike):
    """The mark without its tile, like the menu bar glyph: letters cut into the strike."""
    assert TILE in MARK, "mark.svg changed shape; update build.py"
    # Pocket ID draws the logo at 40px inside its own 64px tile, so crop closer to
    # the letters (x 32-168), leaving the strike 13 units past each cut.
    return (MARK.replace(TILE, "")
            .replace('viewBox="0 0 200 200"', 'viewBox="12 40 176 128"')
            .replace('fill="#EBEBEB"', f'fill="{letters}"')
            .replace('fill="#C9E265"', f'fill="{strike}"'))


OUT.mkdir(exist_ok=True)
# Pocket ID's logo slot is square. Day: day fg and deep poison. Night: night fg and poison.
(OUT / "logo-light.svg").write_text(glyph("#181818", "#4C6A00"))
(OUT / "logo-dark.svg").write_text(glyph("#EBEBEB", "#C9E265"))
# The tile is the app icon, so it is the favicon.
shutil.copy(BRAND / "mark/mark.svg", OUT / "favicon.svg")

# Emails show the logo at 28px with rounded corners: the tile, at 4x. Chrome
# won't draw a window that small, so it renders 448px and sips scales it down.
with tempfile.TemporaryDirectory() as work:
    page = pathlib.Path(work, "tile.html")
    page.write_text(f'<body style="margin:0"><img src="{(OUT / "favicon.svg").resolve().as_uri()}"'
                    ' width="448" height="448" style="display:block">')
    large = pathlib.Path(work, "tile.png")
    subprocess.run(
        [CHROME, "--headless=new", f"--user-data-dir={work}/profile", "--hide-scrollbars",
         "--force-device-scale-factor=1", "--window-size=448,448",
         "--default-background-color=00000000", f"--screenshot={large}", page.as_uri()],
        check=True, capture_output=True,
    )
    subprocess.run(["sips", "-Z", "112", large, "--out", OUT / "logo-email.png"],
                   check=True, capture_output=True)

# The sign-in page's side panel: the engraved cubes, compressed for the web.
subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "90",
                CUBES, "--out", OUT / "background.jpg"], check=True, capture_output=True)

for file in sorted(OUT.iterdir()):
    print(f"{file.name}: {file.stat().st_size // 1024} KB")
