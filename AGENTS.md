# Code Villain Vault: notes for coding agents

The vault holds the whole studio. Apps are one part of it.

- `brand/`: decisions about look and voice. Change these when the feeling should change. A new palette or a new tone starts here.
- `design/`: principles, tokens, and reference layouts taken from `brand/`. Apps implement them in their own toolkit. Caster is AppKit; Forge, Layer, and the site are web; the Code Villain theme (`apps/theme`) dresses VS Code, Cursor, Neovim and WezTerm; Schemer is in planning. They share the decisions. Read `design/principles.md` before any UI change, in any app.
- `content/`: blog posts and social posts. The site renders the blog. The posts live in `content/blog`.
- `apps/<app>/`: one product. Read `apps/<app>/AGENTS.md` before working there, and run commands from that folder.

Nightshade is every app's default theme, in night and day. An app's theme picker, like Layer's, can add themes that fill the same colour names. The studio surfaces — the mark, the site, the blog, social — follow `brand/` and stay night.

A change to how Code Villain feels starts in `brand/` or `design/`, then the apps catch up. A change to one product's behavior stays in that app.
