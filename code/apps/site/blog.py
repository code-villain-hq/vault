"""Render the posts in studio/content/blog into the site's blog pages.

The Markdown is the plain subset the posts use: paragraphs, ## and ###
headings, lists, quotes, fenced code, links, images, `code`, **strong** and
*emphasis*. A paragraph of images alone becomes a figure; an image's title is
its caption, and two images side by side sit on one panel. CSS and SVG/HTML
blocks take the code palette; other languages stay plain.
"""
from __future__ import annotations

from dataclasses import dataclass
from datetime import date as Date
import html
import math
import os
from pathlib import Path
import re
import struct
from string import Template

SITE = Path(__file__).resolve().parent
VAULT = SITE.parents[2]
POSTS = VAULT / 'studio/content/blog'
TEMPLATES = SITE / 'templates'
# Pages are written to <site>/blog, in the vault preview or in dist, so a
# post's images are linked from there.
PAGE_DIR = SITE / 'blog'
WORDS_PER_MINUTE = 200
FIELDS = ('title', 'date', 'product', 'summary', 'draft')


@dataclass
class Post:
    source: str
    slug: str
    title: str
    date: Date
    product: str
    summary: str
    draft: bool
    markdown: str


def posts() -> list[Post]:
    """Every post in studio/content/blog, newest first."""
    found = []
    for path in POSTS.glob('*.md'):
        if path.name == 'README.md':
            continue
        match = re.fullmatch(r'(\d{4}-\d{2}-\d{2})-([a-z0-9-]+)\.md', path.name)
        if not match:
            raise SystemExit(f'{path.name}: name posts YYYY-MM-DD-slug.md')
        meta, markdown = front_matter(path)
        missing = [field for field in FIELDS if field not in meta]
        if missing:
            raise SystemExit(f'{path.name}: front matter needs {", ".join(missing)}')
        if meta['date'] != match[1]:
            raise SystemExit(f'{path.name}: date {meta["date"]} does not match the file name')
        if meta['draft'] not in {'true', 'false'}:
            raise SystemExit(f'{path.name}: draft is true or false')
        found.append(Post(
            source=path.name,
            slug=match[2],
            title=meta['title'],
            date=Date.fromisoformat(meta['date']),
            product=meta['product'],
            summary=meta['summary'],
            draft=meta['draft'] == 'true',
            markdown=markdown,
        ))
    return sorted(found, key=lambda post: (post.date, post.slug), reverse=True)


def front_matter(path: Path) -> tuple[dict[str, str], str]:
    text = path.read_text()
    match = re.match(r'---\n(.*?)\n---\n', text, re.S)
    if not match:
        raise SystemExit(f'{path.name}: starts with --- front matter ---')
    meta = {}
    for line in match[1].splitlines():
        key, _, value = line.partition(':')
        meta[key.strip()] = value.strip()
    return meta, text[match.end():]


def write(target: Path, drafts: bool) -> list[str]:
    """Write the index and one page per post into target; return the file names."""
    target.mkdir(parents=True, exist_ok=True)
    shown = [post for post in posts() if drafts or not post.draft]
    post_page = Template((TEMPLATES / 'post.html').read_text())
    names = []
    for post in shown:
        body, words = render(post.markdown)
        page = post_page.substitute(
            title=attr(post.title),
            summary=attr(post.summary),
            slug=post.slug,
            source=post.source,
            product=html.escape(post.product),
            date=post.date.isoformat(),
            day=day(post.date),
            minutes=max(1, math.ceil(words / WORDS_PER_MINUTE)),
            draft=('\n        <span aria-hidden="true">·</span>\n        ' + DRAFT) if post.draft else '',
            body=body,
        )
        (target / f'{post.slug}.html').write_text(page)
        names.append(f'{post.slug}.html')
    rows = '\n'.join(row(post) for post in shown)
    (target / 'index.html').write_text(Template((TEMPLATES / 'index.html').read_text()).substitute(rows=rows))
    return ['index.html', *names]


DRAFT = '<span class="ui-status ui-status-warn chip">Draft</span>'


def row(post: Post) -> str:
    tags = f'<span class="ui-chip chip">{html.escape(post.product)}</span>'
    if post.draft:
        tags += DRAFT
    return f'''      <li>
        <a class="post-row" href="{post.slug}.html">
          <time class="post-date" datetime="{post.date.isoformat()}">{day(post.date)}</time>
          <span class="post-main">
            <span class="post-title">{html.escape(post.title)}</span>
            <span class="post-excerpt">{html.escape(post.summary)}</span>
          </span>
          <span class="post-tags">{tags}</span>
        </a>
      </li>'''


def day(when: Date) -> str:
    return f'{when.day} {when:%b %Y}'


