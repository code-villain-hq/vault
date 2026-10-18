import type { AiRun, Issue, Suggestion } from "@forge/types";
import { Link } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { api, messageOf } from "./api";
import { byId, decisionProposal, identifier, timeAgo, useStates } from "./data";
import { foldDiff, lineDiff, wordPairs } from "./diff";
import { Markdown } from "./markdown";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/** Open drift suggestions, newest first, optionally for one service. */
export function useOpenSuggestions(serviceId?: string) {
  const suggestions = useRecords("Suggestion");
  return useMemo(
    () =>
      suggestions
        .filter(
          (s) =>
            s.kind === "drift" && s.status === "open" && (!serviceId || s.service_id === serviceId),
        )
        .sort((a, b) => b.created_at.localeCompare(a.created_at)),
    [suggestions, serviceId],
  );
}

/** Accepting or dismissing a suggestion, with what went wrong. */
function useResolve(suggestion: Suggestion) {
  const { workspace } = useWorkspace();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resolve = async (action: "accept" | "dismiss") => {
    setBusy(true);
    setError(null);
    try {
      await api("POST", `/api/w/${workspace.id}/suggestions/${suggestion.id}/${action}`);
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  };
  return { busy, error, resolve };
}

/** Whether the server has a model for Forge AI, asked once per workspace. */
const aiStatus = new Map<string, Promise<boolean>>();

/** Whether Forge AI can run here; `undefined` until the server says. */
/** Whether the server has a model for Forge AI; undefined until it has said. */
export function useAiConfigured(): boolean | undefined {
  const { workspace } = useWorkspace();
  const [configured, setConfigured] = useState<boolean>();
  useEffect(() => {
    let status = aiStatus.get(workspace.id);
    if (!status) {
      status = api<{ configured: boolean }>("GET", `/api/w/${workspace.id}/ai`).then(
        (s) => s.configured,
        () => {
          aiStatus.delete(workspace.id);
          return false;
        },
      );
      aiStatus.set(workspace.id, status);
    }
    let current = true;
    void status.then((value) => current && setConfigured(value));
    return () => {
      current = false;
    };
  }, [workspace.id]);
  return configured;
}

/**
 * The triage suggestion for an issue, from Forge AI or an agent: the open
 * one, that Forge AI is still looking, or a button to ask it. Without a
 * model there's no button: an agent's suggestion arrives by itself.
 */
export function TriageSuggestion({ issueId }: { issueId: string }) {
  const { workspace } = useWorkspace();
  const aiConfigured = useAiConfigured();
  const suggestions = useRecords("Suggestion");
  const runs = useRecords("AiRun");
  const suggestion = suggestions.find(
    (s) => s.kind === "triage" && s.status === "open" && s.issue_id === issueId,
  );
  const triageRuns = runs
    .filter((r) => r.kind === "triage" && r.issue_id === issueId)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
  const looking = triageRuns.some((r) => r.status === "queued" || r.status === "running");
  const last = triageRuns[0];
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);

  if (suggestion) return <TriageCard suggestion={suggestion} />;
  if (looking) return <p className="text-xs text-muted">Forge AI is looking at this issue…</p>;
  if (!aiConfigured) return null;
  return (
    <div className="flex items-center gap-3 text-xs">
      <Button
        variant="secondary"
        disabled={asking}
        onClick={() => {
          setError(null);
          setAsking(true);
          api("POST", `/api/w/${workspace.id}/issues/${issueId}/triage`)
            .catch((e) => setError(messageOf(e)))
            .finally(() => setAsking(false));
        }}
      >
        {last ? "Ask Forge AI again" : "Ask Forge AI to triage"}
      </Button>
      {last?.status === "failed" && <span className="text-danger-ink">{last.error}</span>}
      {last?.status === "succeeded" && last.suggestions === 0 && (
        <span className="text-faint">Nothing to suggest last time.</span>
      )}
      {error && <span className="text-danger-ink">{error}</span>}
    </div>
  );
}

