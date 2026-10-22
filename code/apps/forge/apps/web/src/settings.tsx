import { type AccessConfig, accountPages } from "@code-villain/web";
import { api } from "./api";
import { useTeams } from "./data";
import { PageHeader } from "./layout";
import { loadSession, signedIn, signOut } from "./session";
import { useRecords, useWorkspace } from "./workspace";

/** A guest is added to teams, and admins and members are in some of them too. */
export const TEAM_ACCESS: AccessConfig = {
  noun: "team",
  plural: "teams",
  useOptions: useTeams,
  everyone: true,
};

const pages = accountPages({
  name: "Forge",
  api,
  useWorkspace,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
  access: TEAM_ACCESS,
});

export function MembersPage() {
  // Agents are members too, but they aren't people: they're listed apart.
  const agents = new Set(
    useRecords("User")
      .filter((u) => u.kind === "agent")
      .map((u) => u.id),
  );
  return (
    <pages.MembersPage
      agents={agents}
      removeBody="They lose access at once, and are unassigned from their issues."
    />
  );
}

export const AccountPage = pages.AccountPage;
