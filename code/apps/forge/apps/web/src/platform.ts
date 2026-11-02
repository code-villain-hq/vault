// What differs between the browser and the desktop app. Shared openers and
// deep links live in `@code-villain/web`; Forge also badges the dock and
// shows notifications for the inbox.

import { isDesktop } from "@code-villain/web";

export { isDesktop, onAppLink, openExternalLinksInBrowser, openInBrowser } from "@code-villain/web";

/**
 * Shows a native notification. Desktop only; does nothing in the browser.
 * The desktop app shows it (`notify`): the plugin's own never show on macOS.
 */
export async function notify(title: string, body: string) {
  if (!isDesktop) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("notify", { title, body });
}

/** The number on the dock icon; zero clears it. Desktop only. */
export async function setBadge(count: number) {
  if (!isDesktop) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
}
