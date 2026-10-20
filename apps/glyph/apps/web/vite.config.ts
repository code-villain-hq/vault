import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // Forge's dev server has 5173.
    port: 5174,
    strictPort: true,
    // The server answers CORS itself, so the packaged app can use this port.
    cors: false,
    // The Rust server, so the app and the API share an origin in development.
    proxy: {
      "/api": { target: "http://127.0.0.1:7480", ws: true },
    },
  },
});
