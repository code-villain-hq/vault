import type { ApiError } from "@glyph/types";
import { apiUrl, authHeaders, credentials, rememberToken } from "./server";

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

let onSessionEnded = () => {};

/** What to do when the server says the session has ended (outside of signing in). */
export function setSessionEndedHandler(handler: () => void) {
  onSessionEnded = handler;
}

export async function api<T>(method: string, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(apiUrl(path), {
      method,
      credentials,
      headers: {
        ...authHeaders(),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new RequestError(0, "offline", "Can't reach Glyph. Check your connection.");
  }
  await rememberToken(response);

  if (response.status === 401 && !path.startsWith("/api/auth/")) onSessionEnded();
  if (response.status === 204) return undefined as T;
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    const error = data as ApiError | null;
    throw new RequestError(
      response.status,
      error?.code ?? "error",
      error?.message ?? response.statusText,
    );
  }
  return data as T;
}

/** A message to show for a failed request. */
export function messageOf(error: unknown): string {
  const message = error instanceof Error ? error.message : "Something went wrong.";
  return message.charAt(0).toUpperCase() + message.slice(1);
}
