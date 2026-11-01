// How Raven's code is formatted and linted: `vp check` reads this, from here.
// The web app's own build settings are in apps/web/vite.config.ts.
import { defineConfig } from "vite-plus";
import { fmt, lint } from "../../kit/dev-id/vite-plus";

// Generated, copied from the vault, or sketches: never edited here.
const generated = [
  "packages/types/src/**",
  "apps/web/src/design/**",
  "**/src-tauri/gen/**",
  "storyboards/**",
];

export default defineConfig({
  fmt: fmt(generated),
  lint: { ...lint(), ignorePatterns: generated },
});
