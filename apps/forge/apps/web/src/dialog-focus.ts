import { useEffect, useRef } from "react";

/** Remember the opener before a form's React autoFocus runs during mount. */
export function useDialogFocus(open: boolean) {
  const opener = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (open) return;
    const remember = (event: Event) => {
      if (!(event.target instanceof Element)) return;
      if (event.target.closest('.app-dialog, [role="menu"], [role="alertdialog"]')) return;
      const target =
        event.type === "pointerdown"
          ? event.target.closest<HTMLElement>(
              "button, a[href], input, textarea, select, [tabindex]",
            )
          : event.target;
      if (
        target instanceof HTMLElement &&
        target !== document.body &&
        target !== document.documentElement
      ) {
        opener.current = target;
      }
    };
    document.addEventListener("focusin", remember, true);
    document.addEventListener("pointerdown", remember, true);
    return () => {
      document.removeEventListener("focusin", remember, true);
      document.removeEventListener("pointerdown", remember, true);
    };
  }, [open]);
  return opener;
}
