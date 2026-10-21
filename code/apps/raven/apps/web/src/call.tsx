import { Link, useParams } from "@tanstack/react-router";
import { useApi } from "./api";
import { ago, ms } from "./format";
import {
  NotAvailableOffline,
  PageBody,
  PageHeader,
  useAppName,
  useAppParams,
  useAppPath,
} from "./layout";
import type { CallPage as Call, LogLine, Span } from "./types";
import { ErrorMessage, Section } from "./ui";

/** One call: where its time went, step by step, and what it logged. */
export function CallPage() {
  const params = useAppParams();
  const { trace } = useParams({ strict: false }) as { trace: string };
  const name = useAppName();
  const { data, error, offline, reload } = useApi<Call>(
    useAppPath(`calls/${encodeURIComponent(trace)}`),
  );
  const root = data?.spans[0];
  if (offline) {
    return (
      <NotAvailableOffline
        title={`Call ${trace.slice(0, 12)}`}
        what={`${name}'s calls`}
        onRetry={reload}
      />
    );
  }
  return (
    <>
      <PageHeader
        title={`Call ${trace.slice(0, 12)}`}
        parent={
          <Link to="/$workspace/$app/technical" params={params} className="hover:text-fg">
            Technical
          </Link>
        }
      />
      <PageBody>
        <ErrorMessage>{error}</ErrorMessage>
        {data && root && <CallBody call={data} root={root} />}
      </PageBody>
    </>
  );
}

/** Spans in call order, each with its depth under the root. */
function tree(spans: Span[]): [Span, number][] {
  const children = new Map<string | null, Span[]>();
  const ids = new Set(spans.map((s) => s.span_id));
  for (const span of spans) {
    const parent = span.parent_span_id && ids.has(span.parent_span_id) ? span.parent_span_id : null;
    children.set(parent, [...(children.get(parent) ?? []), span]);
  }
  const out: [Span, number][] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const span of children.get(parent) ?? []) {
      out.push([span, depth]);
      walk(span.span_id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

function CallBody({ call, root }: { call: Call; root: Span }) {
  const params = useAppParams();
  const start = Date.parse(root.started_at);
  const total = Math.max(root.duration_ms, 0.001);
  const failed = call.spans.find((s) => s.is_error && s.error_message);
  return (
    <>
      <section className="flex flex-col gap-2">
        <Link
          to="/$workspace/$app/action"
          params={params}
          search={{ name: root.name, window: 3600 }}
          className="font-mono text-title font-semibold break-all hover:underline"
        >
          {root.name}
        </Link>
        <p className="flex flex-wrap gap-x-4 text-muted tabular-nums">
          <span className="font-semibold text-fg">{ms(root.duration_ms)}</span>
          {root.http_status != null && (
            <span className={root.is_error ? "font-semibold text-danger-ink" : ""}>
              HTTP {root.http_status}
            </span>
          )}
          <span title={root.started_at}>{ago(root.started_at)}</span>
          {root.version && <span className="font-mono">{root.version}</span>}
        </p>
        {failed && <p className="ui-tint-danger rounded-md px-3 py-2">{failed.error_message}</p>}
      </section>

      <Section title="Where the time went" aside={`${call.spans.length} steps`}>
        <ol className="flex flex-col">
          {tree(call.spans).map(([span, depth]) => {
            const offset = ((Date.parse(span.started_at) - start) / total) * 100;
            const width = (span.duration_ms / total) * 100;
            const statement = span.attributes["db.statement"];
            return (
              <li
                key={span.span_id}
                className="ui-row grid grid-cols-5 items-center gap-3 px-2 py-1"
              >
                <div
                  className="col-span-2 flex min-w-0 flex-col"
                  style={{ paddingLeft: depth * 12 }}
                >
                  <span
                    className={`truncate ${span.is_error ? "font-semibold text-danger-ink" : ""} ${span.kind === "db" ? "font-mono text-xs" : ""}`}
                    title={typeof statement === "string" ? statement : span.name}
                  >
                    {typeof statement === "string" ? statement : span.name}
                  </span>
                </div>
                <div className="relative col-span-3 h-4">
                  <div
                    className={`absolute top-1 h-2 rounded-sm ${span.is_error ? "bg-danger" : depth === 0 ? "bg-fg/40" : "bg-fg/25"}`}
                    style={{
                      left: `${Math.min(offset, 99.5)}%`,
                      width: `max(2px, ${Math.min(width, 100 - offset)}%)`,
                    }}
                  />
                  <span
                    className="absolute top-0 text-xs whitespace-nowrap text-muted tabular-nums"
                    style={
                      offset + width > 80
                        ? { right: `${100 - offset}%`, paddingRight: 6 }
                        : { left: `${offset + width}%`, paddingLeft: 6 }
                    }
                  >
                    {ms(span.duration_ms)}
                  </span>
                </div>
              </li>
            );
          })}
        </ol>
      </Section>

      <Section title="Logs" aside={call.logs.length ? undefined : "This call logged nothing"}>
        {call.logs.length > 0 && <Logs lines={call.logs} />}
      </Section>

      {call.logs_before.length > 0 && (
        <Section
          title="Just before"
          aside="The app's last logs in the five minutes before this call"
        >
          <Logs lines={call.logs_before} />
        </Section>
      )}
    </>
  );
}

const LEVEL_INK: Record<string, string> = {
  ERROR: "font-semibold text-danger-ink",
  WARN: "font-semibold text-warn-ink",
};

function Logs({ lines }: { lines: LogLine[] }) {
  return (
    <ol className="flex flex-col rounded-lg bg-surface py-1 font-mono text-xs">
      {lines.map((line) => (
        <li key={`${line.at} ${line.span_id} ${line.message}`} className="flex gap-3 px-3 py-0.5">
          <span className="shrink-0 text-faint tabular-nums">
            {new Date(line.at).toLocaleTimeString([], { hour12: false })}
          </span>
          <span className={`w-12 shrink-0 ${LEVEL_INK[line.level.toUpperCase()] ?? "text-muted"}`}>
            {line.level.toUpperCase()}
          </span>
          <span className="min-w-0 break-words">{line.message}</span>
          {line.target && <span className="ml-auto shrink-0 text-faint">{line.target}</span>}
        </li>
      ))}
    </ol>
  );
}
