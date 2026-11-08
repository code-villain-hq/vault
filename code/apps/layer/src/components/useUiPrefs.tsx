import { useRef, useState } from "react";
import { api } from "../lib/api";
import type { UiPrefs } from "../lib/types";
import { useStore } from "../store";
import { ErrorMessage, useFailure } from "./ui";

/**
 * The app's prefs (`config/ui.rs`) as a Settings page changes them: Appearance,
 * Notifications and Agents each hold some of them, and save them alike.
 *
 * `at` names the setting a save is for, so a failed one says so under that
 * setting (NOTE-6), not at the top of the page.
 */
export function useUiPrefs<Place extends string>() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const failure = useFailure();
  const [failedAt, setFailedAt] = useState<Place | null>(null);
  const failedUnder = (at: Place) =>
    failedAt === at && <ErrorMessage>{failure.error}</ErrorMessage>;

  /**
   * What the sliders say before it is saved.
   *
   * Bound straight to the stored value, a slider only moved once the save
   * came back, so the thumb jumped back between steps of a drag — and every
   * step was a save, rewriting the whole config file.
   */
  const [uiDraft, setUiDraft] = useState<Partial<UiPrefs>>({});
  const uiDraftRef = useRef(uiDraft);
  const uiSaveTimer = useRef<number | undefined>(undefined);
  const uiSaves = useRef(0);
  const ui = settings ? { ...settings.ui, ...uiDraft } : null;

  /** Save a change to the prefs, on top of whatever a slider has not saved yet. */
  async function saveUi(patch: Partial<UiPrefs>, at: Place) {
    // Read now, not from the render that scheduled this: a slider's save
    // runs after a pause, and anything switched meanwhile must survive it.
    const stored = useStore.getState().settings?.ui;
    if (!stored) return;
    // This save carries the draft, so a slider's pending one is not needed
    // — and firing after it would put the slider's older value back.
    window.clearTimeout(uiSaveTimer.current);
    uiDraftRef.current = { ...uiDraftRef.current, ...patch };
    setUiDraft(uiDraftRef.current);
    const n = ++uiSaves.current;
    failure.clear();
    try {
      await api.setUiPrefs({ ...stored, ...uiDraftRef.current });
      await refreshSettings();
      if (n === uiSaves.current) {
        uiDraftRef.current = {};
        setUiDraft({});
      }
    } catch (e) {
      setFailedAt(at);
      failure.fail(e);
    }
  }

  /** A slider: shown at once, saved once it stops moving. */
  function slideUi(patch: Partial<UiPrefs>, at: Place) {
    // Counts as a newer change, so a save already on its way does not clear
    // the draft when it lands — that threw away a value still waiting on its
    // timer, and the thumb jumped back.
    uiSaves.current += 1;
    uiDraftRef.current = { ...uiDraftRef.current, ...patch };
    setUiDraft(uiDraftRef.current);
    window.clearTimeout(uiSaveTimer.current);
    uiSaveTimer.current = window.setTimeout(() => void saveUi({}, at), 250);
  }

  return { ui, saveUi, slideUi, failedUnder };
}
