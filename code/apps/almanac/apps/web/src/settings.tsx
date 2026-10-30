import type { ClaudeCodeInfo, ClaudeKeyInfo } from "@almanac/types";
import { type AccessConfig, accountPages, messageOf, useApi } from "@code-villain/web";
import { CircleCheck, Terminal } from "lucide-react";
import { useState } from "react";
import { api } from "./api";
import { PageBody, PageHeader } from "./layout";
import { loadSession, signedIn, signOut } from "./session";
import { BlockGallery } from "./topic";
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
 * Settings → Claude: who answers. On this Mac that's Claude Code, as you're
 * signed in to it, and whether it may search the web. On a server it's the
 * key Almanac answers with, which stays on the server and never syncs.
 */
export function ClaudePage() {
  const { workspace, place } = useWorkspace();
  const path = `/api/w/${workspace.id}/claude`;
  const { data, error, reload } = useApi<ClaudeKeyInfo>(path);
  return (
    <>
      <PageHeader title="Claude" parent="Settings" />
      <PageBody>
        <ErrorMessage>{error}</ErrorMessage>
        {!data ? (
          place.kind === "mac" &&
          !error && <p className="text-muted">Looking for Claude Code on this Mac…</p>
        ) : data.via === "claude_code" ? (
          <ClaudeCode data={data} path={path} reload={reload} />
        ) : (
          <TheKey data={data} path={path} reload={reload} />
        )}
      </PageBody>
    </>
  );
}

/**
 * Settings → Blocks: the kinds of block a topic's page can have, each drawn
 * with sample points. A talk picks among them as it lays a page out.
 */
export function BlocksPage() {
  return (
    <>
      <PageHeader title="Blocks" parent="Settings" />
      <PageBody>
        <p className="max-w-measure text-muted">
          What a topic's page can be made of, each with sample points. Ask a talk to lay a page out
          anew and it picks from these.
        </p>
        <BlockGallery />
      </PageBody>
    </>
  );
}

interface ClaudeProps {
  data: ClaudeKeyInfo;
  path: string;
  reload: () => Promise<unknown>;
}

function ClaudeCode({ data, path, reload }: ClaudeProps) {
  const { workspace } = useWorkspace();
  const admin = useIsAdmin();
  const [problem, setProblem] = useState("");
  const code = data.claude_code;

  const setWeb = async (web: boolean) => {
    setProblem("");
    try {
      await api("PUT", `${path}/web`, { web });
      await reload();
    } catch (e) {
      setProblem(messageOf(e));
    }
  };

  return (
    <div className="flex max-w-2xl flex-col gap-7">
      <Section title="Who answers">
        <div className="flex flex-col gap-3">
          {code?.state === "ready" ? (
            <div className="flex items-start gap-2.5">
              <CircleCheck
                size={15}
                strokeWidth={1.75}
                aria-hidden="true"
                className="mt-0.5 shrink-0 text-ok"
              />
              <div>
                <p className="font-semibold">Claude Code on this Mac</p>
                <p className="text-muted">
                  As you're signed in to it, through claude-agent-acp
                  {code.version ? ` ${code.version}` : ""}.
                </p>
              </div>
            </div>
          ) : (
            code && <CantReach code={code} reload={reload} />
          )}
          <p className="text-muted">
            Almanac sends it what the topic sticks to, its page and the talk, and lets it use only
            Almanac's own tools: keeping points, asking you, reminders, and starting topics. Not
            your files, your shell, or your Claude Code settings.
          </p>
        </div>
      </Section>
      <Section title="The web">
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <p className="min-w-0 flex-1">
              Search the web when a question needs it, and link what it read
            </p>
            <fieldset className="ui-segmented" aria-label="Search the web" disabled={!admin}>
              {(
                [
                  [true, "On"],
                  [false, "Off"],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={label}
                  type="button"
                  className="ui-segment"
                  aria-pressed={data.web === value}
                  onClick={() => data.web !== value && void setWeb(value)}
                >
                  {label}
                </button>
              ))}
            </fieldset>
          </div>
          {!admin && <p className="text-muted">Only an admin of {workspace.name} changes it.</p>}
          <ErrorMessage>{problem}</ErrorMessage>
        </div>
      </Section>
    </div>
  );
}

/** Why Claude Code can't answer, and what to do about it. */
function CantReach({ code, reload }: { code: ClaudeCodeInfo; reload: () => Promise<unknown> }) {
  const [checking, setChecking] = useState(false);
  const check = async () => {
    setChecking(true);
    await reload();
    setChecking(false);
  };
  const [title, body] =
    code.state === "not_installed"
      ? [
          "Almanac can't reach Claude Code",
          "It talks to Claude Code through its ACP adapter, which isn't installed. Install it, then ask again.",
        ]
      : code.state === "signed_out"
        ? [
            "Claude Code isn't signed in",
            "Run claude in Terminal and sign in there. Almanac answers as soon as you have.",
          ]
        : ["Almanac can't reach Claude Code", code.problem ?? "It didn't answer."];
  return (
    <div className="flex flex-col gap-2.5 rounded-lg bg-panel px-5 py-4">
      <p className="flex items-center gap-2 font-semibold">
        <Terminal size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
        {title}
      </p>
      <p className="text-muted">{body}</p>
      {code.state === "not_installed" && <Command text={code.install} />}
      <div>
        <Button variant="secondary" disabled={checking} onClick={() => void check()}>
          Check again
        </Button>
      </div>
    </div>
  );
}

/** A command to run in Terminal, with Copy. */
function Command({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const [problem, setProblem] = useState("");
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      setProblem("Couldn't copy; select the command and copy it yourself.");
    }
  };
  return (
    <>
      <div className="flex items-center gap-2">
        <input
          readOnly
          value={text}
          aria-label="The command"
          onFocus={(e) => e.target.select()}
          className="ui-field h-8 min-w-0 flex-1 rounded-md px-2 font-mono text-xs"
        />
        <Button variant="secondary" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <ErrorMessage>{problem}</ErrorMessage>
    </>
  );
}

/** A company server answers with a key, set here or in its settings. */
function TheKey({ data, path, reload }: ClaudeProps) {
  const { workspace } = useWorkspace();
  const admin = useIsAdmin();
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
    <Section title="The key Almanac answers with">
      <div className="flex max-w-xl flex-col gap-3">
        <p>
          {data.set
            ? `Almanac answers with the key ending in ${data.ends_with ?? "…"}.`
            : data.from_settings
              ? "Almanac answers with the key in its server's settings."
              : "There's no key yet, so Almanac can't answer or set up topics."}
        </p>
        <p className="text-muted">
          Talks go to Claude through the Anthropic API, with what the topic sticks to and what its
          page kept. Web search is off. The key stays on the server Almanac runs on and never syncs;
          make one in the Claude Console under API keys.
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
                {data.set ? "A new key" : "API key"}
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
              {data.set && (
                <Button variant="secondary" disabled={saving} onClick={() => void run("DELETE")}>
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
  );
}
