import { expect, test } from "bun:test";
import { readdirSync } from "node:fs";

// The server runs migrations by number and refuses to start if two share one
// (FOR-34: two branches each added a 0021). Renumber the newer, unmerged one.
test("every migration has its own number", () => {
  const dir = new URL("../migrations", import.meta.url).pathname;
  const byNumber = new Map<string, string[]>();
  for (const file of readdirSync(dir).filter((f) => f.endsWith(".sql"))) {
    const number = file.split("_")[0] ?? file;
    byNumber.set(number, [...(byNumber.get(number) ?? []), file]);
  }
  const shared = [...byNumber.values()].filter((files) => files.length > 1);
  expect(shared).toEqual([]);
});
