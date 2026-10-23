import { useEffect, useState } from "react";
import { appConfig, bundled } from "./places";

export type SignInProviders = { oidc: boolean; company?: string | null; error?: string };

/** A failed request says nothing about the server's sign-in configuration. */
export function discoverProviders(
  server: string,
  receive: (providers: SignInProviders) => void,
  retryMs = 2000,
  events: EventTarget | undefined = typeof window === "undefined" ? undefined : window,
): () => void {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let busy = false;
  const read = async () => {
    if (busy || controller.signal.aborted) return;
    clearTimeout(timer);
    busy = true;
    try {
      const response = await fetch(`${server}/api/auth/providers`, {
        signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]),
      });
      if (!response.ok) throw new Error("provider discovery failed");
      const providers = (await response.json()) as SignInProviders;
      if (typeof providers.oidc !== "boolean") throw new Error("not an app server");
      if (!controller.signal.aborted) receive(providers);
    } catch {
      if (!controller.signal.aborted) {
        receive({ oidc: false, error: `Can't reach ${appConfig().name}. Check your connection.` });
        timer = setTimeout(() => void read(), retryMs);
      }
    } finally {
      busy = false;
    }
  };
  const online = () => void read();
  events?.addEventListener("online", online);
  void read();
  return () => {
    controller.abort();
    clearTimeout(timer);
    events?.removeEventListener("online", online);
  };
}

export function useSignInProviders(server: string) {
  const [providers, setProviders] = useState<SignInProviders | null>(null);
  useEffect(() => {
    setProviders(null);
    if (bundled && !server) {
      setProviders({ oidc: false });
      return;
    }
    return discoverProviders(server, setProviders);
  }, [server]);
  return providers;
}
