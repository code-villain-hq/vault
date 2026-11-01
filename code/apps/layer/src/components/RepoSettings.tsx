import { useEffect, useState } from "react";
import { updateByFor } from "../lib/derive";
import type { Project, RepoHealth, UpdateBy } from "../lib/types";

// A repository's own settings, as fields on its row in the Repos view.

/**
 * How Update from base updates branches here (UPD-7). Unset, it follows
 * the guess from history, and the first option says what that is.
 */
export function UpdateByPicker({
  p,
  h,
  onChange,
}: {
  p: Project;
  h: RepoHealth | undefined;
  onChange: (by: UpdateBy | null) => void;
}) {
  const guess = updateByFor(undefined, h);
  return (
    <select
      className="ui-field update-by"
      value={p.update_by ?? ""}
      title={`How Update from base brings the base in. ${updateByFor(p, h).why}`}
      onChange={(e) => onChange((e.target.value || null) as UpdateBy | null)}
    >
      <option value="">{h?.update_guess ? `${guess.by}s (guessed)` : "merges (default)"}</option>
      <option value="merge">merges</option>
      <option value="rebase">rebases</option>
    </select>
  );
}

/** What a loop runs here (LOOP-1), saved on Enter or leaving the field. */
export function CheckCommand({ p, onSave }: { p: Project; onSave: (command: string) => void }) {
  const [value, setValue] = useState(p.check ?? "");
  useEffect(() => setValue(p.check ?? ""), [p.check]);
  return (
    <input
      className="ui-field repo-check mono"
      value={value}
      placeholder="check command, for loops"
      title="What says the work here is done (bun run check, cargo test). A loop runs it after each of an agent's turns."
      spellCheck={false}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => {
        if (value.trim() !== (p.check ?? "")) onSave(value.trim());
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") setValue(p.check ?? "");
      }}
    />
  );
}

/**
 * Where specs are kept for this repository (SPEC-1, SPEC-3): committed in it,
 * under a folder, or by the app for a repository whose team keeps none.
 */
export function SpecsPicker({
  p,
  onChange,
}: {
  p: Project;
  onChange: (inApp: boolean, folder: string) => void;
}) {
  const folder = p.spec_folder ?? "specs";
  const folders = [...new Set(["specs", "docs/specs", folder])];
  return (
    <select
      className="ui-field update-by"
      value={p.specs_in_app ? "" : folder}
      title="Where a task's spec is kept: committed on its branch in this folder, or by the app, outside git"
      onChange={(e) => onChange(!e.target.value, e.target.value || folder)}
    >
      {folders.map((f) => (
        <option key={f} value={f}>
          specs in {f}/
        </option>
      ))}
      <option value="">specs kept by the app</option>
    </select>
  );
}
