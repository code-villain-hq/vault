# Villain Vault: notes for coding agents

The vault holds the whole studio. Apps are one part of it.

- `brand/`: decisions about look and voice. Change these when the feeling should change. A new palette or a new tone starts here.
- `design/`: tokens and reference layouts taken from `brand/`. Apps implement them in their own toolkit. Caster is AppKit; Forge, Layer, and the site are web. They share the decisions.
- `content/`: blog posts and social posts. The site renders the blog. The posts live in `content/blog`.
- `apps/<app>/`: one product. Read `apps/<app>/AGENTS.md` before working there, and run commands from that folder.

An app can have its own UI theme. Layer's theme picker is one. The studio surfaces — the mark, the site, the blog, social — follow `brand/`.

A change to how Villain feels starts in `brand/` or `design/`, then the apps catch up. A change to one product's behavior stays in that app.
