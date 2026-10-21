import { cargoVersion } from "@code-villain/web/vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Settings end with it; the Rust workspace keeps the one version.
const version = cargoVersion(new URL("../../Cargo.toml", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  // The shared kit (`code/kit/web`) sits outside this app: it must use the
  // app's React and router, not a second copy beside the kit.
  resolve: { dedupe: ["react", "react-dom", "@base-ui/react", "@tanstack/react-router"] },
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
