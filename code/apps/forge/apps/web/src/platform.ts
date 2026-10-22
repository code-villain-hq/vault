// What differs between the browser and the desktop app. Desktop features
// load only inside the desktop app, so the web bundle doesn't need them.

import { isDesktop } from "@code-villain/web";

export { isDesktop };

/** Shows a native notification. Desktop only; does nothing in the browser. */
export async function notify(title: string, body: string) {
  if (!isDesktop) return;
  const { isPermissionGranted, requestPermission, sendNotification } = await import(
    "@tauri-apps/plugin-notification"
  );
  let granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === "granted";
  if (granted) sendNotification({ title, body });
}

/** The number on the dock icon; zero clears it. Desktop only. */
export async function setBadge(count: number) {
  if (!isDesktop) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
}

/**
 * In the desktop app, links to other sites open in the browser instead of
 * replacing Forge in its window.
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
 * Calls `handler` with each `forge://` link the system hands the desktop app
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
