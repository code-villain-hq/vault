# Site

The Villain Studios website: the homepage, and the blog in [`blog/`](blog). Shared styles are in [`site.css`](site.css).

The blog shows posts from [`content/blog`](../../content/blog). There is no build step yet: each post page is written by hand from its Markdown. The blog index lists one real post (a draft) and three sample rows for the design, marked in the HTML.

It reads colour, type, and shape from [`design/tokens`](../../design/tokens) by relative path, so open it through the vault's preview server (`vault-static` in `.claude/launch.json`, then `/apps/site/`), not from disk. A deploy has to copy the tokens in.
