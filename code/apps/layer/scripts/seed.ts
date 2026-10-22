// Readies Acme Inc's workspace for trying Layer, for development only. Its
// people come from dev-id (code/kit/dev-id/people.toml); this signs in as
// Ada Admin, adds a repository with a note, and GitHub, so the pages have
// something on them, and puts Gus Guest in that repository. Needs
// `bun run dev:server` running, for Acme Inc.

import { signInAs } from "../../../kit/dev-id/dev";

const server = process.env.LAYER_URL ?? "http://localhost:5176";

/** Waits up to ten seconds, two of dev-id's rounds, for `check` to find something. */
async function until<T>(what: string, check: () => Promise<T | undefined>) {
	for (let i = 0; i < 20; i++) {
		const found = await check();
		if (found) return found;
		await Bun.sleep(500);
	}
	throw new Error(`gave up waiting for ${what}`);
}

const ada = await until("Ada", () =>
	signInAs(server, "ada").then(
		(signed) => (signed.me.workspaces[0] ? signed : undefined),
		() => undefined,
	),
);
const cookie = ada.session;
const workspace = ada.me.workspaces[0] as { id: string; name: string };
console.log(`Signed in as ${ada.me.user.email}, in ${workspace.name}.`);

const w = `/api/w/${workspace.id}`;
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

// Gus is a guest, for trying what a guest sees.
const members = (await (
	await fetch(`${server}${w}/members`, { headers: { cookie } })
).json()) as { user: { id: string; email: string } }[];
const gus = members.find((m) => m.user.email === "gus@acme.test");
const snapshot = (await (
	await fetch(`${server}${w}/sync/bootstrap`, { headers: { cookie } })
).json()) as { records: { Repository?: { id: string; name: string }[] } };
const first = snapshot.records.Repository?.[0];
const added =
	gus &&
	first &&
	(
		await fetch(`${server}${w}/members/${gus.user.id}/access`, {
			method: "PUT",
			headers: { "content-type": "application/json", cookie },
			body: JSON.stringify({ items: [first.id] }),
		})
	).ok;
console.log(
	added
		? `Gus Guest is a guest in ${first.name}. Sign in as anyone in people.toml.`
		: "Couldn't add Gus Guest to the repository: is he in people.toml?",
);