function TriageCard({ suggestion }: { suggestion: Suggestion }) {
  const { busy, error, resolve } = useResolve(suggestion);
  const author = useRecords("User").find((u) => u.id === suggestion.created_by);
  const [why, setWhy] = useState(false);
  // An agent that found nothing to change says so, and why.
  const nothing = suggestion.changes.length === 0;
  return (
    <div className="flex flex-col gap-2 rounded-md bg-surface px-3 py-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        <span className="font-medium text-fg">
          {author?.name ?? "Forge AI"} suggests{nothing && " leaving it as it is"}
        </span>
        {suggestion.changes.map((change) => (
          <span key={change}>{change}</span>
        ))}
        <span className="ml-auto flex gap-2">
          <Button variant="secondary" onClick={() => setWhy(!why)}>
            {why ? "Hide why" : "Why?"}
          </Button>
          {!nothing && (
            // Secondary: the triage bar's Accept is the action this state needs.
            <Button variant="secondary" disabled={busy} onClick={() => void resolve("accept")}>
              Apply
            </Button>
          )}
          <Button variant="secondary" disabled={busy} onClick={() => void resolve("dismiss")}>
            {nothing ? "Done" : "Dismiss"}
          </Button>
        </span>
      </div>
      {why && (
        <div className="text-xs text-muted">
          <Markdown text={suggestion.body} />
        </div>
      )}
      <ErrorMessage>{error}</ErrorMessage>
    </div>
  );
}

/** The description a proposal would give its issue. */
export function proposedDescription(suggestion: Suggestion): string | undefined {
  for (const operation of suggestion.operations as {
    action?: string;
    model?: string;
    data?: { description?: unknown };
  }[]) {
    if (
      operation.action === "update" &&
      operation.model === "Issue" &&
      typeof operation.data?.description === "string"
    ) {
      return operation.data.description;
    }
  }
}

/**
 * New descriptions agents propose for the issue, for people to accept or
 * dismiss: agents propose changes to an issue's scope, people decide.
 */
export function DescriptionProposals({ issue }: { issue: Issue }) {
  const suggestions = useRecords("Suggestion");
  const users = useRecords("User");
  const stateType = useStates().get(issue.state_id)?.type;
  // The one an agent asks for a decision with is in the triage bar instead.
  const asked = decisionProposal(issue, stateType, users, suggestions);
  const proposals = suggestions
    .filter(
      (s) =>
        s.kind === "description" &&
        s.status === "open" &&
        s.issue_id === issue.id &&
        s.id !== asked?.id,
    )
    .sort((a, b) => a.created_at.localeCompare(b.created_at));
  return proposals.map((suggestion) => (
    <DescriptionProposal key={suggestion.id} suggestion={suggestion} issue={issue} />
  ));
}

function DescriptionProposal({ suggestion, issue }: { suggestion: Suggestion; issue: Issue }) {
  const users = useRecords("User");
  const author = users.find((u) => u.id === suggestion.created_by);
  const { busy, error, resolve } = useResolve(suggestion);

  return (
    <article className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <h3 className="text-sm">
          <span className="font-medium">{author?.name ?? "Former member"}</span> proposes a new
          description
        </h3>
        <span className="ml-auto flex gap-2">
          {/* An issue can carry several proposals; none claims the one primary action. */}
          <Button variant="secondary" disabled={busy} onClick={() => void resolve("accept")}>
            Accept
          </Button>
          <Button variant="secondary" disabled={busy} onClick={() => void resolve("dismiss")}>
            Dismiss
          </Button>
        </span>
      </header>
      <Markdown text={suggestion.body} />
      <DescriptionDiff issue={issue} suggestion={suggestion} />
      <ErrorMessage>{error}</ErrorMessage>
    </article>
  );
}

