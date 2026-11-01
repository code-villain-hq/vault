import type { ClaudeCodeInfo, ClaudeKeyInfo, ReadToken, ReadTokenInfo } from "@almanac/types";
import { type AccessConfig, accountPages, messageOf, useApi } from "@code-villain/web";
import { CircleCheck, Terminal } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { dateWords, dayKey } from "./format";
import { answersHere, claudeHere, setAnswersHere } from "./here";
import { PageBody, PageHeader } from "./layout";
import { isDesktop } from "./platform";
import { apiUrl } from "./server";
import { loadSession, signedIn, signOut } from "./session";
import { BlockGallery } from "./topic";
import { Button, ErrorMessage, Section } from "./ui";
import { useIsAdmin, useIsGuest, useWorkspace } from "./workspace";

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
 * signed in to it, or ChatGPT, and whether it may search the web. On a server it's the
 * key Almanac answers with, which stays on the server and never syncs; in the
 * desktop app, Claude Code on this Mac answers first there too.
 */
export function ClaudePage() {
  const { workspace, place } = useWorkspace();
  const guest = useIsGuest();
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
          <>
            {isDesktop && place.kind !== "mac" && <ClaudeHere />}
            <TheKey data={data} path={path} reload={reload} />
          </>
        )}
        {/* A guest sees no topics, so there's nothing to read. */}
        {!guest && <Reading />}
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

/** Who can answer on this Mac, as Settings names them. */
const WHO = [
  { key: "claude_code", name: "Claude Code", adapter: "claude-agent-acp", settings: "Claude Code" },
  { key: "chatgpt", name: "ChatGPT", adapter: "codex-acp", settings: "Codex" },
] as const;
type Who = (typeof WHO)[number];

function ClaudeCode({ data, path, reload }: ClaudeProps) {
  const { workspace } = useWorkspace();
  const admin = useIsAdmin();
  const [problem, setProblem] = useState("");
  const who = WHO.find((w) => w.key === data.answers) ?? WHO[0];
  const code = who.key === "chatgpt" ? data.chatgpt : data.claude_code;

  const setAnswers = async (answers: Who["key"]) => {
    setProblem("");
    try {
      await api("PUT", `${path}/answers`, { answers });
      await reload();
    } catch (e) {
      setProblem(messageOf(e));
    }
  };

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
          {data.chatgpt && (
            <fieldset
              className="ui-segmented self-start"
              aria-label="Who answers"
              disabled={!admin}
            >
              {WHO.map((w) => (
                <button
                  key={w.key}
                  type="button"
                  className="ui-segment"
                  aria-pressed={who.key === w.key}
                  onClick={() => who.key !== w.key && void setAnswers(w.key)}
                >
                  {w.name}
                </button>
              ))}
            </fieldset>
          )}
          {code?.state === "ready" ? (
            <Ready who={who} code={code} />
          ) : (
            code && <CantReach who={who} code={code} path={path} reload={reload} />
          )}
          <p className="text-muted">
            Almanac sends it what the topic sticks to, its page and the talk, and lets it use only
            Almanac's own tools: keeping points, asking you, reminders, and starting topics. Not
            your files, your shell, or your {who.settings} settings.
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

/**
 * Another app, such as Claude Code in a terminal, reading this workspace
 * over MCP with your token: shown once when it's made, in the command that
 * adds it.
 */
function Reading() {
  const { workspace } = useWorkspace();
  const path = `/api/w/${workspace.id}/reading`;
  const { data, reload } = useApi<ReadTokenInfo>(path);
  const [token, setToken] = useState("");
  const [problem, setProblem] = useState("");
  const [busy, setBusy] = useState(false);
  const run = async (method: "POST" | "DELETE") => {
    setBusy(true);
    setProblem("");
    try {
      const made = await api<ReadToken>(method, path);
      setToken(method === "POST" ? made.token : "");
      await reload();
    } catch (e) {
      setProblem(messageOf(e));
    } finally {
      setBusy(false);
    }
  };
  const on = (at: string) => dateWords(dayKey(new Date(at)));
  const address = new URL(apiUrl("/api/mcp/read"), location.href).href;
  return (
    <Section title="Reading from other apps">
      <div className="flex max-w-2xl flex-col gap-3">
        <p className="text-muted">
          Claude Code, or another app that speaks MCP, can read your topics with a token of yours:
          their pages and a search. It changes nothing.
        </p>
        {token ? (
          <>
            <p>Add it to Claude Code with this. It shows only now.</p>
            <Command
              text={`claude mcp add --transport http almanac ${address} --header "Authorization: Bearer ${token}"`}
            />
          </>
        ) : (
          data?.made_at && (
            <p>
              Your token was made {on(data.made_at)}
              {data.used_at ? `, last read ${on(data.used_at)}` : ", and hasn't read yet"}.
            </p>
          )
        )}
        <div className="flex gap-2">
          <Button variant="secondary" disabled={busy} onClick={() => void run("POST")}>
            {data?.made_at ? "Make a new token" : "Make a token"}
          </Button>
          {data?.made_at && (
            <Button variant="secondary" disabled={busy} onClick={() => void run("DELETE")}>
              Stop it reading
            </Button>
          )}
        </div>
        <ErrorMessage>{problem}</ErrorMessage>
      </div>
    </Section>
  );
}

/** It can answer on this Mac. */
function Ready({ who, code }: { who: Who; code: ClaudeCodeInfo }) {
  return (
    <div className="flex items-start gap-2.5">
      <CircleCheck
        size={15}
        strokeWidth={1.75}
        aria-hidden="true"
        className="mt-0.5 shrink-0 text-ok"
      />
      <div>
        <p className="font-semibold">{who.name} on this Mac</p>
        <p className="text-muted">
          {who.key === "chatgpt"
            ? "Signed in for Almanac, through"
            : "As you're signed in to it, through"}{" "}
          {who.adapter}
          {code.version ? ` ${code.version}` : ""}.
        </p>
      </div>
    </div>
  );
}

/**
 * A server's workspace in the desktop app: Claude Code on this Mac answers
 * first, and the server's key only when it can't. Yours, on this Mac, not
 * the workspace's.
 */
function ClaudeHere() {
  const { place } = useWorkspace();
  const [on, setOn] = useState(answersHere);
  const [code, setCode] = useState<ClaudeCodeInfo | null>(null);
  const [problem, setProblem] = useState("");
  const check = useCallback(async () => {
    setProblem("");
    try {
      setCode(await claudeHere(place));
    } catch (e) {
      setProblem(messageOf(e));
    }
  }, [place]);
  useEffect(() => {
    if (on) void check();
  }, [on, check]);
  const turn = (value: boolean) => {
    setAnswersHere(value);
    setOn(value);
  };
  return (
    <Section title="Claude Code on this Mac">
      <div className="flex max-w-2xl flex-col gap-3">
        <div className="flex items-center gap-3">
          <p className="min-w-0 flex-1">Answer with Claude Code on this Mac</p>
          <fieldset className="ui-segmented" aria-label="Answer with Claude Code on this Mac">
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
                aria-pressed={on === value}
                onClick={() => on !== value && turn(value)}
              >
                {label}
              </button>
            ))}
          </fieldset>
        </div>
        {on &&
          (code?.state === "ready" ? (
            <Ready who={WHO[0]} code={code} />
          ) : code ? (
            <CantReach who={WHO[0]} code={code} path="" reload={check} />
          ) : (
            !problem && <p className="text-muted">Looking for Claude Code on this Mac…</p>
          ))}
        <p className="text-muted">
          {on
            ? "Talks and new topics go to Claude Code here, as you're signed in to it, with only Almanac's tools on the server. The key below answers only when Claude Code can't."
            : "The key below answers, as it does in the browser."}{" "}
          This is yours, on this Mac.
        </p>
        <ErrorMessage>{problem}</ErrorMessage>
      </div>
    </Section>
  );
}

