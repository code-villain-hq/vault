import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useStore } from "../store";

/**
 * Put text on the system clipboard.
 *
 * The browser Clipboard API is unreliable in a Tauri webview (no secure
 * context, and macOS often refuses the call), so this goes through the
 * clipboard plugin that talks to the OS pasteboard directly.
 */
export async function copyText(text: string): Promise<void> {
  await writeText(text);
}

/** Copy, then say so: "Copied <what>", or that the clipboard was out of reach. */
export function copyAndSay(text: string, what: string): void {
  const { toast } = useStore.getState();
  void copyText(text)
    .then(() => toast("success", `Copied ${what}`))
    .catch(() => toast("error", "Could not reach the clipboard"));
}