/** What a proposal changes in its issue's description, as a diff. */
export function DescriptionDiff({ issue, suggestion }: { issue: Issue; suggestion: Suggestion }) {
  const proposed = proposedDescription(suggestion) ?? "";
  // Against the description as it is now, which is what accepting replaces.
  return (
    <TextDiff label="What changes in the description" before={issue.description} after={proposed} />
  );
}

/** What changes from one text to the other, line by line, unchanged runs folded. */
export function TextDiff({
  label,
  before,
  after,
}: {
  label: string;
  before: string;
  after: string;
}) {
  const diff = useMemo(
    () => foldDiff(wordPairs(lineDiff(before.trim(), after.trim()))),
    [before, after],
  );

  return (
    <figure
      aria-label={label}
      className="overflow-x-auto rounded-md bg-surface py-1 font-mono text-xs leading-relaxed"
    >
      {diff.map((line, index) =>
        "skipped" in line ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity but their place
          <div key={index} className="px-3 text-faint">
            ⋯ {line.skipped} unchanged {line.skipped === 1 ? "line" : "lines"}
          </div>
        ) : (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity but their place
            key={index}
            className={`whitespace-pre-wrap px-3 ${
              line.kind === "added"
                ? "tint-ok"
                : line.kind === "removed"
                  ? `tint-danger ${line.words ? "" : "line-through"}`
                  : "text-muted"
            }`}
          >
            <span aria-hidden="true" className="mr-2 select-none text-faint">
              {line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}
            </span>
            {line.kind !== "same" && (
              <span className="sr-only">{line.kind === "added" ? "Added: " : "Removed: "}</span>
            )}
            {line.words
              ? // Only the words that changed stand out; the rest reads as before.
                line.words.map((word, at) => (
                  <span
                    // biome-ignore lint/suspicious/noArrayIndexKey: words have no identity but their place
                    key={at}
                    className={
                      word.kind === "added"
                        ? // The word's tint over its line's: the status recipe, twice.
                          "tint-ok rounded-sm"
                        : word.kind === "removed"
                          ? "tint-danger rounded-sm line-through"
                          : undefined
                    }
                  >
                    {word.text}
                  </span>
                ))
              : line.text || " "}
          </div>
        ),
      )}
    </figure>
  );
}

/** One suggestion from Forge AI: what drifted, the evidence, and the fix. */
export function SuggestionCard({ suggestion }: { suggestion: Suggestion }) {
  const { workspace } = useWorkspace();
  const services = useRecords("Service");
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const service = services.find((s) => s.id === suggestion.service_id);
  const issue = issues.find((i) => i.id === suggestion.issue_id);
  const spec = useRecords("Spec").find((s) => s.id === suggestion.spec_id);
  const { busy, error, resolve } = useResolve(suggestion);

  return (
    <article className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <header className="flex items-start gap-3">
        <span className="ui-chip mt-0.5 px-2 py-1">Drift</span>
        <div className="min-w-0 flex-1">
          <h3 className="font-medium">{suggestion.title}</h3>
          <p className="text-xs text-muted">
            Forge AI ·{" "}
            {service && (
              <Link
                to="/$workspace/service/$service"
                params={{ workspace: workspace.url_key, service: service.id }}
                className="hover:text-fg"
              >
                {service.name}
              </Link>
            )}
            {issue && (
              <>
                {" · "}
                <Link
                  to="/$workspace/issue/$identifier"
                  params={{
                    workspace: workspace.url_key,
                    identifier: identifier(issue, teamsById),
                  }}
                  className="hover:text-fg"
                >
                  {identifier(issue, teamsById)}
                </Link>
              </>
            )}
            {spec && (
              <>
                {" · "}
                <Link
                  to="/$workspace/spec/$spec"
                  params={{ workspace: workspace.url_key, spec: spec.id }}
                  className="hover:text-fg"
                >
                  {suggestion.requirement ? `Requirement: ${suggestion.requirement}` : spec.title}
                </Link>
              </>
            )}
            {" · "}
            <time
              dateTime={suggestion.created_at}
              title={new Date(suggestion.created_at).toLocaleString()}
            >
              {timeAgo(suggestion.created_at)}
            </time>
          </p>
        </div>
      </header>
      <Markdown text={suggestion.body} />
      {suggestion.changes.length > 0 ? (
        <div>
          <h4 className="mb-1 text-xs font-medium text-muted">If you accept</h4>
          <ul className="list-disc pl-5">
            {suggestion.changes.map((change) => (
              <li key={change}>{change}</li>
            ))}
          </ul>
        </div>
      ) : (
        <p className="text-xs text-muted">
          No changes proposed: this one needs a person to decide.
        </p>
      )}
      <ErrorMessage>{error}</ErrorMessage>
      <div className="flex gap-2">
        {/* Cards come in lists, so none of them claims the one primary action. */}
        {suggestion.changes.length > 0 && (
          <Button variant="secondary" disabled={busy} onClick={() => void resolve("accept")}>
            Accept
          </Button>
        )}
        <Button variant="secondary" disabled={busy} onClick={() => void resolve("dismiss")}>
          {suggestion.changes.length > 0 ? "Dismiss" : "Done"}
        </Button>
      </div>
    </article>
  );
}

