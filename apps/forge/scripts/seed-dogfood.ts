// Describes Forge's own code as services in the local "Forge" workspace, so
// drift checks can run on the vault: each service names its folder
// (relative to the folder containing the vault, so it works in development
// and in Docker) and its paths, which start at the vault's root. Safe to run again: existing services with the
// same name are updated. Needs `bun run dev` (or `bun run app` with
// FORGE_URL=http://localhost:7390) and `bun run seed`.
import { account, BASE, call, DEV_ADMIN } from "./seed-dev";

const REPOSITORY = "villain-vault";
const FORGE = "apps/forge/";

const SERVICES: { name: string; description: string; paths: string[] }[] = [
  {
    name: "Sync engine",
    description:
      "Keeps every client's local copy in step with Postgres: transactions, the change log, snapshots, the live feed, and the client's store and outbox.",
    paths: [
      "crates/sync/**",
      "packages/sync-client/**",
      "crates/domain/src/protocol.rs",
      "crates/server/src/ws.rs",
    ],
  },
  {
    name: "Data model",
    description: "Forge's records, their validation and rules, and the database schema.",
    paths: ["crates/domain/**", "migrations/**"],
  },
  {
    name: "Accounts and access",
    description:
      "Sign-up, sign-in, sessions, invites, roles, members and teams, and the desktop app's tokens.",
    paths: [
      "crates/server/src/{auth,accounts,invites,members,teams,limits,routes,config,state}.rs",
      "apps/web/src/{auth,session,server,api}.ts*",
    ],
  },
  {
    name: "Forge AI",
    description:
      "Drift checks of services against their code, and triage of new issues, as suggestions people accept or dismiss.",
    paths: ["crates/ai/**", "crates/server/src/drift/**", "apps/web/src/suggestions.tsx"],
  },
  {
    name: "MCP server",
    description:
      "The tool coding agents use to read and change the tracker, acting as an agent member.",
    paths: [
      "crates/server/src/mcp/**",
      "crates/server/src/agents.rs",
      "apps/web/src/settings-agents.tsx",
    ],
  },
  {
    name: "GitHub connection",
    description:
      "The GitHub App: installations, webhooks, linking pull requests and commits to issues, and the clones drift checks read.",
    paths: ["crates/server/src/github/**", "apps/web/src/settings-github.tsx"],
  },
  {
    name: "Web app",
    description: "The React app people use in the browser and inside the desktop app.",
    paths: ["apps/web/**"],
  },
  {
    name: "Desktop app",
    description:
      "The Tauri shell: notifications, the dock badge, links, and the session token in the keychain.",
    paths: ["apps/desktop/**"],
  },
];

const admin = await account(DEV_ADMIN, { workspace_name: "Forge" });
const workspace = admin.body.workspaces.find((w: { name: string }) => w.name === "Forge");
const bootstrap = await fetch(`${BASE}/api/w/${workspace.id}/sync/bootstrap`, {
  headers: { cookie: admin.session ?? "" },
}).then((r) => r.json());
const team =
  bootstrap.records.Team.find((t: { key: string }) => t.key === "FOR") ?? bootstrap.records.Team[0];
const existing: { id: string; name: string }[] = bootstrap.records.Service;

const operations = SERVICES.map((service) => {
  const data = {
    ...service,
    repository: REPOSITORY,
    paths: service.paths.map((path) => FORGE + path),
  };
  const found = existing.find((s) => s.name.toLowerCase() === service.name.toLowerCase());
  return found
    ? { action: "update", model: "Service", id: found.id, data }
    : {
        action: "create",
        model: "Service",
        id: crypto.randomUUID(),
        data: { ...data, team_id: team?.id },
      };
});
const result = await call(
  `/api/w/${workspace.id}/sync/transactions`,
  { id: crypto.randomUUID(), operations },
  admin.session,
);
if (result.status !== 200)
  throw new Error(`couldn't save the services: ${JSON.stringify(result.body)}`);

console.log(`${SERVICES.length} services describe Forge in "${workspace.name}".`);
console.log(
  `With ANTHROPIC_API_KEY set on the server, open ${BASE}/${workspace.url_key}/services and press "Check for drift".`,
);
