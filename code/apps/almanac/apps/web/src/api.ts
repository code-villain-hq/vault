// The kit's API calls; `./server` names the app before any request.
import "./server";
import { RequestError } from "@code-villain/web";

export { api, messageOf, RequestError, setSessionEndedHandler, useApi } from "@code-villain/web";

/** Statuses worth asking again: signed out (until signed in again), timeouts, rate limits. */
const AGAIN = new Set([401, 408, 425, 429]);

/**
 * Whether a request that failed is worth sending again later: the server
 * wasn't there or couldn't answer, not one that refused it.
 */
export function triedLater(error: unknown): boolean {
  return (
    !(error instanceof RequestError) ||
    error.offline ||
    error.status >= 500 ||
    AGAIN.has(error.status)
  );
}
