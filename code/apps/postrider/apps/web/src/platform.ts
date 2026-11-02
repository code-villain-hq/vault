// What differs between the browser and the desktop app: shared in
// `@code-villain/web`.

export { isDesktop, onAppLink, openExternalLinksInBrowser, openInBrowser } from "@code-villain/web";

/** Saves a mail's file in Downloads and opens it, in the desktop app. */
export async function saveAndOpen(name: string, bytes: Uint8Array): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_file", bytes, { headers: { "x-file-name": encodeURIComponent(name) } });
}
