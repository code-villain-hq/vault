import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // What the Layer server serves (LAYER_WEB_DIR, by default dist-web).
  build: { outDir: "../dist-web", emptyOutDir: true },
  server: {
    // Forge's dev server has 5173, Glyph's 5174, Raven's 5175. The
    // desktop app's own Vite has 1420.
    port: 5176,
    strictPort: true,
    // The server answers CORS itself.
    cors: false,
    // The Rust server, so the app and the API share an origin in development.
    proxy: {
      "/api": { target: "http://127.0.0.1:7680", ws: true },
    },
  },
});
