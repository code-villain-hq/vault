import { useRef, useState } from "react";
import type { ReviewNote } from "../store";
import { Markdown } from "./Markdown";

/**
 * `text` with a suggested change for `code` added at the end, as GitHub
 * writes one: on GitHub the author applies it in one click.
 */
function withSuggestion(text: string, code: string): string {
  const lead = text.trim() ? `${text.trimEnd()}\n\n` : "";
  return `${lead}\`\`\`suggestion\n${code}\n\`\`\``;
}

/**
 * A note's text being written: a new one, or one edited. "Suggest a change"
 * starts a suggestion from the line's code, to be edited into what it
 * should read.
 */
function NoteText({
  initial, code, placeholder, saveLabel, onSave, onCancel,
}: {
  initial: string;
  code: string;
  placeholder: string;
  saveLabel: string;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const box = useRef<HTMLTextAreaElement>(null);
  return (
    <>
      <textarea className="ui-field"
        ref={box}
        rows={Math.min(12, Math.max(3, text.split("\n").length + 1))}
        autoFocus
        value={text}
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onSave(text);
          if (e.key === "Escape") onCancel();
        }}
      />
      <div className="actions">
        <button className="ui-button ui-button-primary" onClick={() => onSave(text)}>{saveLabel}</button>
        <button className="ui-button" onClick={onCancel}>Cancel</button>
        {code !== "" && !text.includes("```suggestion") && (
          <button
            className="ui-button"
            title="Write what the line should read; the author can apply it from GitHub"
            onClick={() => {
              const next = withSuggestion(text, code);
              setText(next);
              // The line's code selected inside the block: typing replaces
              // it with what it should read.
              const end = next.length - "\n```".length;
              requestAnimationFrame(() => {
                box.current?.focus();
                box.current?.setSelectionRange(end - code.length, end);
              });
            }}
          >
            Suggest a change
          </button>
        )}
        <span style={{ color: "var(--faint)", fontSize: "var(--type-ui-small-size)" }}>⌘↵ to {saveLabel.toLowerCase()}</span>
      </div>
    </>
  );
}

/**
 * The box a note is typed into, holding its own text.
 *
 * In the diff's state, every keystroke re-rendered the whole diff — three
 * thousand rows — to change one textarea.
 */
export function NoteEditor({
  onAdd, onCancel, placeholder, code,
}: { onAdd: (text: string) => void; onCancel: () => void; placeholder: string; code: string }) {
  return (
    <div className="inline-comment">
      <NoteText initial="" code={code} placeholder={placeholder} saveLabel="Add note" onSave={onAdd} onCancel={onCancel} />
    </div>
  );
}

/** A note under a line of the Diff tab (DIFF-8), queued to send as written. */
export function NoteCard({
  note,
  onDrop,
  onEdit,
  where,
}: {
  note: ReviewNote;
  onDrop: () => void;
  onEdit: (body: string) => void;
  /** Said when the note is not under its line: `L120`. */
  where?: string;
}) {
  const [editing, setEditing] = useState(false);
  // Said when it is not plain: a removed line, a range, or away from its line.
  const span = note.side === "LEFT"
    ? `removed L${note.line}`
    : note.startLine ? `L${note.startLine}–${note.line}` : where ? `L${note.line}` : null;
  if (editing) {
    return (
      <div className="inline-comment">
        <NoteText
          initial={note.body}
          code={note.code}
          placeholder="What should the author know about this line?"
          saveLabel="Save"
          onSave={(body) => { if (body.trim()) onEdit(body.trim()); setEditing(false); }}
          onCancel={() => setEditing(false)}
        />
      </div>
    );
  }
  return (
    <div className="inline-comment">
      {span && (
        <div className="note-head">
          <span className="ui-chip">{span}</span>
        </div>
      )}
      <div className="body"><Markdown text={note.body} /></div>
      <div className="actions">
        <button className="ui-button" onClick={() => setEditing(true)} title="Reword it">
          Edit
        </button>
        <button className="ui-button ui-button-danger" onClick={onDrop}>
          Remove
        </button>
      </div>
    </div>
  );
}