/** A service's drift checks: start one, and see how the last ones went. */
export function DriftChecks({
  serviceId,
  hasRepository,
}: {
  serviceId: string;
  hasRepository: boolean;
}) {
  const { workspace } = useWorkspace();
  const runs = useRecords("AiRun");
  const [error, setError] = useState<string | null>(null);
  const mine = useMemo(
    () =>
      runs
        .filter((r) => r.kind === "drift" && r.service_id === serviceId)
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .slice(0, 5),
    [runs, serviceId],
  );
  const busy = mine.some((r) => r.status === "queued" || r.status === "running");

  const start = async () => {
    setError(null);
    try {
      await api("POST", `/api/w/${workspace.id}/services/${serviceId}/check`);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-medium text-muted">Drift checks</h2>
        <Button
          variant="secondary"
          disabled={busy || !hasRepository}
          title={hasRepository ? undefined : "Give the service a repository first"}
          onClick={() => void start()}
        >
          {busy ? "Checking…" : "Check for drift"}
        </Button>
      </div>
      <ErrorMessage>{error}</ErrorMessage>
      {mine.length === 0 && (
        <p className="text-muted">
          Forge AI reads this service's code and history and suggests where the tracker no longer
          matches it.
        </p>
      )}
      <ul className="flex flex-col gap-2">
        {mine.map((run) => (
          <RunRow key={run.id} run={run} />
        ))}
      </ul>
    </section>
  );
}

function RunRow({ run }: { run: AiRun }) {
  const [open, setOpen] = useState(false);
  const label = {
    queued: "Waiting",
    running: "Checking…",
    succeeded: `${run.suggestions} ${run.suggestions === 1 ? "suggestion" : "suggestions"}`,
    failed: "Failed",
  }[run.status];
  const tone = {
    queued: "text-faint",
    running: "text-warn-ink",
    succeeded: "text-ok-ink",
    failed: "text-danger-ink",
  }[run.status];
  return (
    <li className="rounded-md">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-hover"
      >
        <span className={`text-xs font-medium ${tone}`}>{label}</span>
        <span className="text-xs text-muted">
          {new Date(run.created_at).toLocaleString(undefined, {
            month: "short",
            day: "numeric",
            hour: "numeric",
            minute: "2-digit",
          })}
          {" · "}
          {run.trigger}
        </span>
        {run.cost_usd > 0 && (
          <span className="ml-auto text-xs text-faint">${run.cost_usd.toFixed(2)}</span>
        )}
      </button>
      {open && (run.summary || run.error) && (
        <div className="px-3 py-2">
          {run.error && <p className="mb-2 text-danger-ink">{run.error}</p>}
          <Markdown text={run.summary} />
        </div>
      )}
    </li>
  );
}
