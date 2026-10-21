import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { useApi } from "./api";
import { CallsChart, DurationChart } from "./charts";
import { ago, clock, ms, percent, rate, windowName } from "./format";
import { PageBody, PageHeader, useAppKey } from "./layout";
import { Errors } from "./technical";
import type { ActionPage as Action } from "./types";
import { ErrorMessage, Section, Stat, WindowPicker } from "./ui";

export type ActionSearch = { name?: string; window?: number };

/** One action up close: how its speed and failures moved, and its slowest calls. */
export function ActionPage() {
  const app = useAppKey();
  const { name = "", window = 3600 } = useSearch({ strict: false }) as ActionSearch;
  const navigate = useNavigate();
  const { data, error } = useApi<Action>(
    `/api/apps/${app}/action?name=${encodeURIComponent(name)}&window=${window}`,
  );
  return (
    <>
      <PageHeader
        title={name}
        parent={
          <Link to="/$app/technical" params={{ app }} search={{ window }} className="hover:text-fg">
            Technical
          </Link>
        }
      >
        <WindowPicker
          value={window}
          onChange={(secs) =>
            void navigate({ to: "/$app/action", params: { app }, search: { name, window: secs } })
          }
        />
      </PageHeader>
      <PageBody>
        <ErrorMessage>{error}</ErrorMessage>
        {data && <ActionBody app={app} page={data} />}
      </PageBody>
    </>
  );
}

function ActionBody({ app, page }: { app: string; page: Action }) {
  const { stats } = page;
  if (!stats) {
    return (
      <p className="text-muted">
        No calls to {page.name} in {windowName(page.window_secs)}.
      </p>
    );
  }
  const bucket =
    page.bucket_secs === 60 ? "minute" : page.bucket_secs === 900 ? "15 minutes" : "hour";
  return (
    <>
      <h2 className="font-mono text-title font-semibold break-all">{page.name}</h2>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat
          label="Calls a minute"
          value={rate(stats.calls_per_min)}
          note={`${stats.calls} calls`}
        />
        <Stat
          label="Failed"
          value={percent(stats.error_rate)}
          note={stats.errors ? `${stats.errors} ${stats.errors === 1 ? "call" : "calls"}` : "none"}
          tone={stats.errors ? "danger" : undefined}
        />
        <Stat label="Typical call" value={ms(stats.p50_ms)} note="p50" />
        <Stat
          label="Slow end"
          value={ms(stats.p95_ms)}
          note={
            stats.normal_p95_ms != null
              ? `usually ${ms(stats.normal_p95_ms)} this week`
              : "no week to compare yet"
          }
          tone={stats.slower_than_usual ? "warn" : undefined}
        />
      </div>

      <Section title="How long calls took" aside={`Each point is a ${bucket}`}>
        <DurationChart points={page.series} />
      </Section>

      <Section title="Calls" aside={`Each bar is a ${bucket}`}>
        <CallsChart points={page.series} />
      </Section>

      {page.errors.length > 0 && (
        <Section title="Errors">
          <Errors app={app} errors={page.errors} />
        </Section>
      )}

      <Section title="Slowest calls" aside="Open one to see where its time went">
        <ul className="flex flex-col">
          {page.slowest.map((call) => (
            <li key={call.trace_id}>
              <Link
                to="/$app/calls/$trace"
                params={{ app, trace: call.trace_id }}
                className="ui-row flex items-center gap-4 px-2 py-1.5 tabular-nums"
              >
                <span className="w-16 font-semibold">{ms(call.duration_ms)}</span>
                <span
                  className={`w-16 ${call.is_error ? "font-semibold text-danger-ink" : "text-muted"}`}
                >
                  {call.http_status ?? (call.is_error ? "Failed" : "")}
                </span>
                <span className="text-muted" title={call.started_at}>
                  {clock(call.started_at)} · {ago(call.started_at)}
                </span>
                <span className="ml-auto truncate font-mono text-xs text-faint">
                  {call.trace_id.slice(0, 12)}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      </Section>
    </>
  );
}
