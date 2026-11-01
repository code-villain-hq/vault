// The app's build, and how all of Layer's code is formatted and linted:
// `vp check` reads this, from here. The web app's build is in web/vite.config.ts.
import process from "node:process";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite-plus";
import { fmt, lint } from "../../kit/dev-id/vite-plus";
const host = process.env.TAURI_DEV_HOST;

// Generated, or copied from the vault: never edited here.
const generated = ["packages/types/src/**", "src/design/**", "**/src-tauri/gen/**"];
const rules = lint(
  "Confirm from src/components/ui.tsx, or in web/ ConfirmDialog from @code-villain/web",
);

// https://vite.dev/config/
export default defineConfig({
  fmt: fmt(generated),
  lint: {
    ...rules,
    rules: {
      ...rules.rules,
      // Rows, pane bars and chips take clicks on divs and spans, which the
      // keyboard can't reach; the app's keymap covers the common ones. Each
      // becomes a control in a change of its own, then these come back on.
      "jsx-a11y/no-static-element-interactions": "off",
      "jsx-a11y/click-events-have-key-events": "off",
    },
    ignorePatterns: generated,
  },
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // The window's page, and the one the app serves to a paired phone
  // (src-tauri/src/phone). mock.html and phone-mock.html stay dev-only.
  build: {
    rollupOptions: {
      input: { main: "index.html", phone: "phone.html" },
    },
  },
});
