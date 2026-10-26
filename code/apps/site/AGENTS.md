# Site: notes for coding agents

The Code Villain website at https://www.codevillain.eu/: the homepage, a page per product, and the blog. [`README.md`](README.md) says how it is built and what each page shows; read it first. Run commands from this folder.

- Studio surfaces stay night and follow [`studio/brand/`](../../../studio/brand/README.md). Read [`studio/design/principles.md`](../../../studio/design/principles.md) before any visual change, and [`studio/design/layouts/blog-post.md`](../../../studio/design/layouts/blog-post.md) before changing the blog.
- Colour, type and shape come from [`studio/design/tokens`](../../../studio/design/tokens) and the shared [`studio/design/components.css`](../../../studio/design/components.css). No literal colours, sizes or radii in the site's CSS; `studio/design/lint` fails CI on them.
- Product pages come from [`studio/content/products`](../../../studio/content/products) through [`products.py`](products.py); edit the TOML, never the generated `<product>/` folders. The homepage's Products menu must match `products.menu(0)`.
- Posts live in [`studio/content/blog`](../../../studio/content/blog). Edit the Markdown, [`templates/`](templates) or [`blog.py`](blog.py), never the generated `blog/`.
- `python3 build.py` renders the blog and the product pages and writes `dist/`, checking every local dependency. Preview through `vault-static` in the vault's `.claude/launch.json`, at `/apps/site/`.
- Deploying lives in the private `infra` repository. Show a change in the preview and ask before publishing it.
