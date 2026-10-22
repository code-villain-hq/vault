// Fills Acme Inc's workspace with the made-up villain strings in demo/, and
// puts Gus Guest in its first language after the source. Its people come
// from dev-id (code/kit/dev-id/people.toml). Safe to run again: keys that
// are there already are left alone. Needs `bun run dev` running, for Acme
// Inc. Local development only.
import { readdir } from "node:fs/promises";
import { signInAs } from "../../../kit/dev-id/dev";

export const BASE = process.env.GLYPH_URL ?? "http://localhost:5174";

/** dev-id's admin. */
export const DEV_ADMIN = "ada";

const demo = new URL("../demo/", import.meta.url).pathname;
/** Well under the server's request size limit. */
const BATCH = 500;

type Operation =
  | { action: "create"; model: string; id: string; data: Record<string, unknown> }
  | { action: "update"; model: string; id: string; data: Record<string, unknown> };

export async function call(path: string, body: unknown, cookie?: string, method = "POST") {
  const response = await fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", ...(cookie ? { cookie } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: response.status, body: await response.json().catch(() => null) };
}

/** Signs someone in through dev-id: their session cookie, `/api/me` and workspace. */
export async function account(who: string) {
  const { session, me } = await signInAs(BASE, who);
  const workspace = me.workspaces[0];
  if (!workspace) throw new Error(`${who} isn't in Acme Inc's workspace yet: is dev-id running?`);
  return { session, me, workspace };
}

/** Waits up to ten seconds, two of dev-id's rounds, for `check` to find something. */
async function until<T>(what: string, check: () => Promise<T | undefined>) {
  for (let i = 0; i < 20; i++) {
    const found = await check();
    if (found) return found;
    await Bun.sleep(500);
  }
  throw new Error(`gave up waiting for ${what}`);
}

type Meta = { confidence?: number; translated_by?: string; context?: string };

/** The demo strings as rows: every language folder, every namespace file. */
async function demoTranslations() {
  const meta: Record<string, Record<string, Record<string, Meta>>> = (
    await Bun.file(`${demo}translations/translations.meta.json`).json()
  ).translations;
  const rows: Record<string, unknown>[] = [];
  const langs = (await readdir(`${demo}translations`, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  for (const lang of langs) {
    for (const file of (await readdir(`${demo}translations/${lang}`)).sort()) {
      const namespace = file.replace(/\.json$/, "");
      const strings: Record<string, string> = await Bun.file(
        `${demo}translations/${lang}/${file}`,
      ).json();
      for (const [key, value] of Object.entries(strings)) {
        const about = meta[lang]?.[namespace]?.[key] ?? {};
        rows.push({
          lang,
          namespace,
          key,
          value,
          confidence: about.confidence ?? null,
          translated_by: about.translated_by ?? "human",
          context: about.context ?? "",
        });
      }
    }
  }
  return { langs, rows };
}

async function send(workspace: string, cookie: string, operations: Operation[]) {
  for (let start = 0; start < operations.length; start += BATCH) {
    const result = await call(
      `/api/w/${workspace}/sync/transactions`,
      { id: Bun.randomUUIDv7(), operations: operations.slice(start, start + BATCH) },
      cookie,
    );
    if (result.status !== 200) throw new Error(`seeding failed: ${JSON.stringify(result.body)}`);
  }
}

const create = (model: string, data: Record<string, unknown>): Operation => ({
  action: "create",
  model,
  id: Bun.randomUUIDv7(),
  data,
});

if (import.meta.main) {
  const admin = await until("Ada", () => account(DEV_ADMIN).catch(() => undefined));
  const { workspace } = admin;
  const cookie = admin.session;
  const bootstrap = async () =>
    (await call(`/api/w/${workspace.id}/sync/bootstrap`, undefined, cookie, "GET")).body.records;
  // The source language comes with the workspace's first person.
  const current = await until("the source language", async () => {
    const records = await bootstrap();
    return records.Language?.length ? records : undefined;
  });
  const have = (model: string, key: (row: Record<string, unknown>) => string) =>
    new Set<string>((current[model] ?? []).map(key));

  const { langs, rows } = await demoTranslations();
  const languages = have("Language", (l) => String(l.code));
  const pairs = have("LanguagePair", (p) => `${p.source_lang}_${p.target_lang}`);
  const keys = have("Translation", (t) => `${t.lang}/${t.namespace}/${t.key}`);
  const terms = have("GlossaryEntry", (g) =>
    `${g.source_lang}_${g.target_lang}/${g.source_term}`.toLowerCase(),
  );
  const glossary: Record<string, unknown>[] = await Bun.file(`${demo}glossary.json`).json();

  const operations: Operation[] = [
    ...langs.filter((code) => !languages.has(code)).map((code) => create("Language", { code })),
    ...langs
      .filter((code) => code !== "en" && !pairs.has(`en_${code}`))
      .map((code) => create("LanguagePair", { source_lang: "en", target_lang: code })),
    ...rows
      .filter((row) => !keys.has(`${row.lang}/${row.namespace}/${row.key}`))
      .map((row) => create("Translation", row)),
    ...glossary
      .filter((entry) => {
        const id = `${entry.source_lang}_${entry.target_lang}/${entry.source_term}`;
        return !terms.has(id.toLowerCase());
      })
      .map((entry) => create("GlossaryEntry", entry)),
  ];
  await send(workspace.id, cookie, operations);

  const members: { user: { id: string; email: string } }[] = (
    await call(`/api/w/${workspace.id}/members`, undefined, cookie, "GET")
  ).body;
  const guest = members.find((m) => m.user.email === "gus@acme.test");
  if (!guest) throw new Error("Gus isn't in the workspace: check people.toml");
  const after = await bootstrap();
  const language = (after.Language ?? []).find((l: { is_source: boolean }) => !l.is_source);
  if (language) {
    const access = await call(
      `/api/w/${workspace.id}/members/${guest.user.id}/access`,
      { items: [language.id] },
      cookie,
      "PUT",
    );
    if (access.status !== 204) throw new Error(`couldn't add Gus to ${language.code}`);
  }

  console.log(
    `Seeded "${workspace.name}": ${operations.length} new records, ` +
      `and Gus Guest in ${language?.code ?? "no language"}.`,
  );
  console.log(`Open ${BASE}/${workspace.url_key} and sign in as anyone in people.toml.`);
}
