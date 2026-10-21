import { useSyncExternalStore } from "react";
import { api, RequestError, setSessionEndedHandler } from "./api";
import { forgetToken, hasSession, loadToken } from "./server";
import type { Me } from "./types";

export type Session =
  | { status: "loading" }
  | { status: "signed-out" }
  | { status: "signed-in"; me: Me };

let session: Session = { status: "loading" };
const listeners = new Set<() => void>();

function set(next: Session) {
  session = next;
  for (const listener of listeners) listener();
}

export function useSession(): Session {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => session,
  );
}

/** Asks the server who is signed in. */
export async function loadSession(): Promise<void> {
  await loadToken();
  if (!hasSession()) {
    // The packaged app hasn't signed in to a server yet.
    set({ status: "signed-out" });
    return;
  }
  try {
    signedIn(await api<Me>("GET", "/api/me"));
  } catch (error) {
    if (error instanceof RequestError && error.status === 401) {
      ended();
      return;
    }
    // The server is restarting or out of reach: try again shortly.
    setTimeout(() => void loadSession(), 3000);
  }
}

export function signedIn(me: Me) {
  set({ status: "signed-in", me });
}

/** The session ended on the server, or here. */
function ended() {
  void forgetToken();
  set({ status: "signed-out" });
}

setSessionEndedHandler(ended);

export async function signOut(): Promise<void> {
  await api("POST", "/api/auth/logout").catch(() => {});
  ended();
}
