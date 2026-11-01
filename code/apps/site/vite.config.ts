// How the site's script and CSS are formatted and linted: `vp check` reads
// this. Ruff does the Python (the vault's ruff.toml).
import { defineConfig } from "vite-plus";
import { fmt, lint } from "../../kit/dev-id/vite-plus";

// The HTML is a template or checked against one (test_products.py), and
// build.py writes the pages, the blog and dist/.
const html = ["**/*.html"];

export default defineConfig({ fmt: fmt(html), lint: lint() });
