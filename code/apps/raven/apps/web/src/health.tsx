import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { useApi } from "./api";
import { ago, clock, lasted, ms, percent } from "./format";
import {
  levelWords,
  NotAvailableOffline,
  PageBody,
  PageHeader,
  useAppName,
  useAppParams,
  useAppPath,
} from "./layout";
import type { HealthPage as Health, Level, Slot } from "./types";
import { buttonClass, ErrorMessage, Section, Stat } from "./ui";
import { useIsAdmin } from "./workspace";

const STATUS_CLASS: Record<Level, string> = {
  up: "ui-status",
  warn: "ui-status ui-status-warn",
  down: "ui-status ui-status-danger",
  unknown: "ui-status",
};

/** Level 1, for anyone: is the app up and well, and if not, what to do. */
export function HealthPage() {
  const name = useAppName();
  const { data, error, offline, reload } = useApi<Health>(useAppPath("health"));
  if (offline) {
    return <NotAvailableOffline title="Health" what={`${name}'s health`} onRetry={reload} />;
  }
  return (
    <>
      <PageHeader title="Health" parent={name} />
      <PageBody>
        <ErrorMessage>{error}</ErrorMessage>
        {data && <HealthBody health={data} />}
      </PageBody>
    </>
  );
}

function HealthBody({ health }: { health: Health }) {
  const params = useAppParams();
  const isAdmin = useIsAdmin();
  const { status } = health;
  return (
    <>
      <section className="flex flex-col items-start gap-3">
        <span className={`${STATUS_CLASS[status.level]} px-2.5 py-0.5`}>
          {levelWords(status.level)}
        </span>
        <p className="text-title font-semibold">{status.reason}</p>
        <p className="text-muted">
          {status.down_since && (
            <span title={`Since ${clock(status.down_since)}`}>
              Down for {lasted(status.down_since)}.{" "}
            </span>
          )}
          {status.last_check
            ? `Checked ${ago(status.last_check)}. Raven checks every half minute.`
            : health.health_url
              ? "Raven checks every half minute."
              : `${health.name} has no health check yet.`}
        </p>
        {status.action && (
          <Link
            to="/$workspace/$app/action"
            params={params}
            search={{ name: status.action, window: 900 }}
            className={`${buttonClass("primary")} mt-1 flex`}
          >
            Look into it
          </Link>
        )}
      </section>

      <Section title="Last 24 hours" aside="Each bar is half an hour">
        <Timeline slots={health.slots} />
      </Section>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
        <Stat
          label="Answered its checks"
          value={percent(health.uptime)}
          note="of the last day"
          tone={health.uptime != null && health.uptime < 0.99 ? "danger" : undefined}
        />
        <Stat
          label="A check takes"
          value={ms(health.typical_check_ms)}
          note="typically, when it answers"
        />
        <Stat
          label="Calls"
          value={health.sending || health.is_self ? "Sending" : "Not yet"}
          note={
            health.sending || health.is_self ? (
              <Link to="/$workspace/$app/technical" params={params} className="ui-link">
                See them in Technical
              </Link>
            ) : (
              "Only health checks so far"
            )
          }
        />
      </div>

      {!health.sending && !health.is_self && (
        <p className="rounded-lg bg-surface px-4 py-3 text-muted">
          {health.name} doesn't send its calls yet, so Raven knows whether it answers, not how it is
          doing. {isAdmin ? "Give it a key in " : "An admin gives it a key in "}
          <Link
            to="/$workspace/settings/apps"
            params={{ workspace: params.workspace }}
            className="ui-link"
          >
            Settings › Apps
          </Link>{" "}
          and {isAdmin ? "point" : "points"} its OpenTelemetry exporter at Raven.
        </p>
      )}

      <Section title="Latest checks" aside={health.health_url ?? undefined}>
        {health.recent_checks.length === 0 ? (
          <p className="text-muted">No checks yet.</p>
        ) : (
          <ul className="flex flex-col">
            {health.recent_checks.map((check) => (
              <li
                key={check.checked_at}
                className="ui-row flex items-center gap-4 px-2 py-1.5 tabular-nums"
              >
                <span className="w-12 text-muted">{clock(check.checked_at)}</span>
                <span className={check.ok ? "w-16 text-fg" : "w-16 font-semibold text-danger-ink"}>
                  {check.ok ? "Answered" : "Failed"}
                </span>
                <span className="w-16 text-muted">{ms(check.duration_ms)}</span>
                <span className="truncate text-muted">
                  {check.error ?? (check.status_code ? `HTTP ${check.status_code}` : "")}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Section>
    </>
  );
}

const SLOT_CLASS: Record<Slot["state"], string> = {
  // A calm half hour stays quiet; colour is spent on the ones that need a look.
  ok: "bg-fg/15",
  none: "bg-fg/5",
  warn: "bg-warn",
  down: "bg-danger",
};

function slotWords(slot: Slot) {
  const time = `${clock(slot.start)}–${clock(new Date(Date.parse(slot.start) + 30 * 60_000).toISOString())}`;
  if (slot.state === "none") return `${time}: nothing heard`;
  const parts = [
    `${slot.checks - slot.failed} of ${slot.checks} checks answered`,
    slot.errors > 0 ? `${slot.errors} failed ${slot.errors === 1 ? "call" : "calls"}` : null,
  ].filter(Boolean);
  return `${time}: ${parts.join(", ")}`;
}

/** Half hours as bars: quiet when calm, warn or danger when not. */
function Timeline({ slots }: { slots: Slot[] }) {
  const [hovered, setHovered] = useState<Slot | null>(null);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex h-8 items-stretch gap-0.5">
        {slots.map((slot) => (
          <div
            key={slot.start}
            role="img"
            aria-label={slotWords(slot)}
            onMouseEnter={() => setHovered(slot)}
            onMouseLeave={() => setHovered((h) => (h === slot ? null : h))}
            className={`flex-1 rounded-sm ${SLOT_CLASS[slot.state]} ${hovered === slot ? "opacity-70" : ""}`}
          />
        ))}
      </div>
      <div className="flex justify-between text-xs text-faint">
        {hovered ? (
          <span className="text-fg">{slotWords(hovered)}</span>
        ) : (
          <>
            <span>24 hours ago</span>
            <span>now</span>
          </>
        )}
      </div>
    </div>
  );
}
