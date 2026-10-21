// What differs between the browser and the desktop app. Desktop features
// load only inside the desktop app, so the web bundle doesn't need them.

/** True inside the Tauri desktop app. */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * In the desktop app, links to other sites open in the browser instead of
 * replacing Layer in its window.
 */
export function openExternalLinksInBrowser() {
  if (!isDesktop) return;
  document.addEventListener("click", (event) => {
    const link = (event.target as Element | null)?.closest?.("a[href]");
    if (!(link instanceof HTMLAnchorElement) || event.defaultPrevented) return;
    const url = new URL(link.href, location.href);
    if (!["http:", "https:", "mailto:"].includes(url.protocol) || url.origin === location.origin) {
      return;
    }
    event.preventDefault();
    void import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl(url.href));
  });
}

/** Opens a page in the browser. Desktop only. */
export async function openInBrowser(url: string) {
  if (!isDesktop) return;
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/**
 * Calls `handler` with each `layer://` link the system hands the desktop app
 * while it runs; returns a function that stops listening.
 */
export function onAppLink(handler: (url: URL) => void): () => void {
  if (!isDesktop) return () => {};
  let stop: (() => void) | undefined;
  let stopped = false;
  void import("@tauri-apps/plugin-deep-link").then(({ onOpenUrl }) =>
    onOpenUrl((urls) => {
      for (const url of urls) {
        try {
          handler(new URL(url));
        } catch {
          // Not a link we can read.
        }
      }
    }).then((unlisten) => {
      if (stopped) unlisten();
      else stop = unlisten;
    }),
  );
  return () => {
    stopped = true;
    stop?.();
  };
}
