import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // Forge's dev server has 5173, Glyph's 5174.
    port: 5175,
    strictPort: true,
    // The Rust server, so the app and the API share an origin in development.
    proxy: {
      "/api": { target: "http://127.0.0.1:7580" },
    },
  },
});
