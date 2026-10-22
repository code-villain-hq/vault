// A local test account, for development only. Needs `bun run dev:server`
// running, with passwords on (LAYER_OIDC_ISSUER empty), and no account yet:
// Layer takes a password sign-up for its first account only, which sets it
// up with a workspace. Everyone after joins by invite.
// Then it adds a repository with a note, and GitHub, so the pages have
// something on them, and invites Gus Guest, a guest in that repository.

const server = process.env.LAYER_URL ?? "http://127.0.0.1:7680";
export const account = {
	name: "Local Villain",
	email: "villain@layer.test",
	password: "layer-local-only",
	workspace_name: "Code Villain",
};

const signup = await fetch(`${server}/api/auth/signup`, {
	method: "POST",
	headers: { "content-type": "application/json" },
	body: JSON.stringify(account),
});
if (signup.status === 403) {
	console.log(
		`Layer has an account already; sign in as ${account.email} if it's this one.`,
	);
	process.exit(0);
}
if (!signup.ok) {
	console.error(`Couldn't sign up: ${signup.status} ${await signup.text()}`);
	process.exit(1);
}
console.log(`Signed up ${account.email} (password in scripts/seed.ts).`);

const cookie = signup.headers.get("set-cookie")?.split(";")[0] ?? "";
const me = (await (
	await fetch(`${server}/api/me`, { headers: { cookie } })
).json()) as {
	workspaces: { id: string }[];
};
const workspace = me.workspaces[0]?.id;
const w = `/api/w/${workspace}`;
const create = (model: string, data: object) => ({
	action: "create",
	model,
	id: Bun.randomUUIDv7(),
	data,
});
const written = await fetch(`${server}${w}/sync/transactions`, {
	method: "POST",
	headers: { "content-type": "application/json", cookie },
	body: JSON.stringify({
		id: Bun.randomUUIDv7(),
		operations: [
			create("Repository", {
				remote_key: "git@git.codevillain.eu:code-villain/vault.git",
				name: "vault",
				default_branch: "main",
				group_name: "studio",
				check_command: "bun run check",
			}),
			create("Note", {
				repo: "git.codevillain.eu/code-villain/vault",
				text: "Each app's commands run from its own folder under code/apps.",
				paths: ["code/apps"],
				source: "seed",
			}),
			create("Connection", {
				kind: "github",
				settings: {
					api_url: "https://api.github.com",
					web_url: "https://github.com",
				},
			}),
		],
	}),
});
if (!written.ok) {
	console.error(
		`Couldn't add the examples: ${written.status} ${await written.text()}`,
	);
	process.exit(1);
}
console.log("Added a repository with a note, and GitHub.");

// A guest in the repository, for trying what a guest sees.
const invite = (await (
	await fetch(`${server}${w}/invites`, {
		method: "POST",
		headers: { "content-type": "application/json", cookie },
		body: JSON.stringify({ role: "guest" }),
	})
).json()) as { token: string };
const gus = {
	name: "Gus Guest",
	email: "guest@layer.test",
	password: account.password,
};
let joined = await fetch(`${server}/api/auth/signup`, {
	method: "POST",
	headers: { "content-type": "application/json" },
	body: JSON.stringify({ ...gus, invite_token: invite.token }),
});
if (!joined.ok)
	joined = await fetch(`${server}/api/auth/login`, {
		method: "POST",
		headers: { "content-type": "application/json" },
		body: JSON.stringify({ email: gus.email, password: gus.password }),
	});
const guest = (await joined.json()) as { user?: { id: string } };
const snapshot = (await (
	await fetch(`${server}${w}/sync/bootstrap`, { headers: { cookie } })
).json()) as { records: { Repository?: { id: string; name: string }[] } };
const first = snapshot.records.Repository?.[0];
const added =
	guest.user &&
	first &&
	(
		await fetch(`${server}${w}/members/${guest.user.id}/access`, {
			method: "PUT",
			headers: { "content-type": "application/json", cookie },
			body: JSON.stringify({ items: [first.id] }),
		})
	).ok;
console.log(
	added
		? `${gus.email} is a guest in ${first.name} (same password).`
		: `Couldn't add ${gus.email} as a guest.`,
);
