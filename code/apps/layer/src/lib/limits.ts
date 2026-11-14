/**
 * How the sidebar's corner says each agent plan's limits (§27). The types,
 * as `limits.rs` keeps them, are mirrored in `types-limits.ts`.
 */
import { ago } from "./time";
import type { PlanLimits, PlanWindow } from "./types-limits";

const NAMES: Record<string, string> = { claude: "Claude", openai: "OpenAI" };

export function providerName(provider: string): string {
  return NAMES[provider] ?? provider;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** How long until `at`, in the two largest units: `3d 4h`, `2h 13m`, `42m`, `<1m`. */
export function timeLeft(at: number, now: number): string {
  const ms = at - now;
  if (ms < MINUTE) return "<1m";
  const d = Math.floor(ms / DAY);
  const h = Math.floor((ms % DAY) / HOUR);
  const m = Math.floor((ms % HOUR) / MINUTE);
  if (d > 0) return h > 0 ? `${d}d ${h}h` : `${d}d`;
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  return `${m}m`;
}

export type LimitTone = "ok" | "warn" | "danger" | "muted";

export interface WindowView {
  /** `5h` or `wk`. */
  label: string;
  /** `42%`, or `reset` once the window has started over since it was read. */
  used: string;
  /**
   * Time to its reset, once the window is amber or red: below that, when it
   * starts over does not matter yet, and at the sidebar's usual width every
   * countdown was cut short. The hover has every reset time.
   */
  left: string | null;
  /** Warn from 80%, with a fifth left; danger from 95%. */
  tone: LimitTone;
}

/**
 * A window as it stands at `now`. Past its reset, what was read no longer
 * holds and what is used now is not known until the provider says again, so
 * it reads `reset` rather than an old number (LIMIT-5).
 */
export function windowView(label: string, w: PlanWindow | null, now: number): WindowView | null {
  if (!w) return null;
  if (w.resets_at !== null && w.resets_at <= now)
    return { label, used: "reset", left: null, tone: "muted" };
  const pct = Math.round(w.used_percent);
  const tone = pct >= 95 ? "danger" : pct >= 80 ? "warn" : "ok";
  return {
    label,
    used: `${pct}%`,
    left: w.resets_at === null || tone === "ok" ? null : timeLeft(w.resets_at, now),
    tone,
  };
}

/** A provider's windows, the five hours first; none when it said neither. */
export function limitWindows(l: PlanLimits, now: number): WindowView[] {
  return [windowView("5h", l.session, now), windowView("wk", l.weekly, now)].filter(
    (w): w is WindowView => w !== null,
  );
}

/** The hover text: what each window is, exactly when it resets, and how old the reading is. */
export function limitsTitle(l: PlanLimits, now: number): string {
  const when = (at: number) =>
    new Date(at).toLocaleString(undefined, {
      weekday: "short",
      hour: "numeric",
      minute: "2-digit",
    });
  const say = (name: string, w: PlanWindow | null) => {
    if (!w) return null;
    if (w.resets_at !== null && w.resets_at <= now) return `${name}: reset ${when(w.resets_at)}`;
    const resets = w.resets_at === null ? "" : `, resets ${when(w.resets_at)}`;
    return `${name}: ${Math.round(w.used_percent)}% used${resets}`;
  };
  return [
    `${providerName(l.provider)} plan, read ${ago(l.read_at, now)}`,
    say("5-hour window", l.session),
    say("Week", l.weekly),
    l.provider === "openai" ? "Click to ask OpenAI again." : "Updates as Claude agents reply.",
  ]
    .filter(Boolean)
    .join("\n");
}
