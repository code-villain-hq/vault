// Calling an app's API in a place, and reading it while a page is open.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  apiUrl,
  appConfig,
  authHeaders,
  credentialsFor,
  type Place,
  placeFor,
  rememberToken,
} from "./places";
import { poll } from "./poll";
import { messageOf, UNKNOWN_PROBLEM } from "./problems";

/** What a server says when a request fails. */
type ApiError = { code: string; message: string };

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
  signal?: AbortSignal,
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(apiUrl(path, place), {
      method,
      signal,
      credentials: credentialsFor(place),
      headers: {
        ...authHeaders(place),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new RequestError(0, "offline", `Can't reach ${appConfig().name}. Check your connection.`);
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

/** How often an open page asks again, so it stays current without a reload. */
const REFRESH_MS = 15_000;

/**
 * Reads `path` and keeps reading it while the page is open, from the place
 * its workspace is in. `data` stays while a refresh is on its way, so the
 * page doesn't blink. `offline` says the place can't be reached: what the app
 * heard lives there, not in the local copy.
 */
export function useApi<T>(path: string) {
  const [state, setState] = useState<{
    path: string;
    data?: T;
    error?: string;
    offline?: boolean;
  }>({ path });
  const refresher = useRef<() => Promise<void>>(() => Promise.resolve());
  const load = useCallback(
    async (signal: AbortSignal) => {
      try {
        const data = await api<T>(
          "GET",
          path,
          undefined,
          undefined,
          AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
        );
        if (signal.aborted) return;
        setState({ path, data });
      } catch (e) {
        if (signal.aborted) return;
        const offline = e instanceof RequestError && e.offline;
        setState((s) => ({ ...s, path, error: messageOf(e), offline }));
      }
    },
    [path],
  );
  useEffect(() => {
    const polling = poll(load, () => document.visibilityState === "visible", REFRESH_MS);
    refresher.current = polling.refresh;
    void polling.refresh();
    const online = () => void polling.refresh();
    window.addEventListener("online", online);
    return () => {
      polling.stop();
      window.removeEventListener("online", online);
    };
  }, [load]);
  // A new path starts empty rather than showing the last page's numbers.
  const current = state.path === path;
  return {
    data: current ? state.data : undefined,
    error: current ? state.error : undefined,
    offline: current && Boolean(state.offline),
    reload: useCallback(() => refresher.current(), []),
  };
}
