// The verification gate: everything here must pass before a change is done.
// The server's tests need Postgres (`bun run db`).
import { $ } from "bun";
import { assertTypesCurrent, runChecks } from "../../../kit/dev-id/check";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which the web app builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// The parts every studio web app shares (`code/kit/web`).
const kitWeb = `${root}../../kit/web`;
const vp = `${root}node_modules/.bin/vp`;

await runChecks([
  ["guard", () => $`bun scripts/guard.ts`.cwd(root)],
  ["design copies are current", () => $`bun scripts/sync-design.ts --check`.cwd(root)],
  ["bun test (app)", () => $`bun test scripts`.cwd(root)],
  // Oxfmt, Oxlint and TypeScript's own errors, for the app, its scripts and
  // the web app alike, as vite.config.ts says.
  ["vp check", () => $`${vp} check`.cwd(root)],
  // The kits, each with its own vite.config.ts: Vite+ checks only under
  // the folder it runs in.
  ["vp check (sync kit)", () => $`${vp} check`.cwd(kitClient)],
  ["vp check (web kit)", () => $`${vp} check`.cwd(kitWeb)],
  ["app build", () => $`${vp} build`.cwd(root).quiet()],
  ["cargo fmt", () => $`cargo fmt --all --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy --workspace --all-targets -- -D warnings`.cwd(root)],
  ["cargo test (app)", () => $`cargo test -p villain-layer --lib --quiet`.cwd(root)],
  [
    "cargo test (server)",
    () => $`cargo test -p layer-domain -p layer-sync -p layer-server --quiet`.cwd(root),
  ],
  [
    "generated types are current",
    () =>
      assertTypesCurrent(`${root}packages/types/src`, () =>
        $`bun scripts/gen-types.ts`.cwd(root).quiet(),
      ),
  ],
  [
    "bun test (web, sync kit)",
    async () => {
      await $`bun test src`.cwd(`${root}web`);
      await $`bun test`.cwd(kitClient);
      await $`bun test`.cwd(kitWeb);
    },
  ],
  ["web build", () => $`bun run --cwd web build`.cwd(root).quiet()],
]);
