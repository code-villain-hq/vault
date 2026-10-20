// Saves a backup in the backup folder as two files with the same time in
// their names:
//   forge-<time>.dump                  the whole database, from the Postgres
//                                      container (`bun run db` or `bun run app`)
//   forge-<time>-attachments.tar.gz    the attached files, which live in the
//                                      data folder, not in the database
// The attachments come from the running app's forge-data volume when
// `bun run app` is up, and otherwise from the data folder `bun run dev` uses
// (FORGE_DATA_DIR, default ./data). Repository clones (data/repos) are left
// out: Forge clones them from GitHub again.
//
// The backup folder is FORGE_BACKUP_DIR, or else backup/forge next to the
// vault's checkout (~/code/villain/backup/forge for ~/code/villain/villain-vault),
// the same for every worktree. Never inside the repository: it is public.
//
// To restore one, stop the server, then (with <folder> the backup folder):
//   docker compose exec -T postgres pg_restore -U forge -d forge --clean --if-exists < <folder>/forge-<time>.dump
//   docker compose exec -T postgres psql -U forge -c "UPDATE workspaces SET sync_epoch = gen_random_uuid()"
//   tar -xzf <folder>/forge-<time>-attachments.tar.gz -C data
// The second command makes every client download a fresh copy instead of
// trusting what it saw after the backup was made. The third puts the files
// back in the data folder (use FORGE_DATA_DIR's folder if you set one). For
// `bun run app`, unpack them into its volume instead:
//   docker compose run --rm --no-deps -T forge tar -xzf - -C /var/lib/forge < <folder>/forge-<time>-attachments.tar.gz
import { mkdir, readdir, rm } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { $ } from "bun";

const root = new URL("..", import.meta.url).pathname;

/**
 * The folder backups go in: `FORGE_BACKUP_DIR`, or else `backup/forge` next
 * to the checkout `from` belongs to. A worktree counts as its main checkout,
 * so every worktree's backups end up together, outside the repository.
 */
export async function backupFolder(env = process.env, from = root): Promise<string> {
  const configured = env.FORGE_BACKUP_DIR?.trim();
  if (configured) {
    return configured.startsWith("~/")
      ? join(homedir(), configured.slice(2))
      : resolve(from, configured);
  }
  // The main checkout's .git, also from a worktree: <folder>/villain-vault/.git.
  const git = (
    await $`git rev-parse --path-format=absolute --git-common-dir`.cwd(from).text()
  ).trim();
  return join(dirname(dirname(git)), "backup", "forge");
}

/** Where the app container (`bun run app`) keeps its files. */
const CONTAINER_DATA_DIR = "/var/lib/forge";

/**
 * Packs `<dataDir>/attachments` into `file` (a .tar.gz that unpacks into the
 * data folder) and returns how many files it holds. Without any attachments
 * it writes nothing and returns 0.
 */
export async function backupAttachments(dataDir: string, file: string): Promise<number> {
  const entries = await readdir(join(dataDir, "attachments"), {
    recursive: true,
    withFileTypes: true,
  }).catch(() => []);
  const count = entries.filter((entry) => entry.isFile()).length;
  if (count === 0) return 0;
  // COPYFILE_DISABLE: macOS's tar would otherwise add "._" files for
  // extended attributes.
  const tar = await $`tar -czf ${file} -C ${dataDir} attachments`
    .env({ ...process.env, COPYFILE_DISABLE: "1" })
    .quiet()
    .nothrow();
  if (tar.exitCode !== 0) {
    await rm(file, { force: true });
    throw new Error(`Couldn't pack ${dataDir}/attachments:\n${tar.stderr.toString()}`);
  }
  return count;
}

/** The same, from the running app container's volume. */
async function backupContainerAttachments(file: string): Promise<number> {
  const countFiles = `find ${CONTAINER_DATA_DIR}/attachments -type f 2>/dev/null | wc -l`;
  const counted = await $`docker compose exec -T forge sh -c ${countFiles}`
    .cwd(root)
    .quiet()
    .nothrow();
  const count = Number.parseInt(counted.stdout.toString().trim(), 10);
  if (counted.exitCode !== 0 || Number.isNaN(count)) {
    throw new Error(`Couldn't look at the app's attachments:\n${counted.stderr.toString()}`);
  }
  if (count === 0) return 0;
  const tar = await $`docker compose exec -T forge tar -czf - -C ${CONTAINER_DATA_DIR} attachments`
    .cwd(root)
    .quiet()
    .nothrow();
  if (tar.exitCode !== 0) {
    throw new Error(`Couldn't pack the app's attachments:\n${tar.stderr.toString()}`);
  }
  await Bun.write(file, tar.stdout);
  return count;
}

/** Whether the whole app runs in Docker (`bun run app`). */
async function appContainerRunning(): Promise<boolean> {
  const ps = await $`docker compose ps --status running -q forge`.cwd(root).quiet().nothrow();
  return ps.exitCode === 0 && ps.stdout.toString().trim() !== "";
}

function size(bytes: number): string {
  return `${(bytes / 1024).toFixed(0)} KB`;
}

async function main() {
  const stamp = new Date().toISOString().slice(0, 19).replaceAll(":", "-");
  const folder = await backupFolder();
  const base = join(folder, `forge-${stamp}`);
  await mkdir(folder, { recursive: true });

  // The database first: attachments added while this runs are then extra
  // files in the backup rather than missing ones.
  const dump = await $`docker compose exec -T postgres pg_dump -U forge -Fc forge`
    .cwd(root)
    .quiet()
    .nothrow();
  if (dump.exitCode !== 0) {
    console.error(`Backup failed. Is Postgres running (bun run db)?\n${dump.stderr.toString()}`);
    process.exit(1);
  }
  await Bun.write(`${base}.dump`, dump.stdout);
  console.log(`Saved ${base}.dump (${size(dump.stdout.length)})`);

  const file = `${base}-attachments.tar.gz`;
  const inDocker = await appContainerRunning();
  const configured = process.env.FORGE_DATA_DIR?.trim() || "data";
  const dataDir = isAbsolute(configured) ? configured : join(root, configured);
  const from = inDocker ? "the app's forge-data volume" : dataDir;
  try {
    const count = inDocker
      ? await backupContainerAttachments(file)
      : await backupAttachments(dataDir, file);
    if (count === 0) {
      console.log(`No attachments in ${from}, so there's no attachments file.`);
    } else {
      const plural = count === 1 ? "" : "s";
      console.log(`Saved ${file} (${count} file${plural}, ${size(Bun.file(file).size)})`);
    }
  } catch (error) {
    console.error(
      `${error instanceof Error ? error.message : error}\nThe database is saved, the attachments aren't.`,
    );
    process.exit(1);
  }
}

if (import.meta.main) await main();
