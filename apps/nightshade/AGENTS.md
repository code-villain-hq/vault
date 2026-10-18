# Nightshade editor themes: notes for coding agents

Nightshade for code editors: VS Code (and Cursor) themes in night and day, and a palette for Neovim. Run commands from this folder.

Everything here is generated from the vault's tokens: `design/tokens/syntax.resolved.json` for code and `color.resolved.json` for the editor's frame. Never edit `vscode/themes/*.json` or `nvim/lua/nightshade/palette.lua` by hand. Change the tokens (`design/tokens/syntax.json`, then `python3 build.py` there), then:

```sh
bun build.ts           # write the themes and the Neovim palette
bun build.ts --check   # write nothing; fail if an output has drifted (CI runs this)
bun install && bun specimen.ts   # draw the vault's own code in both modes, for review
```

`vscode/` is a complete extension (its `package.json` lists both themes). To try it in Cursor or VS Code without packaging, link it into the editor's extensions folder and reload the window:

```sh
ln -sfn "$PWD/vscode" ~/.cursor/extensions/code-villain.nightshade-0.1.0
```
