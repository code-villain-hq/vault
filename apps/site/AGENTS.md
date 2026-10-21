# Site: notes for coding agents

The Code Villain website at https://www.codevillain.eu/: the homepage and the blog. [`README.md`](README.md) says how it is built and what each page shows; read it first. Run commands from this folder.

- Studio surfaces stay night and follow [`brand/`](../../brand/README.md). Read [`design/principles.md`](../../design/principles.md) before any visual change, and [`design/layouts/blog-post.md`](../../design/layouts/blog-post.md) before changing the blog.
- Colour, type and shape come from [`design/tokens`](../../design/tokens) and the shared [`design/components.css`](../../design/components.css). No literal colours, sizes or radii in `site.css` or `home.css`; `design/lint` fails CI on them.
- Posts live in [`content/blog`](../../content/blog). Edit the Markdown, [`templates/`](templates) or [`blog.py`](blog.py), never the generated `blog/`.
- `python3 build.py` renders the blog and writes `dist/`, checking every local dependency. Preview through `vault-static` in the vault's `.claude/launch.json`, at `/apps/site/`.
- Deploying lives in the private `infra` repository. Show a change in the preview and ask before publishing it.
