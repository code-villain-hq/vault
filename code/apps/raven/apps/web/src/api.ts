import type { ApiError } from "@raven/types";
import { useCallback, useEffect, useState } from "react";
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
    throw new RequestError(0, "offline", "Can't reach Raven. Check your connection.");
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

/** How often an open page asks again, so it stays current without a reload. */
const REFRESH_MS = 15_000;

/**
 * Reads `path` and keeps reading it while the page is open, from the place
 * its workspace is in. `data` stays while a refresh is on its way, so the
 * page doesn't blink. `offline` says the place can't be reached: what Raven
 * heard lives there, not in the local copy.
 */
export function useApi<T>(path: string) {
  const [state, setState] = useState<{
    path: string;
    data?: T;
    error?: string;
    offline?: boolean;
  }>({ path });
  const load = useCallback(async () => {
    try {
      const data = await api<T>("GET", path);
      setState({ path, data });
    } catch (e) {
      const offline = e instanceof RequestError && e.offline;
      setState((s) => ({ ...s, path, error: messageOf(e), offline }));
    }
  }, [path]);
  useEffect(() => {
    void load();
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, REFRESH_MS);
    window.addEventListener("online", load);
    return () => {
      clearInterval(timer);
      window.removeEventListener("online", load);
    };
  }, [load]);
  // A new path starts empty rather than showing the last page's numbers.
  const current = state.path === path;
  return {
    data: current ? state.data : undefined,
    error: current ? state.error : undefined,
    offline: current && Boolean(state.offline),
    reload: load,
  };
}
