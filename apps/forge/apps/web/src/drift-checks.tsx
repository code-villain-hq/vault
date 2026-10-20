// Drift checks: Forge AI's, and those agents run themselves and report through
// MCP (spec "Drift checks from agents").

import type {
  AiRun,
  CheckResult,
  CheckVerdict,
  DriftCheckDetails,
  Issue,
  UsagePart,
} from "@forge/types";
import { Link, useParams } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { api, messageOf } from "./api";
import { byId, identifier, timeAgo, useNow, useTeams } from "./data";
import { PageHeader } from "./layout";
import { Markdown } from "./markdown";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useSyncStatus, useWorkspace } from "./workspace";

/** An agent's running check unheard from for this long has lost contact. */
const LOST_CONTACT_MS = 30 * 60_000;

type Tone = "live" | "danger" | "warn" | "ok" | "neutral";

/** When the check started: agents' checks start running, Forge AI's wait first. */
const startOf = (run: AiRun) => run.started_at ?? run.created_at;

/** When an agent's running check was last heard from, if that's too long ago. */
function lostSince(run: AiRun, now: number): string | null {
  if (run.trigger !== "agent" || run.status !== "running") return null;
  const last = run.last_report_at ?? startOf(run);
  return now - Date.parse(last) > LOST_CONTACT_MS ? last : null;
}

const short = (sha: string) => sha.slice(0, 7);

/** "73f0be5..c02dcc0", or one commit for a full audit. */
function range(run: AiRun): string {
  if (!run.commit_to) return "";
  return run.commit_from
    ? `${short(run.commit_from)}..${short(run.commit_to)}`
    : short(run.commit_to);
}

function judged(run: AiRun): number {
  const t = run.tally;
  return t ? t.met + t.not_met + t.unclear + t.skipped + t.covered : 0;
}

