import type { ReactNode } from "react";

export {
  AuthScreen,
  Button,
  buttonClass,
  ErrorMessage,
  Logo,
  Section,
  Splash,
  TextField,
} from "@code-villain/web";

/** One number with its label: the headline figures of a page. */
export function Stat({
  label,
  value,
  note,
  tone,
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  /** Status ink, only when the number needs a look. */
  tone?: "warn" | "danger";
}) {
  const ink = tone === "danger" ? "text-danger-ink" : tone === "warn" ? "text-warn-ink" : "text-fg";
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg bg-surface px-4 py-3">
      <span className="text-xs text-muted">{label}</span>
      <span className={`text-title font-semibold tabular-nums ${ink}`}>{value}</span>
      {note && <span className="truncate text-xs text-faint">{note}</span>}
    </div>
  );
}

/** The window a page looks at, as the shared segmented control. */
export function WindowPicker({
  value,
  onChange,
}: {
  value: number;
  onChange: (secs: number) => void;
}) {
  const windows: [number, string][] = [
    [900, "15m"],
    [3600, "1h"],
    [86_400, "24h"],
    [604_800, "7d"],
  ];
  return (
    <fieldset className="ui-segmented" aria-label="Time window">
      {windows.map(([secs, label]) => (
        <button
          key={secs}
          type="button"
          className="ui-segment"
          aria-pressed={value === secs}
          onClick={() => onChange(secs)}
        >
          {label}
        </button>
      ))}
    </fieldset>
  );
}
