// How the theme's scripts are formatted and linted: `vp check` reads this.
import { defineConfig } from "vite-plus";
import { fmt, lint } from "../../kit/dev-id/vite-plus";

// Written by build.ts from the vault's tokens: never edited here.
const generated = [
  "vscode/themes/**",
  "thunderbird/manifest.json",
  "nvim/lua/code-villain/palette.lua",
  "wezterm/**",
];

export default defineConfig({
  fmt: fmt(generated),
  lint: { ...lint(), ignorePatterns: generated },
});
