// The kits' gate: fmt, clippy and tests for each Rust kit, and `vp check`
// for this kit's TypeScript. A Rust kit is a package of its own, outside
// every app's Cargo workspace, so an app's check formats it but never lints
// or tests it; and no app's check reaches this kit's scripts. (The web and
// sync kits' TypeScript is checked by every app that builds on them.) CI
// runs this on every push to code/kit (`.forgejo/workflows/kit.yml`).
import { $ } from "bun";
import { type CheckStep, runChecks } from "./check";

const kitRoot = new URL("..", import.meta.url).pathname;
const kits = ["auth", "claude", "desktop", "dev-id", "notify", "scim", "settings", "sync"];

const devId = `${kitRoot}dev-id`;

await runChecks([
  ...kits.flatMap((kit): CheckStep[] => {
    const dir = `${kitRoot}${kit}`;
    return [
      [`cargo fmt (${kit})`, () => $`cargo fmt --check`.cwd(dir)],
      [`cargo clippy (${kit})`, () => $`cargo clippy --all-targets -- -D warnings`.cwd(dir)],
      [`cargo test (${kit})`, () => $`cargo test --quiet`.cwd(dir)],
    ];
  }),
  // Oxfmt, Oxlint and TypeScript's own errors, as its vite.config.ts says.
  [
    "vp check (dev-id)",
    async () => {
      await $`bun install --frozen-lockfile`.cwd(devId).quiet();
      await $`node_modules/.bin/vp check`.cwd(devId);
    },
  ],
]);
