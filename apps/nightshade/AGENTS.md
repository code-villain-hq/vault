# Nightshade editor themes: notes for coding agents

Nightshade for code editors: VS Code (and Cursor) themes in night and day, and a palette for Neovim. Run commands from this folder.

Everything here is generated from the vault's tokens: `design/tokens/syntax.resolved.json` for code and `color.resolved.json` for the editor's frame. Never edit `vscode/themes/*.json` or `nvim/lua/nightshade/palette.lua` by hand. Change the tokens (`design/tokens/syntax.json`, then `python3 build.py` there), then:

```sh
bun build.ts           # write the themes and the Neovim palette
bun build.ts --check   # write nothing; fail if an output has drifted (CI runs this)
bun install && bun specimen.ts   # draw the vault's own code in both modes, for review
```

`vscode/` is a complete extension (its `package.json` lists both themes). Cursor and VS Code load only extensions they installed themselves; a folder linked into `~/.cursor/extensions` is ignored. Package it and install the `.vsix`:

```sh
cd vscode && bunx --bun @vscode/vsce@3.6.0 package --allow-missing-repository --skip-license -o /tmp/nightshade.vsix
cursor --install-extension /tmp/nightshade.vsix   # or: code --install-extension
```

Then reload the window and pick "Code Villain Nightshade Night" or "Day" with ⌘K ⌘T. After a rebuild, raise `version` in `vscode/package.json` and install again.
