# Code Villain theme: notes for coding agents

The studio's editor and terminal theme, in night and day: a VS Code (and Cursor) extension, a Neovim colorscheme, WezTerm colour schemes, and a starship prompt. Its colours are the Nightshade palette for code. Run commands from this folder.

The colours are generated from the vault's tokens: `design/tokens/syntax.resolved.json` for code and `color.resolved.json` for the editor's frame. Never edit `vscode/themes/*.json`, `nvim/lua/code-villain/palette.lua` or `wezterm/*.toml` by hand. To change colours or which scopes get which colour, edit `design/tokens/syntax.json`, run `python3 build.py` there, then:

```sh
bun build.ts           # write the VS Code themes, the Neovim palette and the WezTerm schemes
bun build.ts --check   # write nothing; fail if an output has drifted (CI runs this)
bun install && bun specimen.ts   # draw the vault's own code in both modes, for review
```

`nvim/lua/code-villain/init.lua` is written by hand. It maps Neovim's highlight groups (editor, legacy syntax, Treesitter, LSP semantic tokens, diagnostics, and plugins) onto the palette's roles, following tokyonight's choices of what gets which kind of colour. `background` picks the mode: dark is night, light is day.

Every terminal takes one 16-colour table, `ansi` in `design/tokens/syntax.resolved.json`: WezTerm, the VS Code and Cursor terminal, Neovim's `:terminal`, and Layer. Its hues are the syntax hues at a calmer chroma, and its green is poison.

## Installing

Neovim, with lazy.nvim:

```lua
{ dir = "~/code/villain/villain-vault/apps/theme/nvim", name = "code-villain", lazy = false, priority = 1000,
  config = function() vim.cmd.colorscheme("code-villain") end }
```

Cursor and VS Code load only extensions they installed themselves; a folder linked into `~/.cursor/extensions` is ignored. Package `vscode/` and install the `.vsix`:

```sh
cd vscode && bunx --bun @vscode/vsce@3.6.0 package --allow-missing-repository --skip-license -o /tmp/code-villain.vsix
cursor --install-extension /tmp/code-villain.vsix   # or: code --install-extension
```

WezTerm: point `color_scheme_dirs` at this folder's `wezterm/` and pick the scheme by the system appearance:

```lua
config.color_scheme_dirs = { wezterm.home_dir .. "/code/villain/villain-vault/apps/theme/wezterm" }
config.color_scheme = wezterm.gui.get_appearance():find("Dark") and "Code Villain Night" or "Code Villain Day"
```

The shell prompt: `starship/code-villain.toml` is a starship theme (zephyr's layout) written only in ANSI colour names, so the terminal's scheme colours it in night and day, and green is poison. It is written by hand, since it has no colours of its own to generate. Link it as `~/.config/starship/code-villain.toml`, then with zephyr's prompt plugin set `zstyle ':zephyr:plugin:prompt' theme starship code-villain` (or point `STARSHIP_CONFIG` at it).

For Cursor, reload the window and pick "Code Villain Night" or "Code Villain Day" with ⌘K ⌘T. After a rebuild, raise `version` in `vscode/package.json` and install again.
