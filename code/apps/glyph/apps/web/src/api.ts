import { messageOf, UNKNOWN_PROBLEM } from "@code-villain/web";
import type { ApiError } from "@glyph/types";
import { apiUrl, authHeaders, credentialsFor, type Place, placeFor, rememberToken } from "./server";

/** A failed API call, with the server's error code and message. */
export class RequestError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "RequestError";
  }

  get offline() {
    return this.status === 0;
  }
}

let onSessionEnded = (_place: Place) => {};

/** What to do when a place says its session has ended (outside of signing in). */
export function setSessionEndedHandler(handler: (place: Place) => void) {
  onSessionEnded = handler;
}

/** Calls the API in a place: the one named, or the one the path belongs to. */
export async function api<T>(
  method: string,
  path: string,
  body?: unknown,
  place = placeFor(path),
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(apiUrl(path, place), {
      method,
      credentials: credentialsFor(place),
      headers: {
        ...authHeaders(place),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new RequestError(0, "offline", "Can't reach Glyph. Check your connection.");
  }
  await rememberToken(response, place);

  if (response.status === 401 && !path.startsWith("/api/auth/")) onSessionEnded(place);
  if (response.status === 204) return undefined as T;
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    const error = data as ApiError | null;
    throw new RequestError(
      response.status,
      error?.code ?? "error",
      error?.message ?? UNKNOWN_PROBLEM,
    );
  }
  return data as T;
}

export { messageOf };
