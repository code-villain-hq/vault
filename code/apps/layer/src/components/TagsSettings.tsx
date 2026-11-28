import { useId, useRef, useState } from "react";
import { rejectInvalidTextTransfer, tagError } from "../lib/item-details-editor";
import { useStore } from "../store";
import { BusyLabel, Confirm, ErrorMessage, Field, useFailure } from "./ui";

/**
 * Runs one save at a time, checking the text first. The page's add, rename
 * and delete share it, so nothing starts while another is saving.
 */
function useSave() {
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const failure = useFailure();
  async function run(work: () => Promise<unknown>): Promise<boolean> {
    if (running.current) return false;
    failure.clear();
    running.current = true;
    setBusy(true);
    try {
      await work();
      return true;
    } catch (error) {
      failure.fail(error);
      return false;
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return { busy, run, failure };
}

/** A tag field: the same checks on typing, pasting and dropping as the editor's. */
function TagInput({
  label,
  value,
  invalid,
  describedBy,
  disabled,
  autoFocus,
  onChange,
  onInvalid,
  onEnter,
  onEscape,
}: {
  label: string;
  value: string;
  invalid: string | null;
  describedBy: string;
  disabled: boolean;
  autoFocus?: boolean;
  onChange: (value: string) => void;
  onInvalid: (error: string | null) => void;
  onEnter: () => void;
  onEscape?: () => void;
}) {
  return (
    <input
      className="ui-field"
      aria-label={label}
      value={value}
      disabled={disabled}
      autoFocus={autoFocus}
      aria-invalid={!!invalid}
      aria-describedby={invalid ? describedBy : undefined}
      onPaste={(e) =>
        rejectInvalidTextTransfer(e.clipboardData.getData("text/plain"), e, onInvalid)
      }
      onDrop={(e) => rejectInvalidTextTransfer(e.dataTransfer.getData("text/plain"), e, onInvalid)}
      // Enter is handled here, not by submitting a form: in the installed
      // app (WebKit) the form's submit never reached its handler, so Add tag
      // did nothing at all, while it worked in the mock harness (Chromium).
      // The editor's tag field, which works, does the same.
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.nativeEvent.isComposing) {
          e.preventDefault();
          onEnter();
        } else if (e.key === "Escape" && onEscape) {
          e.preventDefault();
          e.stopPropagation();
          onEscape();
        }
      }}
      onChange={(e) => {
        onChange(e.target.value);
        onInvalid(null);
      }}
    />
  );
}

export function TagsSettings() {
  const tags = useStore((s) => s.tags);
  const addTag = useStore((s) => s.addTag);
  const renameTag = useStore((s) => s.renameTag);
  const deleteTag = useStore((s) => s.deleteTag);
  const save = useSave();
  const [text, setText] = useState("");
  const [invalid, setInvalid] = useState<string | null>(null);
  // The tag being renamed or deleted, picked by clicking its chip.
  const [editing, setEditing] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [nameInvalid, setNameInvalid] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const id = useId();
  const nameId = useId();

  function add() {
    const error = tagError(text);
    setInvalid(error);
    if (error) return;
    void save.run(() => addTag(text)).then((ok) => ok && setText(""));
  }
  function pick(tag: string) {
    save.failure.clear();
    setNameInvalid(null);
    if (editing === tag) return setEditing(null);
    setEditing(tag);
    setName(tag);
  }
  function rename() {
    if (!editing) return;
    const error = tagError(name);
    setNameInvalid(error);
    if (error) return;
    if (name.trim() === editing) return setEditing(null);
    void save.run(() => renameTag(editing, name)).then((ok) => ok && setEditing(null));
  }

  return (
    // The hint goes above the field, not under the list: below a long list
    // it was out of sight, and it says what the page is for.
    <Field label="Tags">
      <div className="hint tags-hint">
        Reusable text tags for chats and work in this profile. Click a tag to rename or delete it.
      </div>
      <div className="browser-add">
        <TagInput
          label="New tag"
          value={text}
          invalid={invalid}
          describedBy={id}
          disabled={save.busy}
          onChange={setText}
          onInvalid={setInvalid}
          onEnter={add}
        />
        <button
          type="button"
          className="ui-button"
          disabled={save.busy || !text.trim()}
          onClick={add}
        >
          <BusyLabel busy={save.busy && !editing} idle="Add tag" working="Adding…" />
        </button>
      </div>
      <div id={id}>
        <ErrorMessage>{invalid}</ErrorMessage>
      </div>
      {tags.length ? (
        // The tag chip the rows use. The site chip this had before keeps
        // room on its right for a remove button a tag does not have, so a
        // short tag ("Fe") came out as a lopsided circle.
        <ul className="site-chips tags-list" aria-label="Tag catalog">
          {tags.map((tag) => (
            <li key={tag}>
              <button
                type="button"
                className="ui-chip item-details-tag"
                aria-pressed={editing === tag}
                aria-label={`Rename or delete tag ${tag}`}
                disabled={save.busy}
                onClick={() => pick(tag)}
              >
                {tag}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="hint">No tags yet. Add a tag to reuse it in chats and work.</p>
      )}
      {editing && (
        <div className="browser-add tags-edit">
          <TagInput
            key={editing}
            label={`New name for tag ${editing}`}
            value={name}
            invalid={nameInvalid}
            describedBy={nameId}
            disabled={save.busy}
            autoFocus
            onChange={setName}
            onInvalid={setNameInvalid}
            onEnter={rename}
            onEscape={() => setEditing(null)}
          />
          <button
            type="button"
            className="ui-button ui-button-primary"
            disabled={save.busy || !name.trim() || name.trim() === editing}
            onClick={rename}
          >
            <BusyLabel busy={save.busy} idle="Rename" working="Renaming…" />
          </button>
          <button
            type="button"
            className="ui-button ui-button-danger"
            disabled={save.busy}
            onClick={() => setDeleting(editing)}
          >
            Delete tag
          </button>
          <button
            type="button"
            className="ui-button"
            disabled={save.busy}
            onClick={() => setEditing(null)}
          >
            Cancel
          </button>
        </div>
      )}
      <div id={nameId}>
        <ErrorMessage>{nameInvalid}</ErrorMessage>
      </div>
      <ErrorMessage>{save.failure.error}</ErrorMessage>
      {deleting && (
        <Confirm
          title={`Delete tag "${deleting}"?`}
          body="It comes off every chat and work task that has it. This cannot be undone."
          confirmLabel="Delete tag"
          busyLabel="Deleting…"
          onConfirm={async () => {
            await deleteTag(deleting);
            setDeleting(null);
            setEditing(null);
          }}
          onCancel={() => setDeleting(null)}
        />
      )}
    </Field>
  );
}
