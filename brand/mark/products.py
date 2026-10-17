"""Write each product's mark: its initial on the night tile, cut into the strike.

Run from this folder: python3 products.py
Needs Swift (outline.swift reads the font with CoreText). Every product
mark shares the V mark's construction (README.md); only the letter differs.
"""
import pathlib
import subprocess

HERE = pathlib.Path(__file__).parent
FONT = HERE / "../../apps/site/assets/fonts/grenze-gotisch.ttf"
PRODUCTS = {"forge": "Forge", "layer": "Layer", "caster": "Caster"}

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
  <rect width="200" height="200" rx="45" fill="#0A0B0D"/>
  <rect y="108" width="200" height="15" fill="#C9E265" mask="url(#cut)"/>
  <use href="#{id}" fill="#ECECF1"/>
</svg>
"""

for file, name in PRODUCTS.items():
    letter = name[0]
    path = subprocess.run(["swift", HERE / "outline.swift", FONT, letter],
                          check=True, capture_output=True, text=True).stdout.strip()
    (HERE / f"{file}.svg").write_text(TEMPLATE.format(name=name, id=letter.lower(), path=path))
    print(f"{file}.svg: {letter}")