/** "19:18" today, "Oct 19" before. */
function when(iso: string): string {
  const date = new Date(iso);
  return date.toDateString() === new Date().toDateString()
    ? date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    : date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** The issues the check's agent filed and commented on while it ran, as Forge saw them. */
function useSeen(run: AiRun | undefined): { filed: Issue[]; commented: Issue[] } {
  const issues = useRecords("Issue");
  const comments = useRecords("Comment");
  return useMemo(() => {
    if (!run) return { filed: [], commented: [] };
    const from = Date.parse(startOf(run));
    const to = run.finished_at ? Date.parse(run.finished_at) : Number.POSITIVE_INFINITY;
    const during = (at: string) => Date.parse(at) >= from && Date.parse(at) <= to;
    const filed = issues.filter((i) => i.created_by === run.created_by && during(i.created_at));
    const commentedIds = new Set(
      comments
        .filter((c) => c.author_id === run.created_by && during(c.created_at))
        .map((c) => c.issue_id),
    );
    return { filed, commented: issues.filter((i) => commentedIds.has(i.id)) };
  }, [run, issues, comments]);
}

/** How a check went, in a word or two: what a person should look at is coloured. */
function outcome(run: AiRun, seen: { filed: Issue[] }, now: number): { label: string; tone: Tone } {
  if (run.status === "queued") return { label: "Waiting", tone: "neutral" };
  if (run.status === "running") {
    return lostSince(run, now)
      ? { label: "Lost contact", tone: "danger" }
      : { label: "Checking", tone: "live" };
  }
  if (run.status === "failed") return { label: "Failed", tone: "danger" };
  if (run.trigger !== "agent") {
    return run.suggestions > 0
      ? { label: plural(run.suggestions, "suggestion", "suggestions"), tone: "warn" }
      : { label: "No suggestions", tone: "neutral" };
  }
  if ((run.tally?.unjudged ?? 0) > 0) return { label: "Incomplete", tone: "warn" };
  return seen.filed.length > 0
    ? { label: `${plural(seen.filed.length, "issue", "issues")} filed`, tone: "warn" }
    : { label: "Nothing filed", tone: "neutral" };
}

const TONE_CLASS: Record<Exclude<Tone, "live">, string> = {
  danger: "ui-status-danger",
  warn: "ui-status-warn",
  ok: "ui-status-ok",
  neutral: "",
};

function Pill({ label, tone }: { label: string; tone: Tone }) {
  if (tone === "live") {
    return (
      <span className="flex shrink-0 items-center gap-1.5 text-xs font-medium">
        <span className="ui-live-dot" aria-hidden="true" />
        {label}
      </span>
    );
  }
  return <span className={`ui-status shrink-0 px-2 ${TONE_CLASS[tone]}`}>{label}</span>;
}

/** One line of what a check found so far. */
function resultLine(run: AiRun, seen: { commented: Issue[] }, now: number): string {
  if (run.trigger !== "agent") {
    const text = run.error ?? run.summary;
    return (text.split("\n").find((l) => l.trim()) ?? "").replace(/[#>*_`]/g, "").trim();
  }
  const t = run.tally;
  const lost = lostSince(run, now);
  if (lost) return `no report since ${when(lost)}`;
  const parts: string[] = [];
  const done = judged(run);
  if (run.status === "running") {
    parts.push(`${done} judged`);
    if (run.last_report_at) parts.push(`last report ${timeAgo(run.last_report_at, now)}`);
    return parts.join(" · ");
  }
  if (run.status === "failed" && run.error) parts.push(run.error);
  parts.push(t?.unjudged ? `${done} of ${done + t.unjudged} judged` : `${done} judged`);
  if (t?.not_met) parts.push(`${t.not_met} unmet`);
  if (t?.unclear) parts.push(`${t.unclear} unclear`);
  if (seen.commented.length) parts.push(plural(seen.commented.length, "comment", "comments"));
  return parts.join(" · ");
}

/** A check in a list: how it went, what it checked, what it found, when. */
function CheckRow({ run }: { run: AiRun }) {
  const { workspace } = useWorkspace();
  const services = useRecords("Service");
  const now = useNow();
  const seen = useSeen(run);
  const service = services.find((s) => s.id === run.service_id);
  return (
    <li>
      <Link
        to="/$workspace/drift-check/$check"
        params={{ workspace: workspace.url_key, check: run.id }}
        className="flex h-9 items-center gap-3 rounded-md px-3 hover:bg-hover"
      >
        <Pill {...outcome(run, seen, now)} />
        {run.trigger === "agent" ? (
          <span className="shrink-0 font-mono text-xs">{range(run)}</span>
        ) : (
          <span className="shrink-0 text-xs">Forge AI{service ? ` · ${service.name}` : ""}</span>
        )}
        <span className="min-w-0 flex-1 truncate text-xs text-muted">
          {run.full_audit && <span className="ui-chip mr-2 px-2">Full audit</span>}
          {resultLine(run, seen, now)}
        </span>
        <span
          className="shrink-0 text-xs text-faint"
          title={new Date(startOf(run)).toLocaleString()}
        >
          {when(startOf(run))}
        </span>
      </Link>
    </li>
  );
}

function newestFirst(runs: AiRun[]): AiRun[] {
  return runs
    .filter((r) => r.kind === "drift")
    .sort((a, b) => startOf(b).localeCompare(startOf(a)));
}

/** The workspace's drift checks, newest first. */
export function DriftChecksPage() {
  const runs = newestFirst(useRecords("AiRun"));
  return (
    <>
      <PageHeader title="Drift checks" />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        {runs.length === 0 && (
          <p className="px-6 py-16 text-center text-muted">
            No drift checks yet. Agents report the checks they run through MCP, and Forge AI checks
            a service when a model is set up.
          </p>
        )}
        <ul className="flex flex-col gap-0.5 p-2">
          {runs.map((run) => (
            <CheckRow key={run.id} run={run} />
          ))}
        </ul>
      </div>
    </>
  );
}

/** The drift checks that covered a service, Forge AI's and agents'. */
export function useServiceChecks(serviceId: string): AiRun[] {
  const runs = useRecords("AiRun");
  return useMemo(
    () =>
      newestFirst(
        runs.filter((r) => r.service_id === serviceId || r.service_ids.includes(serviceId)),
      ),
    [runs, serviceId],
  );
}

/** A service's last three drift checks; with a model, a way to ask Forge AI for one. */
export function ServiceDriftChecks({
  serviceId,
  hasRepository,
  ai,
  checks,
}: {
  serviceId: string;
  hasRepository: boolean;
  ai: boolean;
  checks: AiRun[];
}) {
  const { workspace } = useWorkspace();
  const [error, setError] = useState<string | null>(null);
  const busy = checks.some(
    (r) => r.trigger !== "agent" && (r.status === "queued" || r.status === "running"),
  );

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
      <div className="flex items-center gap-3">
        <h2 className="text-xs font-medium text-muted">Drift checks</h2>
        {checks.length > 0 && (
          <Link
            to="/$workspace/drift-checks"
            params={{ workspace: workspace.url_key }}
            className="ui-link text-xs"
          >
            All checks
          </Link>
        )}
        {ai && (
          <Button
            variant="secondary"
            className="ml-auto"
            disabled={busy || !hasRepository}
            title={hasRepository ? undefined : "Give the service a repository first"}
            onClick={() => void start()}
          >
            {busy ? "Checking…" : "Check for drift"}
          </Button>
        )}
      </div>
      <ErrorMessage>{error}</ErrorMessage>
      {checks.length === 0 && (
        <p className="text-muted">
          Forge AI reads this service's code and history and suggests where the tracker no longer
          matches it.
        </p>
      )}
      <ul className="-mx-3 flex flex-col gap-0.5">
        {checks.slice(0, 3).map((run) => (
          <CheckRow key={run.id} run={run} />
        ))}
      </ul>
    </section>
  );
}

/** What a check found beyond the run itself; undefined while loading, null if it failed. */
function useDetails(run: AiRun | undefined): DriftCheckDetails | null | undefined {
  const { workspace } = useWorkspace();
  const [loaded, setLoaded] = useState<{ id: string; details: DriftCheckDetails | null }>();
  const id = run?.id;
  const updatedAt = run?.updated_at;
  const online = useSyncStatus().connection === "online";
  useEffect(() => {
    // Again whenever the check changes, so new verdicts show.
    void updatedAt;
    if (!id || !online) return;
    let current = true;
    api<DriftCheckDetails>("GET", `/api/w/${workspace.id}/drift-checks/${id}`)
      .then((details) => current && setLoaded({ id, details }))
      .catch(() => current && setLoaded((was) => (was?.id === id ? was : { id, details: null })));
    return () => {
      current = false;
    };
  }, [workspace.id, id, updatedAt, online]);
  return loaded && loaded.id === id ? loaded.details : undefined;
}

/** Verdict groups in the order a person should read them; folded ones need no attention. */
const GROUPS: { verdict: CheckVerdict | null; label: string; tone: Tone; folded: boolean }[] = [
  { verdict: "not_met", label: "Unmet", tone: "danger", folded: false },
  { verdict: "unclear", label: "Unclear", tone: "warn", folded: false },
  { verdict: null, label: "Not judged", tone: "warn", folded: false },
  { verdict: "covered", label: "Covered by open work", tone: "neutral", folded: true },
  { verdict: "skipped", label: "Skipped", tone: "neutral", folded: true },
  { verdict: "met", label: "Met", tone: "neutral", folded: true },
];

/** One check: how it went, its requirements by verdict, the issues Forge saw, its usage. */
export function DriftCheckPage() {
  const { check: id } = useParams({ strict: false }) as { check: string };
  const { workspace } = useWorkspace();
  const run = useRecords("AiRun").find((r) => r.id === id);
  const users = useRecords("User");
  const services = useRecords("Service");
  const now = useNow();
  const seen = useSeen(run);
  const details = useDetails(run);

  const list = (
    <Link
      to="/$workspace/drift-checks"
      params={{ workspace: workspace.url_key }}
      className="hover:text-fg"
    >
      Drift checks
    </Link>
  );
  if (!run) return <PageHeader title="Drift check not found" parent={list} />;

  const agent = run.trigger === "agent";
  const covered = services.filter((s) => s.id === run.service_id || run.service_ids.includes(s.id));
  const title = agent ? range(run) : `Forge AI${covered[0] ? ` · ${covered[0].name}` : ""}`;
  const by = users.find((u) => u.id === run.created_by);
  const took = run.finished_at
    ? Math.max(1, Math.round((Date.parse(run.finished_at) - Date.parse(startOf(run))) / 60_000))
    : null;
  const chips = [
    by?.name,
    run.checker,
    run.full_audit ? "Full audit" : null,
    covered.length > 3
      ? plural(covered.length, "service", "services")
      : covered.map((s) => s.name).join(", "),
    took === null ? null : `${took} min`,
  ].filter(Boolean);

  return (
    <>
      <PageHeader title={title} parent={list} />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="flex flex-col gap-8 px-8 py-6">
          <div className="flex flex-col gap-3">
            <h1 className={`text-xl ${agent ? "font-mono" : "font-semibold"}`}>{title}</h1>
            <div className="flex flex-wrap gap-1.5">
              {chips.map((c) => (
                <span key={c} className="ui-chip px-2 py-1 text-xs">
                  {c}
                </span>
              ))}
            </div>
            <Lead run={run} seen={seen} now={now} />
            {run.summary && <Markdown text={run.summary} />}
          </div>
          <Requirements run={run} details={details} />
          <SeenIssues filed={seen.filed} commented={seen.commented} agent={agent} />
          <Usage run={run} />
        </div>
      </div>
    </>
  );
}

/** How the check went, in a sentence or two. */
function Lead({
  run,
  seen,
  now,
}: {
  run: AiRun;
  seen: { filed: Issue[]; commented: Issue[] };
  now: number;
}) {
  const lost = lostSince(run, now);
  const t = run.tally;
  let how: string;
  if (run.status === "queued") how = "Waiting to start.";
  else if (lost) how = `Lost contact. Last heard from at ${when(lost)}.`;
  else if (run.status === "running") {
    how = run.last_report_at
      ? `Checking. Last report ${timeAgo(run.last_report_at, now)}.`
      : "Checking.";
  } else if (run.status === "failed") how = `Failed${run.error ? `: ${run.error}` : "."}`;
  else if (t?.unjudged) {
    how = `Incomplete: ${plural(t.unjudged, "planned requirement has", "planned requirements have")} no verdict.`;
  } else how = "Finished.";
  if (run.trigger !== "agent") return <p>{how}</p>;

  const saw =
    seen.filed.length || seen.commented.length
      ? `Forge saw ${[
          seen.filed.length ? `${plural(seen.filed.length, "issue", "issues")} filed` : null,
          seen.commented.length ? plural(seen.commented.length, "comment", "comments") : null,
        ]
          .filter(Boolean)
          .join(" and ")}.`
      : "Forge saw no issues filed or commented on.";
  const notes = [
    t?.unclear ? `${plural(t.unclear, "requirement is", "requirements are")} unclear` : null,
    t?.covered ? `${t.covered} ${t.covered === 1 ? "is" : "are"} covered by open work` : null,
  ].filter(Boolean);
  return (
    <p>
      {how} <b>{saw}</b>
      {notes.length > 0 && ` ${notes.join(", ")}.`}
    </p>
  );
}

function Requirements({
  run,
  details,
}: {
  run: AiRun;
  details: DriftCheckDetails | null | undefined;
}) {
  const [unfolded, setUnfolded] = useState<Set<string>>(new Set());
  if (details === undefined) return null;
  if (details === null) {
    return <p className="text-muted">Couldn't load what the check found.</p>;
  }
  const results = details.results;
  const specs = new Set(results.map((r) => r.spec_id ?? r.spec_title)).size;
  const groups = GROUPS.map((g) => ({
    ...g,
    // A requirement without a verdict is still planned while the check runs.
    label: g.verdict === null && run.status === "running" ? "Planned" : g.label,
    tone: g.verdict === null && run.status === "running" ? ("neutral" as Tone) : g.tone,
    results: results.filter((r) => r.verdict === g.verdict),
  })).filter((g) => g.results.length > 0);
  const toggle = (label: string) =>
    setUnfolded((was) => {
      const next = new Set(was);
      next.has(label) ? next.delete(label) : next.add(label);
      return next;
    });
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-xs font-medium text-muted">
        Requirements{" "}
        {results.length > 0 && (
          <span className="font-normal text-faint">
            {plural(results.length, "requirement", "requirements")} in{" "}
            {plural(specs, "spec", "specs")}
          </span>
        )}
      </h2>
      {results.length === 0 && <p className="text-muted">No requirements reported yet.</p>}
      {groups.map((g) => {
        const open = g.folded ? unfolded.has(g.label) : !unfolded.has(g.label);
        return (
          <div key={g.label} className="flex flex-col gap-0.5">
            <button
              type="button"
              aria-expanded={open}
              onClick={() => toggle(g.label)}
              className="flex items-center gap-2 self-start rounded-full outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              <Pill label={`${g.label} ${g.results.length}`} tone={g.tone} />
            </button>
            {open && (
              <ul className="-mx-3 flex flex-col gap-0.5">
                {g.results.map((r) => (
                  <ResultRow key={`${r.spec_id ?? r.spec_title}/${r.requirement}`} result={r} />
                ))}
              </ul>
            )}
          </div>
        );
      })}
    </section>
  );
}

function ResultRow({ result }: { result: CheckResult }) {
  const { workspace } = useWorkspace();
  const teams = byId(useTeams());
  const issue = useRecords("Issue").find((i) => i.id === result.issue_id);
  const [open, setOpen] = useState(false);
  return (
    <li>
      <div className="flex min-h-9 items-center gap-3 rounded-md px-3 hover:bg-hover">
        <button
          type="button"
          aria-expanded={open}
          disabled={!result.why}
          onClick={() => setOpen(!open)}
          className="min-w-0 flex-1 truncate py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-accent"
        >
          {result.requirement}
          <span className="text-muted"> · {result.spec_title}</span>
        </button>
        {issue && (
          <Link
            to="/$workspace/issue/$identifier"
            params={{ workspace: workspace.url_key, identifier: identifier(issue, teams) }}
            className="shrink-0 font-mono text-xs text-muted hover:text-fg"
          >
            {identifier(issue, teams)}
          </Link>
        )}
      </div>
      {open && result.why && (
        <div className="px-3 pb-3 text-muted">
          <Markdown text={result.why} />
        </div>
      )}
    </li>
  );
}

function SeenIssues({
  filed,
  commented,
  agent,
}: {
  filed: Issue[];
  commented: Issue[];
  agent: boolean;
}) {
  const { workspace } = useWorkspace();
  const teams = byId(useTeams());
  if (!agent) return null;
  const rows = [
    ...filed.map((issue) => ({ what: "Filed", issue })),
    ...commented.map((issue) => ({ what: "Commented", issue })),
  ];
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-xs font-medium text-muted">
        Issues <span className="font-normal text-faint">seen by Forge</span>
      </h2>
      {rows.length === 0 && <p className="text-muted">The check filed and commented on nothing.</p>}
      <ul className="-mx-3 flex flex-col gap-0.5">
        {rows.map(({ what, issue }) => (
          <li key={`${what}/${issue.id}`}>
            <Link
              to="/$workspace/issue/$identifier"
              params={{ workspace: workspace.url_key, identifier: identifier(issue, teams) }}
              className="flex h-9 items-center gap-3 rounded-md px-3 hover:bg-hover"
            >
              <span className="w-20 shrink-0 text-xs text-muted">{what}</span>
              <span className="w-16 shrink-0 font-mono text-xs text-muted">
                {identifier(issue, teams)}
              </span>
              <span className="min-w-0 flex-1 truncate">{issue.title}</span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

const COLUMNS: { key: keyof UsagePart; label: string }[] = [
  { key: "calls", label: "Calls" },
  { key: "input", label: "Input" },
  { key: "cache_writes", label: "Cache writes" },
  { key: "cache_reads", label: "Cache reads" },
  { key: "output", label: "Output" },
];

const NUMBER = new Intl.NumberFormat();

/** A reported number, or that it's unavailable: never a made-up 0. */
function Amount({
  value,
  estimated,
  partial,
}: {
  value: number | null;
  estimated?: boolean;
  /** A total some parts didn't report: at least this much. */
  partial?: boolean;
}) {
  if (value === null) return <span className="text-faint">unavailable</span>;
  const title = [partial && "Some parts didn't report this", estimated && "Estimated"].filter(
    Boolean,
  );
  return (
    <span title={title.join("; ") || undefined}>
      {partial ? "≥" : estimated ? "≈" : ""}
      {NUMBER.format(value)}
    </span>
  );
}

/** Usage as the agent reported it per part, or Forge AI's tokens and cost. */
function Usage({ run }: { run: AiRun }) {
  if (run.trigger !== "agent") {
    if (run.status === "queued") return null;
    return (
      <section className="flex flex-col gap-2">
        <h2 className="text-xs font-medium text-muted">Usage</h2>
        <p className="text-muted">
          {NUMBER.format(run.input_tokens)} input and {NUMBER.format(run.output_tokens)} output
          tokens · ${run.cost_usd.toFixed(2)}
        </p>
      </section>
    );
  }
  const parts = run.usage;
  if (run.status === "running") return null;
  const known = (key: keyof UsagePart) =>
    (parts ?? []).map((p) => p[key]).filter((v): v is number => typeof v === "number");
  const total = (key: keyof UsagePart) => {
    const values = known(key);
    return values.length ? values.reduce((a, b) => a + b, 0) : null;
  };
  const estimated = (parts ?? []).some((p) => p.output_estimated);
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-xs font-medium text-muted">Usage</h2>
      {!parts?.length ? (
        <p className="text-muted">Unavailable: the check didn't report its usage.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-right text-xs tabular-nums">
            <thead className="text-muted">
              <tr>
                <th className="py-1 pr-3 text-left font-medium" />
                {COLUMNS.map((c) => (
                  <th key={c.key} className="py-1 pl-3 font-medium">
                    {c.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {parts.map((p) => (
                <tr key={p.name}>
                  <td className="py-1 pr-3 text-left">{p.name}</td>
                  {COLUMNS.map((c) => (
                    <td key={c.key} className="py-1 pl-3">
                      <Amount
                        value={p[c.key] as number | null}
                        estimated={c.key === "output" && p.output_estimated}
                      />
                    </td>
                  ))}
                </tr>
              ))}
              {parts.length > 1 && (
                <tr className="font-medium">
                  <td className="py-1 pr-3 text-left">Total</td>
                  {COLUMNS.map((c) => (
                    <td key={c.key} className="py-1 pl-3">
                      <Amount
                        value={total(c.key)}
                        estimated={c.key === "output" && estimated}
                        partial={known(c.key).length > 0 && known(c.key).length < parts.length}
                      />
                    </td>
                  ))}
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