/** Why Claude Code can't answer, and what to do about it. */
function CantReach({
  who,
  code,
  path,
  reload,
}: {
  who: Who;
  code: ClaudeCodeInfo;
  path: string;
  reload: () => Promise<unknown>;
}) {
  const admin = useIsAdmin();
  const [checking, setChecking] = useState(false);
  const [signingIn, setSigningIn] = useState(false);
  const [problem, setProblem] = useState("");
  const check = async () => {
    setChecking(true);
    await reload();
    setChecking(false);
  };
  const signIn = async () => {
    setProblem("");
    setSigningIn(true);
    try {
      await api("POST", `${path}/chatgpt/sign-in`);
      await reload();
    } catch (e) {
      setProblem(messageOf(e));
    }
    setSigningIn(false);
  };
  const signsInHere = who.key === "chatgpt" && code.state === "signed_out";
  const [title, body] =
    code.state === "not_installed"
      ? [
          `Almanac can't reach ${who.name}`,
          `It talks to ${who.name} through its ACP adapter, which isn't installed. Install it, then ask again.`,
        ]
      : signsInHere
        ? [
            "ChatGPT isn't signed in for Almanac",
            "Almanac signs in on its own, apart from your Codex. Sign in once, in the browser.",
          ]
        : code.state === "signed_out"
          ? [
              "Claude Code isn't signed in",
              "Run claude in Terminal and sign in there. Almanac answers as soon as you have.",
            ]
          : [`Almanac can't reach ${who.name}`, code.problem ?? "It didn't answer."];
  return (
    <div className="flex flex-col gap-2.5 rounded-lg bg-panel px-5 py-4">
      <p className="flex items-center gap-2 font-semibold">
        <Terminal size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
        {title}
      </p>
      <p className="text-muted">{body}</p>
      {code.state === "not_installed" && <Command text={code.install} />}
      <div className="flex gap-2">
        {signsInHere && admin && (
          <Button disabled={signingIn} onClick={() => void signIn()}>
            {signingIn ? "Finish in your browser…" : "Sign in to ChatGPT"}
          </Button>
        )}
        <Button variant="secondary" disabled={checking} onClick={() => void check()}>
          Check again
        </Button>
      </div>
      <ErrorMessage>{problem}</ErrorMessage>
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
  const { workspace, place } = useWorkspace();
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
              : isDesktop && place.kind !== "mac"
                ? "There's no key yet, so Almanac can't answer in the browser, or here when Claude Code can't."
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
