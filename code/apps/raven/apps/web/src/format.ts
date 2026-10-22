/** A duration as people read it: "0.8 ms", "180 ms", "1.2 s". */
export function ms(value: number | null | undefined): string {
  if (value == null || Number.isNaN(value)) return "–";
  if (value >= 1000) return `${(value / 1000).toFixed(1)} s`;
  if (value >= 10) return `${Math.round(value)} ms`;
  return `${value.toFixed(1)} ms`;
}

/** A share as a percentage, with as many digits as it needs to say something. */
export function percent(share: number | null | undefined): string {
  if (share == null) return "–";
  const value = share * 100;
  if (value === 0 || value === 100) return `${value}%`;
  if (value >= 99.95) return "99.9%";
  if (value < 0.1) return "<0.1%";
  return `${value < 10 ? value.toFixed(1) : Math.round(value)}%`;
}

/** Calls per minute: "12", "0.4", "<0.1". */
export function rate(perMinute: number): string {
  if (perMinute === 0) return "0";
  if (perMinute < 0.1) return "<0.1";
  return perMinute < 10 ? perMinute.toFixed(1) : String(Math.round(perMinute));
}

export { ago, lasted } from "@code-villain/web";

/** A clock time: "14:05". */
export function clock(iso: string): string {
  const d = new Date(iso);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** The window in words, for sentences: "the last hour". */
export function windowName(secs: number): string {
  return (
    { 900: "the last 15 minutes", 3600: "the last hour", 86400: "the last day" }[secs] ??
    "the last week"
  );
}
