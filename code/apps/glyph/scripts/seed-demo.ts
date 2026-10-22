// Creates local test accounts through the API and fills their workspace with
// the made-up villain strings in demo/: an admin with a "Villain Corp"
// workspace, a member who joined it by invite link, and a guest who
// translates its first language after the source. Safe to run again:
// keys that are there already are left alone. Needs the server running
// (`bun run dev`). Local development only.
import { readdir } from "node:fs/promises";

export const BASE = process.env.GLYPH_URL ?? "http://localhost:7480";

/** Test-only password for the local seed accounts. */
export const DEV_PASSWORD = "glyph-dev-password";
export const DEV_ADMIN = { name: "Ada Admin", email: "admin@glyph.test" };
export const DEV_MEMBER = { name: "Max Member", email: "member@glyph.test" };
export const DEV_GUEST = { name: "Gus Guest", email: "guest@glyph.test" };

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
  const session = response.headers.get("set-cookie")?.split(";")[0];
  return { status: response.status, body: await response.json().catch(() => null), session };
}

/** Signs up, or signs in if the account exists. Returns the session cookie and workspaces. */
export async function account(
  person: { name: string; email: string },
  extra: Record<string, string>,
) {
  const signup = await call("/api/auth/signup", { ...person, password: DEV_PASSWORD, ...extra });
  if (signup.status === 200) return signup;
  const login = await call("/api/auth/login", { email: person.email, password: DEV_PASSWORD });
  if (login.status !== 200)
    throw new Error(`couldn't sign in ${person.email}: ${JSON.stringify(login.body)}`);
  return login;
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
  const admin = await account(DEV_ADMIN, { workspace_name: "Villain Corp" });
  const workspace = admin.body.workspaces.find((w: { name: string }) => w.name === "Villain Corp");
  const cookie = admin.session as string;
  const current = (await call(`/api/w/${workspace.id}/sync/bootstrap`, undefined, cookie, "GET"))
    .body.records;
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

  const invite = await call(`/api/w/${workspace.id}/invites`, { role: "member" }, cookie);
  await account(DEV_MEMBER, { invite_token: invite.body.token });

  const guestInvite = await call(`/api/w/${workspace.id}/invites`, { role: "guest" }, cookie);
  const guest = await account(DEV_GUEST, { invite_token: guestInvite.body.token });
  const after = (await call(`/api/w/${workspace.id}/sync/bootstrap`, undefined, cookie, "GET")).body
    .records;
  const language = (after.Language ?? []).find((l: { is_source: boolean }) => !l.is_source);
  if (language) {
    const access = await call(
      `/api/w/${workspace.id}/members/${guest.body.user.id}/access`,
      { items: [language.id] },
      cookie,
      "PUT",
    );
    if (access.status !== 204) throw new Error(`couldn't add the guest to ${language.code}`);
  }

  console.log(
    `Seeded ${DEV_ADMIN.email} (admin), ${DEV_MEMBER.email} (member) and ${DEV_GUEST.email} ` +
      `(guest in ${language?.code ?? "no language"}) in "${workspace.name}": ` +
      `${operations.length} new records.`,
  );
  console.log(
    `Password for all three: see DEV_PASSWORD in scripts/seed-demo.ts. Open http://localhost:5174/${workspace.url_key}`,
  );
}
