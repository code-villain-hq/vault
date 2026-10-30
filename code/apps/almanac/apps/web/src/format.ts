// Dates as people read them. A reminder's date is a day where you are
// (`YYYY-MM-DD`), never a moment, so these work on local days.

export { ago } from "@code-villain/web";

/** A local day as `YYYY-MM-DD`. */
export function dayKey(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** The local day a `YYYY-MM-DD` names, at midnight. */
export function fromKey(key: string): Date {
  const [y = 1970, m = 1, d = 1] = key.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function today(): string {
  return dayKey(new Date());
}

export function addDays(key: string, days: number): string {
  const date = fromKey(key);
  date.setDate(date.getDate() + days);
  return dayKey(date);
}

/** Days from `from` to `to`; negative when `to` is earlier. */
export function daysBetween(from: string, to: string): number {
  return Math.round((fromKey(to).getTime() - fromKey(from).getTime()) / 86_400_000);
}

/** The Monday of the week `key` is in, and its seven days. */
export function weekOf(key: string): string[] {
  const date = fromKey(key);
  const monday = addDays(key, -((date.getDay() + 6) % 7));
  return Array.from({ length: 7 }, (_, i) => addDays(monday, i));
}

const month = (date: Date) => date.toLocaleDateString("en", { month: "short" });

/** "Oct 26 – Nov 1". */
export function weekWords(days: string[]): string {
  const first = fromKey(days[0] ?? today());
  const last = fromKey(days.at(-1) ?? today());
  const end =
    first.getMonth() === last.getMonth() ? `${last.getDate()}` : `${month(last)} ${last.getDate()}`;
  return `${month(first)} ${first.getDate()} – ${end}`;
}

/** "Mon 26". */
export function weekdayWords(key: string): string {
  const date = fromKey(key);
  return `${date.toLocaleDateString("en", { weekday: "short" })} ${date.getDate()}`;
}

/** "Nov 2". */
export function dateWords(key: string): string {
  const date = fromKey(key);
  return `${month(date)} ${date.getDate()}`;
}

/** When a reminder is due, from today: "today", "tomorrow", "Thursday", "Nov 2", "3 days ago". */
export function dueWords(key: string, from = today()): string {
  const days = daysBetween(from, key);
  if (days === 0) return "today";
  if (days === 1) return "tomorrow";
  if (days === -1) return "yesterday";
  if (days < 0) return `${-days} days ago`;
  if (days < 7) return fromKey(key).toLocaleDateString("en", { weekday: "long" });
  return dateWords(key);
}

/** How often: "every day", "every week", "every 10 days", "once". */
export function rhythmWords(every: number | null): string {
  if (!every) return "once";
  if (every === 1) return "every day";
  if (every === 7) return "every week";
  if (every % 7 === 0) return `every ${every / 7} weeks`;
  return `every ${every} days`;
}

/** A time of day from the server's `HH:MM:SS`: "9:00". */
export function timeWords(at: string | null): string | null {
  if (!at) return null;
  const [h, m] = at.split(":");
  return `${Number(h)}:${m}`;
}

/** "1,2 Mio.", "900": an amount in the reader's numbers, short past a million. */
export function amountWords(amount: number): string {
  return new Intl.NumberFormat(undefined, {
    notation: Math.abs(amount) >= 1e6 ? "compact" : "standard",
    maximumFractionDigits: 1,
  }).format(amount);
}
