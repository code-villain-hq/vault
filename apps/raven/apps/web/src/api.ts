import { useCallback, useEffect, useState } from "react";
import type { ApiError } from "./types";

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
    response = await fetch(path, {
      method,
      credentials: "same-origin",
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new RequestError(0, "offline", "Can't reach Raven. Check your connection.");
  }

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

/** How often an open page asks again, so it stays current without a reload. */
const REFRESH_MS = 15_000;

/**
 * Reads `path` and keeps reading it while the page is open. `data` stays
 * while a refresh is on its way, so the page doesn't blink.
 */
export function useApi<T>(path: string) {
  const [state, setState] = useState<{ path: string; data?: T; error?: string }>({ path });
  const load = useCallback(async () => {
    try {
      const data = await api<T>("GET", path);
      setState({ path, data });
    } catch (e) {
      setState((s) => ({ ...s, path, error: messageOf(e) }));
    }
  }, [path]);
  useEffect(() => {
    void load();
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, REFRESH_MS);
    return () => clearInterval(timer);
  }, [load]);
  // A new path starts empty rather than showing the last page's numbers.
  const current = state.path === path;
  return {
    data: current ? state.data : undefined,
    error: current ? state.error : undefined,
  };
}
