// How Glyph's code is formatted and linted: `vp check` reads this, from here.
// The web app's own build settings are in apps/web/vite.config.ts.
import { defineConfig } from "vite-plus";
import { fmt, lint } from "../../kit/dev-id/vite-plus";

// Generated, or copied from the vault: never edited here.
const generated = ["packages/types/src/**", "apps/web/src/design/**", "**/src-tauri/gen/**"];

export default defineConfig({
  fmt: fmt(generated),
  lint: { ...lint(), ignorePatterns: generated },
});
