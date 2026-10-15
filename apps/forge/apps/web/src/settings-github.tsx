import { useCallback, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { timeAgo } from "./data";
import { PageHeader } from "./layout";
import { publicUrl } from "./server";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

export interface GitHubStatus {
  configured: boolean;
  install_url: string | null;
  /** Whether connecting an installation checks that its installer can access it on GitHub. */
  confirms_ownership: boolean;
  installations: {
    installation_id: number;
    account: string;
    repositories: {
      full_name: string;
      default_branch: string;
      mirrored_at: string | null;
      mirror_error: string | null;
    }[];
  }[];
}

/** The workspace's GitHub connection, if the server has one. */
export function useGitHubStatus() {
  const { workspace } = useWorkspace();
  const [status, setStatus] = useState<GitHubStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setStatus(await api<GitHubStatus>("GET", `/api/w/${workspace.id}/github`));
    } catch (e) {
      setError(messageOf(e));
    }
  }, [workspace.id]);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { status, error, reload };
}

export function GitHubSettingsPage() {
  const { workspace } = useWorkspace();
  const { status, error, reload } = useGitHubStatus();
  const [refreshing, setRefreshing] = useState(false);

  return (
    <>
      <PageHeader title="GitHub" />
      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-8 px-6 py-8">
          <ErrorMessage>{error}</ErrorMessage>
          <section>
            <h2 className="mb-1 font-medium">Connect GitHub</h2>
            <p className="text-muted">
              With GitHub connected, Forge keeps a copy of your repositories for drift checks and
              links pull requests and commits that mention an issue. A pull request that works on an
              issue (FOR-12 in its branch name, or "Fixes FOR-12" in its title or body) moves it to
              review when it opens and completes it when it merges.
            </p>
          </section>

          {status && !status.configured && (
            <section className="flex flex-col gap-2 rounded-lg border border-line p-4">
              <h3 className="font-medium">This server isn't set up for GitHub yet</h3>
              <p className="text-muted">Whoever runs the server creates a GitHub App and sets:</p>
              <pre className="overflow-x-auto rounded-md bg-hover p-3 font-mono text-xs">
                {[
                  "FORGE_GITHUB_APP_ID=…",
                  "FORGE_GITHUB_APP_SLUG=…            # from github.com/apps/<slug>",
                  "FORGE_GITHUB_PRIVATE_KEY_PATH=…    # the app's private key (.pem),",
                  "                                   # or FORGE_GITHUB_PRIVATE_KEY=… with the key itself",
                  "FORGE_GITHUB_WEBHOOK_SECRET=…",
                  "FORGE_DATA_DIR=…                   # optional: where clones are kept (./data)",
                ].join("\n")}
              </pre>
              <p className="text-muted">
                The app needs read access to repository contents, metadata, pull requests, checks
                and commit statuses, and the push, pull request, check suite, check run, status,
                installation and installation repositories events. Point its webhook at{" "}
                <code>{publicUrl("/api/github/webhook")}</code> and its setup URL at{" "}
                <code>{publicUrl("/api/github/setup")}</code>.
              </p>
              <p className="text-muted">
                To let people sign in with GitHub too, also set <code>FORGE_GITHUB_CLIENT_ID</code>{" "}
                and <code>FORGE_GITHUB_CLIENT_SECRET</code> from the app, give it the "Email
                addresses" account permission (read), and set its callback URL to{" "}
                <code>{publicUrl("/api/auth/github/callback")}</code>. Then also turn on "Request
                user authorization (OAuth) during installation", so Forge can confirm that whoever
                connects an installation can access it on GitHub.
              </p>
            </section>
          )}

          {status?.configured && (
            <section className="flex flex-col gap-4">
              {status.install_url ? (
                <a
                  href={status.install_url}
                  className="inline-flex h-8 items-center self-start rounded-md bg-accent-fill px-3 font-medium text-white hover:brightness-110"
                >
                  {status.installations.length
                    ? "Add an organisation or repositories"
                    : "Connect GitHub"}
                </a>
              ) : (
                <p className="text-muted">Ask a workspace admin to connect GitHub.</p>
              )}
              {status.install_url && !status.confirms_ownership && (
                <p className="text-xs text-muted">
                  Forge only confirms that whoever connects an installation can access it on GitHub
                  once the server has GitHub sign-in set up (<code>FORGE_GITHUB_CLIENT_ID</code> and{" "}
                  <code>FORGE_GITHUB_CLIENT_SECRET</code> from this app) and the app has "Request
                  user authorization (OAuth) during installation" turned on in its settings.
                </p>
              )}
              {status.installations.map((installation) => (
                <div key={installation.installation_id} className="flex flex-col gap-1">
                  <div className="flex items-baseline gap-2">
                    <span className="font-medium">{installation.account}</span>
                    <span className="text-xs text-faint">
                      {installation.repositories.length} repositories
                    </span>
                  </div>
                  <ul className="-mx-3 flex flex-col gap-0.5">
                    {installation.repositories.map((repo) => (
                      <li
                        key={repo.full_name}
                        title={`github:${repo.full_name}`}
                        className="flex items-center gap-3 rounded-lg px-3 py-1.5 hover:bg-hover"
                      >
                        {/* The account is the heading; the repository's own name is enough. */}
                        <span className="flex-1 truncate">
                          {repo.full_name.split("/").slice(1).join("/") || repo.full_name}
                        </span>
                        <span
                          className={`text-xs ${repo.mirror_error ? "text-danger" : "text-faint"}`}
                          // Why it failed, or when it was last updated, on hover.
                          title={
                            repo.mirror_error ??
                            (repo.mirrored_at
                              ? new Date(repo.mirrored_at).toLocaleString()
                              : undefined)
                          }
                        >
                          {repo.mirror_error
                            ? "Couldn't update the copy"
                            : repo.mirrored_at
                              ? `updated ${timeAgo(repo.mirrored_at)}`
                              : "Copying…"}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
              {status.installations.length > 0 && workspace.role === "admin" && (
                <Button
                  variant="secondary"
                  className="self-start"
                  disabled={refreshing}
                  onClick={async () => {
                    setRefreshing(true);
                    await api("POST", `/api/w/${workspace.id}/github/refresh`).catch(() => {});
                    setTimeout(() => {
                      setRefreshing(false);
                      void reload();
                    }, 3000);
                  }}
                >
                  {refreshing ? "Updating…" : "Update copies now"}
                </Button>
              )}
              <p className="text-xs text-faint">
                To check a repository for drift, set a service's repository to its{" "}
                <code>github:owner/name</code>.
              </p>
            </section>
          )}
        </div>
      </main>
    </>
  );
}
