# Code Villain Vault

Code Villain's home: the apps, the look, the words, and the record of building them.

Code Villain builds real tools and shows the build in public. Someone looking in, including a future employer, should see the craft and the growth from senior front-end work into building whole products with AI. The name of that role can change. Product engineer is the current one.

| Place | What it holds |
| --- | --- |
| [`studio/`](studio) | The studio itself, no code to build |
| [`studio/brand/`](studio/brand) | How Code Villain looks and sounds: color, type, mark, voice |
| [`studio/design/`](studio/design) | Those decisions as tokens and reference layouts |
| [`studio/content/`](studio/content) | Blog posts and social posts |
| [`code/`](code) | Everything that builds |
| [`code/apps/`](code/apps) | The products |

| App | What it is |
| --- | --- |
| [`code/apps/forge`](code/apps/forge) | Forge, the AI-first issue tracker |
| [`code/apps/layer`](code/apps/layer) | Layer, the agent development environment for macOS |
| [`code/apps/caster`](code/apps/caster) | Caster, the launcher for macOS |
| [`code/apps/glyph`](code/apps/glyph) | Glyph, the AI translation tool for i18n JSON files |
| [`code/apps/raven`](code/apps/raven) | Raven, which watches the studio's apps |
| [`code/apps/theme`](code/apps/theme) | The Code Villain theme for VS Code, Cursor, Neovim, WezTerm and Starship |
| [`code/apps/site`](code/apps/site) | The website. It publishes [`studio/content/blog`](studio/content/blog) |

The studio website is [www.codevillain.eu](https://www.codevillain.eu/). Naming rules live in [`studio/brand/name.md`](studio/brand/name.md).

Each app keeps its own tooling. Run its commands from its own folder.

## Contributing

The vault lives on Forgejo at [git.codevillain.eu/code-villain/vault](https://git.codevillain.eu/code-villain/vault). [github.com/code-villain-hq/vault](https://github.com/code-villain-hq/vault) mirrors it, and pull requests are welcome there. Each one is reviewed and merged on Forgejo with its commits and authors unchanged. The GitHub pull request then shows as merged. Read the app's `AGENTS.md` before working in it. `git config core.hooksPath .githooks`, once per clone, runs each app's pre-commit check when a commit touches it.

Maintainers bring a GitHub pull request across with [`scripts/take-pr`](scripts/take-pr).