def attr(text: str) -> str:
    return html.escape(text, quote=True)


# Blocks.

def render(markdown: str) -> tuple[str, int]:
    """The post body as indented HTML, and its word count for the reading time."""
    blocks: list[str] = []
    words = 0
    lines = markdown.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            i += 1
            continue
        if fence := re.match(r'```(\S*)\s*$', line):
            end = i + 1
            while end < len(lines) and not lines[end].startswith('```'):
                end += 1
            if end == len(lines):
                raise SystemExit(f'Code block opened at line {i + 1} never closes')
            blocks.append(code_block(fence[1], '\n'.join(lines[i + 1:end])))
            i = end + 1
            continue
        if heading := re.match(r'(#{2,3})\s+(.*)', line):
            level = len(heading[1])
            blocks.append(f'<h{level}>{inline(heading[2])}</h{level}>')
            words += len(heading[2].split())
            i += 1
            continue
        if re.match(r'#{4,6}\s', line):
            raise SystemExit(f'Heading at line {i + 1}: use ## or ### for sections')
        if re.match(r'#\s', line):
            raise SystemExit('A post has no # heading: the title comes from front matter, sections start at ##')
        if re.match(r'([-*]|\d+\.)\s', line):
            tag = 'ol' if line[0].isdigit() else 'ul'
            items: list[str] = []
            while i < len(lines) and lines[i].strip():
                if item := re.match(r'\s*(?:[-*]|\d+\.)\s+(.*)', lines[i]):
                    items.append(item[1])
                else:
                    items[-1] += ' ' + lines[i].strip()
                i += 1
            words += sum(len(item.split()) for item in items)
            blocks.append(f'<{tag}>\n' + '\n'.join(f'  <li>{inline(item)}</li>' for item in items) + f'\n</{tag}>')
            continue
        if line.startswith('>'):
            quoted = []
            while i < len(lines) and lines[i].startswith('>'):
                quoted.append(lines[i][1:].strip())
                i += 1
            text = ' '.join(quoted)
            words += len(text.split())
            blocks.append(f'<blockquote><p>{inline(text)}</p></blockquote>')
            continue
        paragraph = []
        while i < len(lines) and lines[i].strip() and not re.match(r'```|#{1,6}\s|>|([-*]|\d+\.)\s', lines[i]):
            paragraph.append(lines[i].strip())
            i += 1
        text = ' '.join(paragraph)
        images = list(IMAGE.finditer(text))
        if images and not IMAGE.sub('', text).strip():
            blocks.append(figure(images))
        else:
            words += len(text.split())
            blocks.append(f'<p>{inline(text)}</p>')
    # Indent to sit inside <article>; code keeps its own whitespace.
    indented = []
    in_pre = False
    for line in '\n\n'.join(blocks).split('\n'):
        indented.append(line if in_pre or not line else '      ' + line)
        if '<pre>' in line and '</pre>' not in line:
            in_pre = True
        elif '</pre>' in line:
            in_pre = False
    return '\n'.join(indented), words


IMAGE = re.compile(r'!\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)')


def figure(images: list[re.Match[str]]) -> str:
    tags = [image(match[2], match[1]) for match in images]
    captions = [match[3] for match in images if match[3]]
    caption = f'\n  <figcaption>{inline(captions[0])}</figcaption>' if captions else ''
    if len(tags) == 1:
        return f'<figure class="post-figure">\n  {tags[0]}{caption}\n</figure>'
    pair = '\n'.join(f'    {tag}' for tag in tags)
    return f'<figure class="post-figure post-figure-pair">\n  <div class="pair">\n{pair}\n  </div>{caption}\n</figure>'


def image(src: str, alt: str) -> str:
    """An <img> for a path written relative to studio/content/blog, linked from the page."""
    if re.match(r'[a-z]+:', src):
        return f'<img src="{attr(src)}" alt="{attr(alt)}">'
    path = (POSTS / src).resolve()
    if not path.is_file():
        raise SystemExit(f'Image not found: {src} (paths are relative to studio/content/blog)')
    href = Path(os.path.relpath(path, PAGE_DIR)).as_posix()
    size = dimensions(path)
    sized = f' width="{size[0]}" height="{size[1]}"' if size else ''
    return f'<img src="{attr(href)}" alt="{attr(alt)}"{sized}>'


def dimensions(path: Path) -> tuple[int, int] | None:
    if path.suffix == '.svg':
        head = path.read_text()[:2000]
        if box := re.search(r'<svg[^>]*\bviewBox="[\d.\s-]+?\s([\d.]+)\s+([\d.]+)"', head):
            return round(float(box[1])), round(float(box[2]))
    if path.suffix == '.png':
        width, height = struct.unpack('>II', path.read_bytes()[16:24])
        return width, height
    return None


