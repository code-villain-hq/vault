import type { ReactNode } from "react";
import { THEMES, type Theme } from "../lib/theme";
import type { ThemeChoice } from "../lib/types";
import { Field } from "./ui";
import { useUiPrefs } from "./useUiPrefs";

/** Settings → Appearance: how the app looks. */
export function AppearanceSettings() {
  const { ui, saveUi, slideUi, failedUnder } = useUiPrefs<
    "theme" | "scale" | "terminal-text" | "conversation-text"
  >();

  if (!ui) return null;
  return (
    <>
      <Field
        label="Theme"
        hint="Match the Mac follows the Mac between Nightshade's night and day. In a terminal an agent keeps its own colours: change them there (Claude Code: /theme)."
      >
        <ThemePicker value={ui.theme} onPick={(theme) => void saveUi({ theme }, "theme")} />
        {failedUnder("theme")}
      </Field>

      <Field
        label={`Interface scale — ${Math.round(ui.scale * 100)}%`}
        hint="Everything but terminal text, which has its own size below."
      >
        <input
          type="range"
          min={0.8}
          max={1.6}
          step={0.05}
          value={ui.scale}
          onChange={(e) => slideUi({ scale: Number(e.target.value) }, "scale")}
        />
        <div className="ui-segmented" style={{ marginTop: 6 }}>
          {[0.9, 1.0, 1.15, 1.3, 1.45].map((v) => (
            <button
              key={v}
              className="ui-segment"
              aria-pressed={Math.abs(ui.scale - v) < 0.001}
              onClick={() => void saveUi({ scale: v }, "scale")}
            >
              {Math.round(v * 100)}%
            </button>
          ))}
        </div>
        {failedUnder("scale")}
      </Field>

      <TextSize
        label="Terminal text"
        hint="Running panes re-fit at once."
        value={ui.terminal_font_size}
        onChange={(v) => slideUi({ terminal_font_size: v }, "terminal-text")}
        error={failedUnder("terminal-text")}
      />
      <TextSize
        label="Conversation text"
        hint="Agents running as a conversation. Open ones change at once."
        value={ui.conversation_font_size}
        onChange={(v) => slideUi({ conversation_font_size: v }, "conversation-text")}
        error={failedUnder("conversation-text")}
      />
    </>
  );
}

/**
 * The themes as small pictures of the app, each drawn in its own tokens:
 * three buttons named Dark, Light and Match the Mac said nothing of what a
 * theme looks like, and the names stop saying it once there are more.
 */
function ThemePicker({
  value,
  onPick,
}: {
  value: ThemeChoice;
  onPick: (theme: ThemeChoice) => void;
}) {
  const tile = (id: ThemeChoice, name: string, shot: ReactNode, note?: string) => (
    <button
      key={id}
      className={`theme-tile${value === id ? " on" : ""}`}
      aria-pressed={value === id}
      onClick={() => onPick(id)}
    >
      <div className="theme-shot">{shot}</div>
      <span className="theme-name">
        {name}
        {note && <span className="theme-note">{note}</span>}
      </span>
    </button>
  );
  return (
    <div className="theme-picker">
      {THEMES.map((t) => tile(t.id, t.name, <Preview theme={t.id} />))}
      {tile(
        "system",
        "Match the Mac",
        <>
          <Preview theme="night" />
          <Preview theme="day" half />
        </>,
        "Nightshade, night or day",
      )}
    </div>
  );
}

/** The app in miniature: top bar, task list, a page with text, statuses and a button. */
function Preview({ theme, half }: { theme: Theme; half?: boolean }) {
  return (
    <div className={`theme-preview${half ? " left-half" : ""}`} data-theme={theme} aria-hidden>
      <div className="tp-bar">
        <i className="on" />
        <i />
        <i />
      </div>
      <div className="tp-body">
        <div className="tp-side">
          <i className="on" />
          <i />
          <i style={{ width: "70%" }} />
        </div>
        <div className="tp-main">
          <i className="text" style={{ width: "80%" }} />
          <i style={{ width: "55%" }} />
          <div className="tp-dots">
            {["ok", "warn", "danger", "info"].map((c) => (
              <b key={c} style={{ background: `var(--status-${c}-ink)` }} />
            ))}
          </div>
          <span className="tp-button" />
        </div>
      </div>
    </div>
  );
}

/** A text size in px, 9–24, saved as the slider moves. */
function TextSize({
  label,
  hint,
  value,
  onChange,
  error,
}: {
  label: string;
  hint: string;
  value: number;
  onChange: (value: number) => void;
  /** A failed save of this size. */
  error?: ReactNode;
}) {
  return (
    <Field label={`${label} — ${value}px`} hint={hint}>
      <input
        type="range"
        min={9}
        max={24}
        step={1}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      {error}
    </Field>
  );
}
