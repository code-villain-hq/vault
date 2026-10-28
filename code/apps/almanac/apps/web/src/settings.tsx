import type { ClaudeKeyInfo } from "@almanac/types";
import { type AccessConfig, accountPages, messageOf, useApi } from "@code-villain/web";
import { useState } from "react";
import { api } from "./api";
import { PageBody, PageHeader } from "./layout";
import { loadSession, signedIn, signOut } from "./session";
import { Button, ErrorMessage, Section } from "./ui";
import { useIsAdmin, useWorkspace } from "./workspace";

/** A guest sees no topics yet: there's nothing to add them to. */
export const TOPIC_ACCESS: AccessConfig = { noun: "topic", plural: "topics", useOptions: () => [] };

export const { MembersPage, AccountPage } = accountPages({
  name: "Almanac",
  api,
  useWorkspace,
  loadSession,
  signedIn,
  signOut,
  PageHeader,
  access: TOPIC_ACCESS,
});

/**
 * Settings → Claude: the key Almanac answers with. It stays on the server
 * Almanac runs on (this Mac's, for a workspace on this Mac) and never syncs.
 */
export function ClaudePage() {
  const { workspace } = useWorkspace();
  const admin = useIsAdmin();
  const path = `/api/w/${workspace.id}/claude`;
  const { data, error, reload } = useApi<ClaudeKeyInfo>(path);
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState("");
  const [saving, setSaving] = useState(false);

  const run = async (method: string, body?: unknown) => {
    setSaving(true);
    setProblem("");
    try {
      await api(method, path, body);
      setKey("");
      await reload();
    } catch (e) {
      setProblem(messageOf(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <PageHeader title="Claude" parent="Settings" />
      <PageBody>
        <Section title="The key Almanac answers with">
          <div className="flex max-w-xl flex-col gap-3">
            <ErrorMessage>{error}</ErrorMessage>
            {data && (
              <p>
                {data.set
                  ? `Almanac answers with the key ending in ${data.ends_with ?? "…"}.`
                  : data.from_settings
                    ? "Almanac answers with the key in its server's settings."
                    : "There's no key yet, so Almanac can't answer or set up topics."}
              </p>
            )}
            <p className="text-muted">
              Talks go to Claude through the Anthropic API, with what the topic sticks to and what
              its page kept. Web search is off. The key stays on the server Almanac runs on and
              never syncs; make one in the Claude Console under API keys.
            </p>
            {admin ? (
              <form
                className="flex flex-col gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  if (key.trim()) void run("PUT", { api_key: key.trim() });
                }}
              >
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-muted">
                    {data?.set ? "A new key" : "API key"}
                  </span>
                  <input
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={key}
                    onChange={(e) => setKey(e.target.value)}
                    placeholder="sk-ant-…"
                    className="ui-field h-9 rounded-md px-3 font-mono"
                  />
                </label>
                <ErrorMessage>{problem}</ErrorMessage>
                <div className="flex gap-2">
                  <Button type="submit" disabled={!key.trim() || saving}>
                    Save key
                  </Button>
                  {data?.set && (
                    <Button
                      variant="secondary"
                      disabled={saving}
                      onClick={() => void run("DELETE")}
                    >
                      Remove the key
                    </Button>
                  )}
                </div>
              </form>
            ) : (
              <p className="text-muted">Only an admin of {workspace.name} changes it.</p>
            )}
          </div>
        </Section>
      </PageBody>
    </>
  );
}
