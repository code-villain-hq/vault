// What differs between the browser and the desktop app. Desktop features
// load only inside the desktop app, so the web bundle doesn't need them.

/** True inside the Tauri desktop app. */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * In the desktop app, links to other sites open in the browser instead of
 * replacing Glyph in its window.
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
