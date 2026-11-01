// What differs between the browser and the desktop app. Shared openers and
// deep links live in `@code-villain/web`; Forge also badges the dock and
// shows notifications for the inbox.

import { isDesktop } from "@code-villain/web";

export {
  isDesktop,
  onAppLink,
  openExternalLinksInBrowser,
  openInBrowser,
} from "@code-villain/web";

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
