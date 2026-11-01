// The Rust kits' gate: fmt, clippy and tests for each. A kit is a package of
// its own, outside every app's Cargo workspace, so an app's check formats
// it but never lints or tests it. CI runs this on every push to code/kit
// (`.forgejo/workflows/kit.yml`).
import { $ } from "bun";
import { type CheckStep, runChecks } from "./check";

const kitRoot = new URL("..", import.meta.url).pathname;
const kits = ["auth", "desktop", "dev-id", "scim", "settings", "sync"];

await runChecks(
  kits.flatMap((kit): CheckStep[] => {
    const dir = `${kitRoot}${kit}`;
    return [
      [`cargo fmt (${kit})`, () => $`cargo fmt --check`.cwd(dir)],
      [`cargo clippy (${kit})`, () => $`cargo clippy --all-targets -- -D warnings`.cwd(dir)],
      [`cargo test (${kit})`, () => $`cargo test --quiet`.cwd(dir)],
    ];
  }),
);
