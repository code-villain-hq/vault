#!/usr/bin/env python3
"""Package the static website and its explicit public dependencies for Uberspace."""
from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import re
import shutil
from urllib.parse import unquote, urljoin, urlsplit

import blog
import products

SITE = Path(__file__).resolve().parent
VAULT = SITE.parents[2]
OUTPUT = SITE / 'dist'

# Keep the vault preview's relative references in source. Only the public
# package gets root-relative URLs; the rest of the vault is never uploaded.
REPLACEMENTS = {
    '../../../../studio/brand/mark/': '/assets/brand/',
    '../../../studio/brand/mark/': '/assets/brand/',
    '../../../../studio/design/components.css': '/assets/components.css',
    '../../../studio/design/components.css': '/assets/components.css',
    '../../../../studio/design/tokens/': '/assets/tokens/',
    '../../../studio/design/tokens/': '/assets/tokens/',
    # Pages one folder down first: the shorter paths are inside these.
    '../../forge/apps/web/public/icon.svg': '/assets/products/forge.svg',
    '../../layer/src-tauri/icons/icon.svg': '/assets/products/layer.svg',
    '../../caster/Resources/AppIcon.svg': '/assets/products/caster.svg',
    '../../glyph/apps/web/public/icon.svg': '/assets/products/glyph.svg',
    '../../raven/apps/web/public/icon.svg': '/assets/products/raven.svg',
    '../forge/apps/web/public/icon.svg': '/assets/products/forge.svg',
    '../layer/src-tauri/icons/icon.svg': '/assets/products/layer.svg',
    '../caster/Resources/AppIcon.svg': '/assets/products/caster.svg',
    '../glyph/apps/web/public/icon.svg': '/assets/products/glyph.svg',
    '../raven/apps/web/public/icon.svg': '/assets/products/raven.svg',
    '../../../../studio/content/blog/images/': '/assets/blog/',
}


def copy(source: Path, destination: str) -> None:
    target = OUTPUT / destination
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


class Links(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.urls: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.urls.extend(value for name, value in attrs if name in {'href', 'src'} and value)


def validate() -> None:
    missing: list[str] = []
    for page in OUTPUT.rglob('*.html'):
        parser = Links()
        parser.feed(page.read_text())
        base = 'https://www.codevillain.eu/' + page.relative_to(OUTPUT).as_posix()
        for value in parser.urls:
            url = urlsplit(urljoin(base, value))
            if url.netloc != 'www.codevillain.eu':
                continue
            path = OUTPUT / unquote(url.path).lstrip('/')
            if path.is_dir():
                path /= 'index.html'
            if not path.is_file():
                missing.append(f'{page.relative_to(OUTPUT)}: {value}')
    for css in OUTPUT.rglob('*.css'):
        for value in re.findall(r'url\([\s\'\"]*([^\)\'\"\s]+)', css.read_text()):
            if not value.startswith(('data:', 'http:', 'https:', '#')):
                path = OUTPUT / value.lstrip('/') if value.startswith('/') else css.parent / value
                if not path.is_file():
                    missing.append(f'{css.relative_to(OUTPUT)}: {value}')
    if missing:
        raise SystemExit('Missing public dependencies:\n' + '\n'.join(missing))


def main() -> None:
    if OUTPUT.exists():
        shutil.rmtree(OUTPUT)
    OUTPUT.mkdir()
    # The vault preview shows drafts; the public package leaves them out.
    shutil.rmtree(SITE / 'blog', ignore_errors=True)
    blog.write(SITE / 'blog', drafts=True)
    for product in products.products():
        shutil.rmtree(SITE / product.slug, ignore_errors=True)
    products.write(SITE)
    copy(SITE / 'index.html', 'index.html')
    pages = [
        'index.html',
        *('blog/' + name for name in blog.write(OUTPUT / 'blog', drafts=False)),
        *products.write(OUTPUT),
    ]
    for relative in pages:
        text = (OUTPUT / relative).read_text()
        for before, after in REPLACEMENTS.items():
            text = text.replace(before, after)
        (OUTPUT / relative).write_text(text)
    for name in ['site.css', 'home.css', 'product.css', 'menu.js']:
        copy(SITE / name, name)
    for path in (SITE / 'assets/fonts').iterdir():
        if path.suffix in {'.ttf', '.css', '.txt'}:
            copy(path, 'assets/fonts/' + path.name)
    # Only captures a page shows: the folder also holds drafts not yet on the site.
    shown = ''.join((OUTPUT / page).read_text() for page in pages)
    for path in (SITE / 'assets/captures').glob('*.png'):
        if f'assets/captures/{path.name}' in shown:
            copy(path, 'assets/captures/' + path.name)
    for path in (VAULT / 'studio/brand/mark').glob('*.svg'):
        copy(path, 'assets/brand/' + path.name)
    for path in (VAULT / 'studio/design/tokens').glob('*.css'):
        copy(path, 'assets/tokens/' + path.name)
    copy(VAULT / 'studio/design/components.css', 'assets/components.css')
    for path in (p for p in (VAULT / 'studio/content/blog/images').glob('*') if p.is_file() and not p.name.startswith('.')):
        copy(path, 'assets/blog/' + path.name)
    for product, path in {
        'forge': 'code/apps/forge/apps/web/public/icon.svg',
        'layer': 'code/apps/layer/src-tauri/icons/icon.svg',
        'caster': 'code/apps/caster/Resources/AppIcon.svg',
        'glyph': 'code/apps/glyph/apps/web/public/icon.svg',
        'raven': 'code/apps/raven/apps/web/public/icon.svg',
    }.items():
        copy(VAULT / path, f'assets/products/{product}.svg')
    copy(SITE / 'uberspace.htaccess', '.htaccess')
    validate()
    files = [p for p in OUTPUT.rglob('*') if p.is_file()]
    print(f'Built {len(files)} public files ({sum(p.stat().st_size for p in files):,} bytes) in {OUTPUT}')


if __name__ == '__main__':
    main()
