#!/usr/bin/env python3
"""Package the static website and its explicit public dependencies for Uberspace."""
from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import re
import shutil
from urllib.parse import unquote, urljoin, urlsplit

SITE = Path(__file__).resolve().parent
VAULT = SITE.parent.parent
OUTPUT = SITE / 'dist'

# Keep the vault preview's relative references in source. Only the public
# package gets root-relative URLs; the rest of the vault is never uploaded.
REPLACEMENTS = {
    '../../../brand/mark/': '/assets/brand/',
    '../../brand/mark/': '/assets/brand/',
    '../../../design/components.css': '/assets/components.css',
    '../../design/components.css': '/assets/components.css',
    '../../../design/tokens/': '/assets/tokens/',
    '../../design/tokens/': '/assets/tokens/',
    '../forge/apps/web/public/icon.svg': '/assets/products/forge.svg',
    '../layer/src-tauri/icons/icon.svg': '/assets/products/layer.svg',
    '../caster/Resources/AppIcon.svg': '/assets/products/caster.svg',
    '../../../content/blog/images/': '/assets/blog/',
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
    for relative in ['index.html', 'blog/index.html', 'blog/villain-gets-a-face.html']:
        text = (SITE / relative).read_text()
        for before, after in REPLACEMENTS.items():
            text = text.replace(before, after)
        target = OUTPUT / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    for name in ['site.css', 'home.css']:
        copy(SITE / name, name)
    for path in (SITE / 'assets/fonts').iterdir():
        if path.suffix in {'.ttf', '.css', '.txt'}:
            copy(path, 'assets/fonts/' + path.name)
    # Only captures a page shows: the folder also holds drafts not yet on the site.
    pages = ''.join((OUTPUT / page).read_text() for page in ['index.html', 'blog/index.html', 'blog/villain-gets-a-face.html'])
    for path in (SITE / 'assets/captures').glob('*.png'):
        if f'assets/captures/{path.name}' in pages:
            copy(path, 'assets/captures/' + path.name)
    for path in (VAULT / 'brand/mark').glob('*.svg'):
        copy(path, 'assets/brand/' + path.name)
    for path in (VAULT / 'design/tokens').glob('*.css'):
        copy(path, 'assets/tokens/' + path.name)
    copy(VAULT / 'design/components.css', 'assets/components.css')
    for path in (VAULT / 'content/blog/images').glob('*.svg'):
        copy(path, 'assets/blog/' + path.name)
    for product, path in {
        'forge': 'apps/forge/apps/web/public/icon.svg',
        'layer': 'apps/layer/src-tauri/icons/icon.svg',
        'caster': 'apps/caster/Resources/AppIcon.svg',
    }.items():
        copy(VAULT / path, f'assets/products/{product}.svg')
    copy(SITE / 'uberspace.htaccess', '.htaccess')
    validate()
    files = [p for p in OUTPUT.rglob('*') if p.is_file()]
    print(f'Built {len(files)} public files ({sum(p.stat().st_size for p in files):,} bytes) in {OUTPUT}')


if __name__ == '__main__':
    main()
