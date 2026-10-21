/** behaviour.md: an unknown problem says so, then what to do. */
export const UNKNOWN_PROBLEM = "Something went wrong. Try again.";

/** A message to show for a failed request or action, as a sentence. */
export function messageOf(error: unknown): string {
  const message = error instanceof Error && error.message ? error.message : UNKNOWN_PROBLEM;
  return message.charAt(0).toUpperCase() + message.slice(1);
}
