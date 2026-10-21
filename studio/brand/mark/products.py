"""Write the studio mark and each product's mark: letters on the night tile,
cut into the strike.

Run from this folder: python3 products.py
Needs Swift (outline.swift reads the font with CoreText). Every mark shares
one construction (README.md); only the letters differ.
"""
import pathlib
import re
import subprocess

HERE = pathlib.Path(__file__).parent
FONT = HERE / "../../../code/apps/site/assets/fonts/grenze-gotisch.ttf"
PRODUCTS = {"forge": "Forge", "layer": "Layer", "caster": "Caster", "glyph": "Glyph", "raven": "Raven"}
# The studio pair: C and V at 0.88 of a product initial, 1 unit apart, so
# their cuts meet and the strike stops between them. Centred on the tile,
# with the pair's middle at y=104.5, so the strike crosses it low as before.
PAIR_SCALE, PAIR_GAP, PAIR_MIDDLE = 0.88, 1, 104.5

TEMPLATE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" role="img" aria-label="{name}">
  <title>{name}</title>
  <!-- Grenze Gotisch 800, 150 units, centered at x=100 with baseline y=152. -->
  <defs>
    <path id="{id}" d="{path}"/>
    <mask id="cut">
      <rect width="200" height="200" fill="#fff"/>
      <use href="#{id}" fill="#000" stroke="#000" stroke-width="14" stroke-linejoin="round"/>
    </mask>
  </defs>
  <rect width="200" height="200" rx="45" fill="#0B0B0B"/>
  <rect y="108" width="200" height="15" fill="#C9E265" mask="url(#cut)"/>
  <use href="#{id}" fill="#EBEBEB"/>
</svg>
"""


def outline(letter):
    return subprocess.run(["swift", HERE / "outline.swift", FONT, letter],
                          check=True, capture_output=True, text=True).stdout.strip()


def points(path):
    numbers = [float(n) for n in re.findall(r"-?\d+\.?\d*", path)]
    return numbers[0::2], numbers[1::2]


def moved(path, scale, dx, dy):
    """The same outline, scaled and shifted, with every coordinate baked in."""
    pairs = iter(re.findall(r"[MLQCZ]|-?\d+\.?\d*", path))
    out = ""
    for token in pairs:
        if token.isalpha():
            out += token
            continue
        x, y = dx + scale * float(token), dy + scale * float(next(pairs))
        out += ("" if out[-1:].isalpha() else " ") + f"{x:.2f} {y:.2f}"
    return out


def pair(first, second):
    paths = [outline(first), outline(second)]
    boxes = [points(p) for p in paths]
    widths = [PAIR_SCALE * (max(xs) - min(xs)) for xs, _ in boxes]
    top = min(min(ys) for _, ys in boxes)
    bottom = max(max(ys) for _, ys in boxes)
    dy = PAIR_MIDDLE - PAIR_SCALE * (top + bottom) / 2
    left = 100 - (sum(widths) + PAIR_GAP) / 2
    result = ""
    for path, (xs, _), width in zip(paths, boxes, widths):
        result += moved(path, PAIR_SCALE, left - PAIR_SCALE * min(xs), dy)
        left += width + PAIR_GAP
    return result


studio = TEMPLATE.format(name="Code Villain", id="cv", path=pair("C", "V")).replace(
    "<!-- Grenze Gotisch 800, 150 units, centered at x=100 with baseline y=152. -->",
    "<!-- Grenze Gotisch 800: C and V at 0.88 of 150 units, 1 unit apart, centred. -->")
(HERE / "mark.svg").write_text(studio)
print("mark.svg: CV")

for file, name in PRODUCTS.items():
    letter = name[0]
    (HERE / f"{file}.svg").write_text(TEMPLATE.format(name=name, id=letter.lower(), path=outline(letter)))
    print(f"{file}.svg: {letter}")
