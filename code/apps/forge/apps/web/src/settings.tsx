import { accountPages } from "@code-villain/web";
import { useState } from "react";
import { api } from "./api";
import { GITHUB_PROBLEMS, useProviders } from "./auth";
import { PageHeader } from "./layout";
import { publicUrl } from "./server";
import { loadSession, signedIn, signOut } from "./session";
import { buttonClass, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

const pages = accountPages({
  name: "Forge",
  api,
  useWorkspace,
  usePasswords: (origin) => useProviders(origin)?.password,
  publicUrl,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
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

export function AccountPage() {
  return (
    <pages.AccountPage noPassword="You joined with GitHub. A password lets you sign in without it too.">
      <GitHubConnection />
    </pages.AccountPage>
  );
}

function GitHubConnection() {
  const { place, me } = useWorkspace();
  const available = useProviders(place.origin)?.github;
  // Why connecting didn't work, when GitHub sent us back here.
  const [problem] = useState(() => {
    const code = new URLSearchParams(location.search).get("github_error");
    return code ? (GITHUB_PROBLEMS[code] ?? GITHUB_PROBLEMS.github_failed) : null;
  });
  if (!available) return null;
  return (
    <section>
      <h2 className="mb-1 font-medium">GitHub</h2>
      <ErrorMessage>{problem}</ErrorMessage>
      {me.github_connected ? (
        <p className="text-muted">Connected: you can sign in with GitHub.</p>
      ) : (
        <>
          <p className="mb-3 text-muted">Connect your GitHub account to sign in with it.</p>
          <a
            href={`/api/auth/github/connect?redirect=${encodeURIComponent(location.pathname)}`}
            className={buttonClass("secondary")}
          >
            Connect GitHub
          </a>
        </>
      )}
    </section>
  );
}
