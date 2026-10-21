const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * How long ago, in words: "just now", "5 minutes ago", "yesterday", then a
 * date ("Oct 3", with the year when it isn't this one), as every Code
 * Villain app says it (studio/design/behaviour.md; the web kit's `ago`). A
 * time a little in the future, from a clock that runs ahead, reads "just
 * now" rather than "in 2 minutes". In English, like the rest of the app's
 * words, whatever language the Mac is set to.
 */
export function ago(when: string | number, now: number = Date.now()): string {
  const t = typeof when === "number" ? when : Date.parse(when);
  if (Number.isNaN(t)) return "";
  const seconds = Math.max(0, (now - t) / 1000);
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return minutes === 1 ? "a minute ago" : `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? "an hour ago" : `${hours} hours ago`;
  const then = new Date(t);
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (then.toDateString() === yesterday.toDateString()) return "yesterday";
  const date = `${MONTHS[then.getMonth()]} ${then.getDate()}`;
  return then.getFullYear() === new Date(now).getFullYear()
    ? date
    : `${date}, ${then.getFullYear()}`;
}

/**
 * How old, as short as it goes: "5m", "3h", "2d", "1w", "4mo", "2y" (TKT-11).
 * For a list where every row has one, so the word "ago" would only repeat.
 */
export function age(when: string | number, now: number = Date.now()): string {
  const t = typeof when === "number" ? when : Date.parse(when);
  if (Number.isNaN(t)) return "";
  const m = Math.max(0, Math.floor((now - t) / 60_000));
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d`;
  if (d < 30) return `${Math.floor(d / 7)}w`;
  if (d < 365) return `${Math.floor(d / 30)}mo`;
  return `${Math.floor(d / 365)}y`;
}
