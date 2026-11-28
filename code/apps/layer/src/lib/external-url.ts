/** A permalink must not be relative, executable, or carry user information. */
export function externalHttpUrl(raw: string | null): string | null {
  if (typeof raw !== "string" || !/^https?:\/\//i.test(raw)) return null;
  try {
    const url = new URL(raw);
    return url.username || url.password ? null : url.href;
  } catch {
    return null;
  }
}
