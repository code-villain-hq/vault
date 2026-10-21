import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Edits text locally and saves it shortly after typing stops, or on blur.
 * Changes from others show up while the field isn't being edited. While
 * `hold` is set (say, halfway through choosing whom to mention), only blur
 * saves.
 */
export function useDraft(value: string, save: (value: string) => void, hold = false) {
  const [draft, setDraft] = useState(value);
  const editing = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const latest = useRef({ draft, value, save, hold });
  latest.current = { draft, value, save, hold };

  useEffect(() => {
    if (!editing.current) setDraft(value);
  }, [value]);

  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const { draft, value, save } = latest.current;
    if (draft !== value) save(draft);
  }, []);
  useEffect(() => () => flush(), [flush]);

  return {
    value: draft,
    onChange: (next: string) => {
      editing.current = true;
      setDraft(next);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        if (!latest.current.hold) flush();
      }, 700);
    },
    onBlur: () => {
      editing.current = false;
      flush();
    },
    /** Sets and saves the text at once, as if the field had been left. */
    commit: (next: string) => {
      clearTimeout(timer.current);
      editing.current = false;
      setDraft(next);
      if (next !== latest.current.value) latest.current.save(next);
    },
  };
}
