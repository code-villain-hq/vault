"""The product pages, and the Products menu every page shares.

Each product's words live in studio/content/products/<slug>.toml. A page sits
one folder down from the site (`forge/index.html`), like the blog.
"""
from __future__ import annotations

from dataclasses import dataclass
import html
from pathlib import Path
import struct
from string import Template
import tomllib

SITE = Path(__file__).resolve().parent
CONTENT = SITE.parents[2] / 'studio/content/products'
TEMPLATES = SITE / 'templates'
CAPTURES = SITE / 'assets/captures'

# Each icon, from the site folder. build.py rewrites these for deployment.
ICONS = {
    'forge': '../forge/apps/web/public/icon.svg',
    'layer': '../layer/src-tauri/icons/icon.svg',
    'caster': '../caster/Resources/AppIcon.svg',
    'glyph': '../glyph/apps/web/public/icon.svg',
    'raven': '../raven/apps/web/public/icon.svg',
    'theme': '../../../studio/brand/mark/mark.svg',
}

CHEVRON = '<svg class="chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>'


@dataclass
class Product:
    slug: str
    data: dict

    @property
    def name(self) -> str:
        return self.data['name']

    @property
    def line(self) -> str:
        return self.data['line']


def products() -> list[Product]:
    found = [Product(path.stem, tomllib.loads(path.read_text())) for path in CONTENT.glob('*.toml')]
    return sorted(found, key=lambda product: product.data['order'])


def icon(slug: str, up: int) -> str:
    """The icon's path from a page `up` folders below the site."""
    return '../' * up + ICONS[slug]


def menu(up: int, here: str | None = None) -> str:
    """The Products menu and The log, for a page `up` folders below the site."""
    root = '../' * up
    rows = []
    for product in products():
        current = ' aria-current="page"' if product.slug == here else ''
        rows.append(
            f'          <li><a href="{root}{product.slug}/"{current}>'
            f'<img src="{icon(product.slug, up)}" width="32" height="32" alt="">'
            f'<span><b>{html.escape(product.name)}</b><span>{html.escape(product.line)}</span></span></a></li>'
        )
    log = ' aria-current="page"' if here == 'log' else ''
    return '\n'.join([
        '      <details class="products-menu">',
        f'        <summary class="ui-link">Products{CHEVRON}</summary>',
        '        <ul>',
        *rows,
        '        </ul>',
        '      </details>',
        f'      <a class="ui-link" href="{root}blog/"{log}>The log</a>',
    ])


def size(name: str) -> tuple[int, int]:
    """A capture's width and height in CSS pixels: they are taken at 2×."""
    with (CAPTURES / name).open('rb') as file:
        width, height = struct.unpack('>II', file.read(24)[16:24])
    return width // 2, height // 2


def figure(block: dict, stack: bool = False) -> str:
    images = []
    for capture in block['captures']:
        width, height = size(capture['src'])
        images.append(
            f'<img src="../assets/captures/{capture["src"]}" width="{width}" height="{height}" '
            f'alt="{html.escape(capture["alt"])}" loading="lazy">'
        )
    stage = 'capture-stage capture-stack' if stack else 'capture-stage'
    caption = f'\n        <figcaption>{html.escape(block["caption"])}</figcaption>' if block.get('caption') else ''
    return (
        f'<figure class="product-figure">\n        <div class="{stage}">\n          '
        + '\n          '.join(images)
        + f'\n        </div>{caption}\n      </figure>'
    )


def page(product: Product, template: Template) -> str:
    data = product.data
    links = []
    for link in data.get('get', []):
        style = 'ui-button ui-button-primary' if link.get('primary') else 'ui-link'
        links.append(f'<a class="{style}" href="{html.escape(link["href"])}">{html.escape(link["label"])}</a>')
    features = []
    for feature in data.get('feature', []):
        features.append(
            '    <section class="feature">\n'
            f'      <div class="feature-text"><h2>{html.escape(feature["title"])}</h2><p>{html.escape(feature["text"])}</p></div>\n'
            f'      {figure(feature, feature.get("stack", False))}\n'
            '    </section>'
        )
    points = ''.join(
        f'\n      <li><strong>{html.escape(point["title"])}</strong> {html.escape(point["text"])}</li>'
        for point in data.get('point', [])
    )
    return template.substitute(
        slug=product.slug,
        name=html.escape(product.name),
        line=html.escape(product.line),
        description=html.escape(data['lead'], quote=True),
        lead=html.escape(data['lead']),
        icon=icon(product.slug, 1),
        menu=menu(1, product.slug),
        get=' '.join(links),
        hero=figure(data['hero'], data['hero'].get('stack', False)),
        features='\n'.join(features),
        points=points,
        fact=html.escape(data['fact']),
    )


def write(target: Path) -> list[str]:
    """Write one page per product into target; return their paths."""
    template = Template((TEMPLATES / 'product.html').read_text())
    names = []
    for product in products():
        folder = target / product.slug
        folder.mkdir(parents=True, exist_ok=True)
        (folder / 'index.html').write_text(page(product, template))
        names.append(f'{product.slug}/index.html')
    return names
