import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { $ } from "bun";
import { backupAttachments } from "./backup";

const made: string[] = [];
afterEach(async () => {
  for (const dir of made.splice(0)) await rm(dir, { recursive: true, force: true });
});

async function folder(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "forge-backup-"));
  made.push(dir);
  return dir;
}

async function put(path: string, contents: string) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
}

test("backs up the attachments, not the repository clones, and unpacks into a data folder", async () => {
  const data = await folder();
  await put(join(data, "attachments/ws-1/a"), "first");
  await put(join(data, "attachments/ws-2/b"), "second");
  await put(join(data, "repos/villain/forge/README.md"), "a clone");
  const file = join(await folder(), "forge-attachments.tar.gz");

  expect(await backupAttachments(data, file)).toBe(2);
  const listed = (await $`tar -tzf ${file}`.text()).split("\n").filter(Boolean);
  expect(listed.some((entry) => entry.includes("repos"))).toBe(false);

  const restored = await folder();
  await $`tar -xzf ${file} -C ${restored}`.quiet();
  expect(await readFile(join(restored, "attachments/ws-1/a"), "utf8")).toBe("first");
  expect(await readFile(join(restored, "attachments/ws-2/b"), "utf8")).toBe("second");
});

test("without attachments there's nothing to save", async () => {
  const data = await folder();
  const file = join(await folder(), "forge-attachments.tar.gz");
  expect(await backupAttachments(data, file)).toBe(0);
  await mkdir(join(data, "attachments/ws-1"), { recursive: true });
  expect(await backupAttachments(data, file)).toBe(0);
  expect(await Bun.file(file).exists()).toBe(false);
});
