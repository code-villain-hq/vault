import { readFileSync } from "node:fs";
import { cargoVersion } from "@code-villain/web/vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite-plus";

// Settings end with it; the Rust workspace keeps the one version.
const version = cargoVersion(new URL("../../Cargo.toml", import.meta.url));

// Postrider runs only on this Mac, where nobody signs in: the dev server
// writes its one session here (`scripts/dev.ts`), and the proxy signs the
// browser's requests with it.
const sessionFile = new URL("../../.dev-session", import.meta.url);
const session = () => {
  try {
    return readFileSync(sessionFile, "utf8").trim();
  } catch {
    return "";
  }
};

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  // The shared kit (`code/kit/web`) sits outside this app: it must use the
  // app's React and router, not a second copy beside the kit.
  resolve: {
    dedupe: ["react", "react-dom", "@base-ui/react", "@tanstack/react-router", "lucide-react"],
  },
  server: {
    // Forge's dev server has 5173, Glyph's 5174, Raven's 5175, Almanac's 5177.
    port: 5178,
    strictPort: true,
    // The server answers CORS itself, so the packaged app can use this port.
    cors: false,
    // The Rust server, so the app and the API share an origin in development.
    proxy: {
      "/api": {
        target: "http://127.0.0.1:7880",
        ws: true,
        configure: (proxy) => {
          const sign = (request: { setHeader(name: string, value: string): void }) => {
            const token = session();
            if (token) request.setHeader("authorization", `Bearer ${token}`);
          };
          proxy.on("proxyReq", sign);
          proxy.on("proxyReqWs", sign);
        },
      },
    },
  },
});
