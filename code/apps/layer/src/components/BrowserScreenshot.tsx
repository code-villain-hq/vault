import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import { CameraIcon } from "./icons";
import { Spinner } from "./ui";

/** How long the button reads "Copied", as a copy button does (NOTE-7). */
const COPIED_FOR = 1500;

/**
 * The camera in the browser panel's bar (BRW-22): a click puts the tab on
 * the clipboard as a PNG, to paste into an agent's terminal or a chat;
 * ⇧-click saves it in Downloads instead. A toolbar icon has no place of its
 * own for a failure, so it fails in the top bar (NOTE-6).
 */
export function BrowserScreenshot({ taskId, disabled }: { taskId: string; disabled: boolean }) {
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function take(save: boolean) {
    setBusy(true);
    try {
      const saved = await api.browserScreenshot(taskId, save);
      if (saved) {
        toast("success", `Saved “${saved}” in Downloads`);
      } else {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_FOR);
      }
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  const label = "Copy a screenshot of the page (⇧-click saves it in Downloads)";
  return (
    <button
      className={`ui-button ui-icon-button${copied ? " copied" : ""}`}
      title={copied ? "Copied" : label}
      aria-label={copied ? "Copied" : label}
      disabled={disabled || busy}
      onClick={(e) => void take(e.shiftKey)}
    >
      {busy ? <Spinner /> : copied ? "Copied" : <CameraIcon />}
    </button>
  );
}
