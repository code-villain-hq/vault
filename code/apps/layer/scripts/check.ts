// The verification gate: everything here must pass before a change is done.
// The server's tests need Postgres (`bun run db`).
import { $ } from "bun";
import { assertTypesCurrent, runChecks } from "../../../kit/dev-id/check";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which the web app builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// The parts every studio web app shares (`code/kit/web`).
const kitWeb = `${root}../../kit/web`;
// The dev tooling every app's scripts share (`code/kit/dev-id`).
const kitDevId = `${root}../../kit/dev-id`;
const biome = `${root}web/node_modules/.bin/biome`;

await runChecks([
  ["guard", () => $`bun scripts/guard.ts`.cwd(root)],
  ["design copies are current", () => $`bun scripts/sync-design.ts --check`.cwd(root)],
  ["bun test (app)", () => $`bun test scripts`.cwd(root)],
  ["tsc (app)", () => $`bunx tsc`.cwd(root)],
  ["tsc (scripts)", () => $`bunx tsc -p scripts`.cwd(root)],
  ["app build", () => $`bunx vite build`.cwd(root).quiet()],
  ["cargo fmt", () => $`cargo fmt --all --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy --workspace --all-targets -- -D warnings`.cwd(root)],
  ["cargo test (app)", () => $`cargo test -p villain-layer --lib --quiet`.cwd(root)],
  ["cargo test (server)", () => $`cargo test -p layer-domain -p layer-sync -p layer-server --quiet`.cwd(root)],
  [
    "generated types are current",
    () =>
      assertTypesCurrent(`${root}packages/types/src`, () =>
        $`bun scripts/gen-types.ts`.cwd(root).quiet(),
      ),
  ],
  ["biome (web)", () => $`${biome} check .`.cwd(`${root}web`)],
  // The kit's client, with its own settings: Biome checks only under its root.
  ["biome (sync kit)", () => $`${biome} check .`.cwd(kitClient)],
  ["biome (web kit)", () => $`${biome} check .`.cwd(kitWeb)],
  ["biome (dev-id kit)", () => $`${biome} check .`.cwd(kitDevId)],
  [
    "tsc (web)",
    async () => {
      await $`bunx tsc -p web --noEmit`.cwd(root);
      await $`bunx tsc -p web/tsconfig.test.json --noEmit`.cwd(root);
    },
  ],
  ["tsc (web kit)", () => $`bunx tsc -p ${kitWeb} --noEmit`.cwd(root)],
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
