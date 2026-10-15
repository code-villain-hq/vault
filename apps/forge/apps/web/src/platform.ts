// What differs between the browser and the desktop app. Desktop features
// load only inside the desktop app, so the web bundle doesn't need them.

/** True inside the Tauri desktop app. */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
