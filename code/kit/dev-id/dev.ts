// What an app's `bun run dev` asks of dev-id: unless the developer has
// settings of their own (an `<app>.toml`, `<APP>_CONFIG`, or an issuer in
// `.env`), the app runs for Acme Inc, signed in by dev-id, on a database of
// its own (`<app>_acme`), so it never meets the developer's real data.

import { SQL } from "bun";

const MANIFEST = new URL("./Cargo.toml", import.meta.url).pathname;
const ISSUER = "http://127.0.0.1:7280";

export interface DevCompany {
	/** dev-id's command, when it isn't running yet. */
	devId: string[] | null;
}

/**
 * Sets the app's settings in `process.env` for dev-id, and makes its
 * database. `database` is the developer's own database URL, whose server
 * gets `<app>_acme` beside it. Null when the developer's settings are used.
 */
export async function devCompany(app: string, root: string, database: string) {
	const prefix = app.toUpperCase();
	const env = process.env;
	const own =
		(await Bun.file(`${root}/${app}.toml`).exists()) ||
		Boolean(env[`${prefix}_CONFIG`]) ||
		Boolean(env[`${prefix}_OIDC_ISSUER`]);
	if (own) return null;

	const url = new URL(database);
	const name = `${app}_acme`;
	await makeDatabase(url, name);
	url.pathname = `/${name}`;

	env.DATABASE_URL = url.toString();
	env[`${prefix}_COMPANY`] = "Acme Inc";
	env[`${prefix}_OIDC_ISSUER`] = ISSUER;
	env[`${prefix}_OIDC_CLIENT_ID`] = app;
	env[`${prefix}_OIDC_CLIENT_SECRET`] = "dev";
	env[`${prefix}_SCIM_TOKEN`] = "dev";
	const running = await fetch(`${ISSUER}/.well-known/openid-configuration`, {
		signal: AbortSignal.timeout(500),
	}).then(
		(r) => r.ok,
		() => false,
	);
	return {
		devId: running ? null : ["cargo", "run", "-q", "--manifest-path", MANIFEST],
	} satisfies DevCompany;
}

/** `name` on the developer's Postgres, made the first time. */
async function makeDatabase(url: URL, name: string) {
	const admin = new URL(url);
	admin.pathname = "/postgres";
	const db = new SQL(admin.toString());
	try {
		const [found] = await db`SELECT 1 FROM pg_database WHERE datname = ${name}`;
		if (!found) await db.unsafe(`CREATE DATABASE "${name}"`);
	} catch (e) {
		throw new Error(
			`Can't reach Postgres at ${url.host} (bun run db starts it): ${e}`,
		);
	} finally {
		await db.close();
	}
}

/**
 * Signs `who` (an id or email in people.toml) in to the app at `base`, as a
 * browser would through dev-id, for seeds. Returns the session cookie and
 * `/api/me`.
 */
export async function signInAs(base: string, who: string) {
	const jar = new Map<string, string>();
	const keep = (response: Response) => {
		for (const cookie of response.headers.getSetCookie()) {
			const [pair = ""] = cookie.split(";");
			const at = pair.indexOf("=");
			jar.set(pair.slice(0, at), pair.slice(at + 1));
		}
	};
	const cookie = () => [...jar].map(([k, v]) => `${k}=${v}`).join("; ");
	const step = async (url: string) => {
		const response = await fetch(url, {
			redirect: "manual",
			headers: { cookie: cookie() },
		});
		keep(response);
		const next = response.headers.get("location");
		if (!next)
			throw new Error(
				`signing in ${who} stopped at ${url} (${response.status})`,
			);
		return new URL(next, url).toString();
	};
	const authorize = new URL(await step(`${base}/api/auth/oidc`));
	authorize.searchParams.set("login_hint", who);
	const callback = await step(authorize.toString());
	const done = new URL(await step(callback));
	const problem = done.searchParams.get("error");
	if (problem) throw new Error(`couldn't sign in ${who}: ${problem}`);
	const me = await fetch(`${base}/api/me`, { headers: { cookie: cookie() } });
	if (!me.ok) throw new Error(`couldn't sign in ${who}: ${me.status}`);
	return { session: cookie(), me: await me.json() };
}
