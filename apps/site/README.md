# Site

The Villain Studios website. Today it is one page, the homepage: the logo, the boast, and the tools.

It will publish the blog from [`content/blog`](../../content/blog); that is not built yet. The posts live there.

It reads colour, type, and shape from [`design/tokens`](../../design/tokens) by relative path, so open it through the vault's preview server (`vault-static` in `.claude/launch.json`, then `/apps/site/`), not from disk. A deploy has to copy the tokens in.
