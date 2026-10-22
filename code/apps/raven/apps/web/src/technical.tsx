import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { useApi } from "./api";
import { ago, lasted, ms, percent, rate, windowName } from "./format";
import {
  NotAvailableOffline,
  PageBody,
  PageHeader,
  useAppName,
  useAppParams,
  useAppPath,
} from "./layout";
import type { ActionRow, ErrorGroup, TechnicalPage as Technical } from "./types";
import { ErrorMessage, Section, Stat, WindowPicker } from "./ui";

export type WindowSearch = { window?: number };

/** Level 2, for developers: how stable and fast each action is, and what failed. */
export function TechnicalPage() {
  const params = useAppParams();
  const { window = 3600 } = useSearch({ strict: false }) as WindowSearch;
  const navigate = useNavigate();
  const name = useAppName();
  const { data, error, offline, reload } = useApi<Technical>(
    `${useAppPath("technical")}?window=${window}`,
  );
  if (offline) {
    return <NotAvailableOffline title="Technical" what={`${name}'s calls`} onRetry={reload} />;
  }
  return (
    <>
      <PageHeader title="Technical" parent={name}>
        <WindowPicker
          value={window}
          onChange={(secs) =>
            void navigate({ to: "/$workspace/$app/technical", params, search: { window: secs } })
          }
        />
      </PageHeader>
      <PageBody>
        <ErrorMessage>{error}</ErrorMessage>
        {data && <TechnicalBody page={data} />}
      </PageBody>
    </>
  );
}

function TechnicalBody({ page }: { page: Technical }) {
  const window = page.window_secs;
  if (page.actions.length === 0) {
    return (
      <p className="text-muted">
        No calls in {windowName(window)}. Calls show here once {page.name} sends them, or pick a
        longer window.
      </p>
    );
  }
  const calls = page.actions.reduce((n, a) => n + a.calls, 0);
  const failed = page.actions.reduce((n, a) => n + a.errors, 0);
  const slower = page.actions.filter((a) => a.slower_than_usual);
  const slowest = page.actions.reduce((worst, a) => (a.p95_ms > worst.p95_ms ? a : worst));
  const current = page.versions[0];
  return (
    <>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat label="Calls a minute" value={rate(calls / (window / 60))} note={`${calls} calls`} />
        <Stat
          label="Failed"
          value={percent(failed / calls)}
          note={failed === 0 ? "none" : `${failed} ${failed === 1 ? "call" : "calls"}`}
          tone={failed > 0 ? "danger" : undefined}
        />
        <Stat
          label="Slower than usual"
          value={slower.length}
          note={slower.length ? slower.map((a) => a.name).join(", ") : "against each one's week"}
          tone={slower.length ? "warn" : undefined}
        />
        <Stat
          label="Running"
          value={<span className="font-mono">{current?.version ?? "–"}</span>}
          note={current ? `for ${lasted(current.first_seen)}` : undefined}
        />
      </div>

      {page.errors.length > 0 && (
        <Section title="Errors" aside={`In ${windowName(window)}, latest first`}>
          <Errors errors={page.errors} />
        </Section>
      )}

      <Section
        title="Actions"
        aside={`Worst first · slowest: ${slowest.name}, ${ms(slowest.p95_ms)}`}
      >
        <Actions actions={page.actions} window={window} />
      </Section>
    </>
  );
}

export function Errors({ errors }: { errors: ErrorGroup[] }) {
  const params = useAppParams();
  return (
    <ul className="flex flex-col gap-1">
      {errors.map((group) => (
        <li key={`${group.action} ${group.message}`}>
          <Link
            to="/$workspace/$app/calls/$trace"
            params={{ ...params, trace: group.last_trace_id }}
            className="ui-row flex flex-col gap-0.5 px-3 py-2"
          >
            <span className="flex items-baseline gap-3">
              <span className="truncate font-semibold text-danger-ink">{group.message}</span>
              <span className="ml-auto shrink-0 text-xs tabular-nums text-muted">
                {group.count} {group.count === 1 ? "time" : "times"}
              </span>
            </span>
            <span className="flex gap-3 text-xs text-muted">
              <span className="truncate font-mono">{group.action}</span>
              <span className="ml-auto shrink-0">
                last {ago(group.last_seen)} · first {ago(group.first_seen)}
                {group.first_version && (
                  <>
                    {" "}
                    in <span className="font-mono">{group.first_version}</span>
                  </>
                )}
              </span>
            </span>
          </Link>
        </li>
      ))}
    </ul>
  );
}

function Actions({ actions, window }: { actions: ActionRow[]; window: number }) {
  const params = useAppParams();
  const cell = "px-3 py-2 text-right tabular-nums";
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-xl border-collapse">
        <thead>
          <tr className="text-xs text-muted">
            <th className="px-3 py-1.5 text-left font-medium">Action</th>
            <th className="px-3 py-1.5 text-right font-medium">Calls/min</th>
            <th className="px-3 py-1.5 text-right font-medium">Failed</th>
            <th className="px-3 py-1.5 text-right font-medium">Typical</th>
            <th className="px-3 py-1.5 text-right font-medium">Slow end</th>
            <th className="px-3 py-1.5 text-right font-medium">Usual slow end</th>
          </tr>
        </thead>
        <tbody>
          {actions.map((action) => (
            <tr key={action.name} className="ui-row">
              <td className="px-3 py-2">
                <Link
                  to="/$workspace/$app/action"
                  params={params}
                  search={{ name: action.name, window }}
                  className="font-mono whitespace-nowrap hover:underline"
                  title={action.name}
                >
                  {action.name}
                </Link>
              </td>
              <td className={cell}>{rate(action.calls_per_min)}</td>
              <td
                className={`${cell} ${action.errors ? "font-semibold text-danger-ink" : "text-muted"}`}
              >
                {action.errors ? percent(action.error_rate) : "–"}
              </td>
              <td className={cell}>{ms(action.p50_ms)}</td>
              <td
                className={`${cell} ${action.slower_than_usual ? "font-semibold text-warn-ink" : ""}`}
              >
                {ms(action.p95_ms)}
              </td>
              <td className={`${cell} text-muted`}>{ms(action.normal_p95_ms)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-2 px-3 text-xs text-faint">
        Typical is the median call; the slow end is the slowest 5%. Usual is the last 7 days.
      </p>
    </div>
  );
}
