// How the kit's code is formatted and linted: check-kits.ts runs `vp check`
// here.
import { defineConfig } from "vite-plus";
import { fmt, lint } from "./vite-plus";

export default defineConfig({ fmt: fmt(), lint: lint() });
