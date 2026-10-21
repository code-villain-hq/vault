import { useState } from "react";
import { api } from "../lib/api";
import { ago } from "../lib/time";
import { useNow } from "../store";
import type { Project, RepoNote } from "../lib/types";
import { Confirm, ErrorMessage, useFailure } from "./ui";

/** The most a note may say, as `notes::NOTE_MAX` has it. */
const NOTE_MAX = 500;

/**
 * What agents learned about one repository, to look through and keep true
 * (MEM-6). A note whose files changed since it was last checked is marked,
 * since an out-of-date note is read as instructions all the same.
 */
export function RepoNotes({ project, notes, onChanged }: {
  project: Project;
  notes: RepoNote[];
  onChanged: () => void;
}) {
  // Fails in the editor while one is open, else under the notes (NOTE-6).
  const failure = useFailure();
  const now = useNow(60_000);
  // A note's id, "new" for one being added, or nothing.
  const [editing, setEditing] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [paths, setPaths] = useState("");
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState<RepoNote | null>(null);

  function start(n?: RepoNote) {
    failure.clear();
    setEditing(n?.id ?? "new");
    setText(n?.text ?? "");
    setPaths(n?.paths.join(", ") ?? "");
  }

  async function run(work: Promise<unknown>) {
    setBusy(true);
    failure.clear();
    const at = editing;
    try {
      await work;
      // Only the editor this was for: leaving one note's editor for another's
      // Edit saves the first, and its answer must not close the second.
      setEditing((now) => (now === at ? null : now));
      onChanged();
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  function save() {
    if (busy || !text.trim()) return;
    const list = paths.split(",").map((p) => p.trim()).filter(Boolean);
    const was = notes.find((n) => n.id === editing);
    // Left as it was: nothing to send, the editor just closes.
    if (was && was.text === text && was.paths.join(",") === list.join(",")) {
      setEditing(null);
      return;
    }
    void run(editing === "new"
      ? api.addRepoNote(project.id, text, list)
      : api.editRepoNote(editing ?? "", text, list));
  }

  function cancel() {
    failure.clear();
    setEditing(null);
  }

  // A note being edited saves when you leave it, or on ⌘Enter, with no Save
  // button (studio/design/behaviour.md); Esc puts it back. Leaving means
  // focus going outside the editor, not from the text to its files. A new
  // note is only added by Add note or ⌘Enter: clicking away from one half
  // written is not asking to keep it.
  const editor = (
    <div
      className="repo-note-edit"
      onBlur={(e) => {
        if (editing === "new" || e.currentTarget.dataset.cancelled) return;
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        save();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          // The editor goes; a blur on its way out must not save it.
          e.currentTarget.dataset.cancelled = "1";
          cancel();
        }
      }}
    >
      <textarea className="ui-field"
        autoFocus
        value={text}
        maxLength={NOTE_MAX}
        placeholder={`One fact about ${project.name} that holds for every task: how to run its tests, a trap in its setup, where something lives.`}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter" && e.metaKey) { e.preventDefault(); save(); } }}
      />
      <input className="ui-field"
        value={paths}
        placeholder="Files or folders it is about, comma separated (optional): marks it when they change"
        onChange={(e) => setPaths(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); save(); } }}
      />
      <div className="note-actions">
        {editing === "new" && (
          <>
            <button className="ui-button ui-button-primary" disabled={busy || !text.trim()} onClick={save}>
              Add note
            </button>
            <button className="ui-button" disabled={busy} onClick={cancel}>Cancel</button>
          </>
        )}
        <span className="muted">
          {text.length}/{NOTE_MAX}
          {editing !== "new" && " · saves when you leave it or press ⌘Enter · Esc puts it back"}
        </span>
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </div>
  );

  return (
    <div className="repo-notes">
      {notes.length === 0 && editing !== "new" && (
        <div className="muted">
          Nothing remembered about {project.name} yet. Agents ask you before they
          keep something here, and you can add a note yourself.
        </div>
      )}
      {notes.map((n) => editing === n.id ? <div key={n.id}>{editor}</div> : (
        <div key={n.id} className={`repo-note${n.changed?.length ? " stale" : ""}`}>
          <div className="note-text">{n.text}</div>
          <div className="note-meta">
            <span>from {n.source || "an agent"}</span>
            <span>written {ago(n.written_at, now)}</span>
            {n.checked_at !== n.written_at && <span>checked {ago(n.checked_at, now)}</span>}
            {n.paths.length > 0 && <span>about {n.paths.map((p) => <code key={p}>{p}</code>)}</span>}
            {n.changed && n.changed.length > 0 && (
              <span className="trouble-text" title={n.changed.join("\n")}>
                {n.changed.length} file{n.changed.length === 1 ? "" : "s"} changed since it was checked
              </span>
            )}
          </div>
          <div className="note-actions">
            <button
              className="ui-button"
              disabled={busy}
              title="You checked it against the code and it holds. Its age and changed files count from now."
              onClick={() => void run(api.checkRepoNote(n.id))}
            >
              Still true
            </button>
            <button className="ui-button" disabled={busy} onClick={() => start(n)}>Edit</button>
            <button className="ui-button ui-button-danger" disabled={busy} onClick={() => setDeleting(n)}>Delete</button>
          </div>
        </div>
      ))}
      {editing === "new" ? editor : (
        <button className="ui-button" style={{ marginTop: 6 }} onClick={() => start()}>Add a note</button>
      )}
      {!editing && <ErrorMessage>{failure.error}</ErrorMessage>}
      {deleting && (
        <Confirm
          title="Delete note"
          confirmLabel="Delete note"
          body={<>
            No later task on <b>{project.name}</b> will be told this:
            <div className="confirm-detail">{deleting.text}</div>
            This can't be undone.
          </>}
          busyLabel="Deleting…"
          onConfirm={() => run(api.deleteRepoNote(deleting.id))}
          onCancel={() => setDeleting(null)}
        />
      )}
    </div>
  );
}
