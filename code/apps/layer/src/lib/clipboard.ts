import { useCallback, useEffect, useRef, useState } from "react";
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

const NO_CLIPBOARD = "Couldn't reach the clipboard. Try again.";

/**
 * Copy from a menu item, then say so: "Copied <what>". The menu closes as it
 * is chosen, so there is no button left to read "Copied" (NOTE-7).
 */
export function copyAndSay(text: string, what: string): void {
  const { toast, fail } = useStore.getState();
  void copyText(text)
    .then(() => toast("success", `Copied ${what}`))
    .catch(() => fail(NO_CLIPBOARD));
}

/** How long a copy button reads "Copied". */
const COPIED_FOR = 1500;

/**
 * A copy button's state: `copied` is true for a moment after `copy`, for the
 * button to read "Copied" instead of raising a toast (NOTE-7,
 * studio/design/behaviour.md). A clipboard out of reach goes to the top
 * bar's status: an icon button has no room for it (NOTE-6).
 */
export function useCopied() {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = useCallback((text: string) => {
    void copyText(text)
      .then(() => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_FOR);
      })
      .catch(() => useStore.getState().fail(NO_CLIPBOARD));
  }, []);
  return { copied, copy };
}