# Code.

LANGUAGES = {'css': 'CSS', 'svg': 'SVG', 'html': 'HTML', 'xml': 'XML', 'js': 'JavaScript', 'ts': 'TypeScript',
             'tsx': 'TSX', 'json': 'JSON', 'rust': 'Rust', 'sh': 'Shell', 'bash': 'Shell', 'zsh': 'Shell',
             'python': 'Python', 'toml': 'TOML', 'yaml': 'YAML', 'swift': 'Swift', 'lua': 'Lua'}


def code_block(language: str, code: str) -> str:
    label = f'\n  <figcaption>{LANGUAGES.get(language, language)}</figcaption>' if language else ''
    if language == 'css':
        body = highlight_css(code)
    elif language in {'svg', 'html', 'xml'}:
        body = highlight_markup(code)
    else:
        body = html.escape(code, quote=False)
    return f'<figure class="code">{label}\n  <pre><code>{body}</code></pre>\n</figure>'


def span(kind: str, text: str) -> str:
    return f'<span class="tok-{kind}">{html.escape(text, quote=False)}</span>'


def highlight_css(code: str) -> str:
    out = []
    depth = 0
    for token in re.finditer(r'(/\*.*?\*/)|("[^"]*"|\'[^\']*\')|([{}])|(--[\w-]+|[a-z-]+)(?=\s*:)'
                             r'|(#[0-9a-fA-F]{3,8}\b|-?\d+(?:\.\d+)?(?:[a-z]+|%)?)|(.)', code, re.S):
        comment, string, brace, prop, number, other = token.groups()
        if comment:
            out.append(span('c', comment))
        elif string:
            out.append(span('s', string))
        elif brace:
            depth += 1 if brace == '{' else -1
            out.append(brace)
        elif prop and depth:
            out.append(span('p', prop))
        elif number and depth:
            out.append(span('n', number))
        else:
            out.append(html.escape(token[0], quote=False))
    return ''.join(out)


def highlight_markup(code: str) -> str:
    def tag(match: re.Match[str]) -> str:
        close, name, attrs, end = match.groups()
        attrs = re.sub(r'([\w:.-]+)(=)("[^"]*"|\'[^\']*\')|([^"\'\w]+|[\w:.-]+)',
                       lambda a: span('p', a[1]) + a[2] + span('s', a[3]) if a[1] else html.escape(a[0], quote=False),
                       attrs)
        return f'&lt;{close}{span("t", name)}{attrs}{html.escape(end, quote=False)}&gt;'

    out = []
    last = 0
    for match in re.finditer(r'<!--.*?-->|<(/?)([\w:.-]+)((?:[^>"\']|"[^"]*"|\'[^\']*\')*?)(/?)>', code, re.S):
        out.append(html.escape(code[last:match.start()], quote=False))
        out.append(span('c', match[0]) if match[0].startswith('<!--') else tag(match))
        last = match.end()
    out.append(html.escape(code[last:], quote=False))
    return ''.join(out)


# Inline.

INLINE = re.compile(r'`([^`]+)`|' + IMAGE.pattern + r'|\[([^\]]+)\]\(([^)\s]+)\)')


def inline(text: str) -> str:
    out = []
    last = 0
    for match in INLINE.finditer(text):
        out.append(prose(text[last:match.start()], text[last - 1] if last else ' '))
        code, alt, src, _title, label, href = match.groups()
        if code is not None:
            out.append(f'<code>{html.escape(code, quote=False)}</code>')
        elif src is not None:
            out.append(image(src, alt))
        else:
            out.append(f'<a href="{attr(href)}">{inline(label)}</a>')
        last = match.end()
    out.append(prose(text[last:], text[last - 1] if last else ' '))
    return ''.join(out)


def prose(text: str, before: str) -> str:
    """Escaped text with typographic quotes and **strong**/*emphasis*."""
    text = quotes(before + text)[1:]
    text = html.escape(text, quote=False)
    text = re.sub(r'\*\*(?=\S)(.+?)(?<=\S)\*\*', r'<strong>\1</strong>', text)
    text = re.sub(r'(?<![\w*])[*_](?=\S)(.+?)(?<=\S)[*_](?![\w*])', r'<em>\1</em>', text)
    return text


def quotes(text: str) -> str:
    text = re.sub(r"(?<=\w)'", '’', text)
    text = re.sub(r"(?<=[\s(\[—–])'|^'", '‘', text)
    text = text.replace("'", '’')
    text = re.sub(r'(?<=[\s(\[—–])"|^"', '“', text)
    return text.replace('"', '”')
