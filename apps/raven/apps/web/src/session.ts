import { useSyncExternalStore } from "react";
import { api, RequestError, setSessionEndedHandler } from "./api";
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
  try {
    signedIn(await api<Me>("GET", "/api/me"));
  } catch (error) {
    if (error instanceof RequestError && error.status === 401) {
      set({ status: "signed-out" });
      return;
    }
    // The server is restarting or out of reach: try again shortly.
    setTimeout(() => void loadSession(), 3000);
  }
}

export function signedIn(me: Me) {
  set({ status: "signed-in", me });
}

setSessionEndedHandler(() => set({ status: "signed-out" }));

export async function signOut(): Promise<void> {
  await api("POST", "/api/auth/logout").catch(() => {});
  set({ status: "signed-out" });
}
