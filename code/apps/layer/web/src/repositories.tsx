import { Dialog } from "@base-ui/react/dialog";
import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import type { Note, Repository, UpdateBy } from "@layer/types";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { type FormEvent, useMemo, useState } from "react";
import { ago } from "./format";
import { PageBody, PageHeader } from "./layout";
import { CreateDialog, MoreMenu } from "./page-parts";
import { remoteKey } from "./remote";
import { Button, buttonClass, ErrorMessage, Section, TextField } from "./ui";
import { useRecords, useSyncStatus, useWorkspace } from "./workspace";

/** As many notes as a repository keeps, and as long as each may be (`notes.rs`). */
const NOTES_PER_REPO = 50;
const NOTE_MAX = 500;

/** The workspace's repositories, by name. */
function useRepositories(): Repository[] {
  const repositories = useRecords("Repository");
  return useMemo(
    () => [...repositories].sort((a, b) => a.name.localeCompare(b.name)),
    [repositories],
  );
}

/** Notes by the remote key of their repository, newest checked first. */
function useNotesByRepo(): Map<string, Note[]> {
  const notes = useRecords("Note");
  return useMemo(() => {
    const byRepo = new Map<string, Note[]>();
    for (const note of notes) byRepo.set(note.repo, [...(byRepo.get(note.repo) ?? []), note]);
    for (const list of byRepo.values())
      list.sort((a, b) => b.checked_at.localeCompare(a.checked_at));
    return byRepo;
  }, [notes]);
}

/** The name a repository likely goes by: the remote's last part. */
function nameOf(remote: string) {
  return remoteKey(remote).split("/").pop() ?? "";
}

/** Groups in name order, ungrouped repositories last. */
function byGroup(repositories: Repository[]) {
  const groups = new Map<string, Repository[]>();
  for (const repository of repositories) {
    const group = repository.group_name ?? "";
    groups.set(group, [...(groups.get(group) ?? []), repository]);
  }
  return [...groups.entries()].sort(([a], [b]) =>
    a === "" ? 1 : b === "" ? -1 : a.localeCompare(b),
  );
}

/**
 * `/$workspace/repositories`: what the workspace works on. Everyone adds
 * them. Each Mac keeps its own clone, and Layer knows it by its remote.
 */
export function RepositoriesPage() {
  const repositories = useRepositories();
  const notes = useNotesByRepo();
  const { ready } = useSyncStatus();
  const [adding, setAdding] = useState(false);
  const groups = byGroup(repositories);

  return (
    <>
      <PageHeader title="Repositories">
        <Button onClick={() => setAdding(true)}>Add repository</Button>
      </PageHeader>
      <PageBody>
        {ready && repositories.length === 0 ? (
          <div className="mx-auto flex max-w-sm flex-col items-center gap-3 py-16 text-center">
            <h2 className="font-medium">No repositories yet</h2>
            <p className="text-muted">
              Add one by its remote. Each Mac clones it in Layer, and shares its notes here.
            </p>
            <Button onClick={() => setAdding(true)}>Add repository</Button>
          </div>
        ) : (
          groups.map(([group, members]) =>
            // One list with no groups needs no heading.
            groups.length === 1 && !group ? (
              <RepositoryList key="" repositories={members} notes={notes} />
            ) : (
              <Section key={group} title={group || "Other"}>
                <RepositoryList repositories={members} notes={notes} />
              </Section>
            ),
          )
        )}
      </PageBody>
      <CreateDialog open={adding} onClose={() => setAdding(false)} width="28rem">
        <AddRepository repositories={repositories} onDone={() => setAdding(false)} />
      </CreateDialog>
    </>
  );
}

function RepositoryList({
  repositories,
  notes,
}: {
  repositories: Repository[];
  notes: Map<string, Note[]>;
}) {
  const { workspace } = useWorkspace();
  return (
    <ul className="-mx-3 flex flex-col gap-0.5">
      {repositories.map((repository) => {
        const count = notes.get(repository.remote_key)?.length ?? 0;
        return (
          <li key={repository.id}>
            <Link
              to="/$workspace/repositories/$repository"
              params={{ workspace: workspace.url_key, repository: repository.id }}
              className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-hover"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate">{repository.name}</div>
                <div className="truncate font-mono text-xs text-muted">{repository.remote_key}</div>
              </div>
              {count > 0 && (
                <span className="text-xs text-faint">
                  {count === 1 ? "1 note" : `${count} notes`}
                </span>
              )}
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

function AddRepository({
  repositories,
  onDone,
}: {
  repositories: Repository[];
  onDone: () => void;
}) {
  const { sync, workspace } = useWorkspace();
  const navigate = useNavigate();
  const [remote, setRemote] = useState("");
  const [name, setName] = useState("");
  const [nameTyped, setNameTyped] = useState(false);
  const [branch, setBranch] = useState("main");
  const [error, setError] = useState<string | null>(null);
  const shownName = nameTyped ? name : nameOf(remote);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const key = remoteKey(remote);
    if (!key.includes("/")) {
      setError("A remote is where git fetches from, like git@github.com:acme/api.git.");
      return;
    }
    const known = repositories.find((r) => r.remote_key === key);
    if (known) {
      setError(`The workspace already has ${key}, as ${known.name}.`);
      return;
    }
    const id = sync.create("Repository", {
      remote_key: key,
      name: shownName.trim(),
      default_branch: branch.trim(),
    });
    onDone();
    void navigate({
      to: "/$workspace/repositories/$repository",
      params: { workspace: workspace.url_key, repository: id },
    });
  };

  return (
    <form onSubmit={submit}>
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Repositories</span>
        <span>›</span>
        <Dialog.Title className="font-normal">Add repository</Dialog.Title>
      </div>
      <div className="flex flex-col gap-4 px-4 py-4">
        <TextField
          label="Remote"
          required
          autoFocus
          placeholder="git@github.com:acme/api.git"
          hint="Where git fetches it from. Every Mac's clone of it is the same repository."
          className="font-mono"
          value={remote}
          onChange={(e) => {
            setRemote(e.target.value);
            setError(null);
          }}
        />
        <TextField
          label="Name"
          required
          placeholder="api"
          value={shownName}
          onChange={(e) => {
            setNameTyped(true);
            setName(e.target.value);
          }}
        />
        <TextField
          label="Default branch"
          required
          className="font-mono"
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
        />
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Dialog.Close render={<Button variant="secondary" />}>Cancel</Dialog.Close>
        <Button type="submit" disabled={!remote.trim() || !shownName.trim() || !branch.trim()}>
          Add
        </Button>
      </div>
    </form>
  );
}

/** `/$workspace/repositories/$repository`: how it is worked on, and its notes. */
export function RepositoryPage() {
  const { workspace, sync } = useWorkspace();
  const { repository: id } = useParams({ strict: false }) as { repository: string };
  const navigate = useNavigate();
  const repository = useRecords("Repository").find((r) => r.id === id);
  const notes = useNotesByRepo().get(repository?.remote_key ?? "") ?? [];
  const { ready } = useSyncStatus();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const parent = (
    <Link
      to="/$workspace/repositories"
      params={{ workspace: workspace.url_key }}
      className="hover:text-fg"
    >
      Repositories
    </Link>
  );

  if (!repository) {
    if (!ready) return null;
    return (
      <>
        <PageHeader title="Not found" parent={parent} />
        <PageBody>
          <p className="text-muted">This workspace has no such repository, or it was removed.</p>
        </PageBody>
      </>
    );
  }

  return (
    <>
      <PageHeader title={repository.name} parent={parent}>
        <MoreMenu
          label="More"
          items={[
            {
              label: "Remove repository",
              danger: true,
              onSelect: () =>
                setConfirmation({
                  title: `Remove ${repository.name}?`,
                  body: "The workspace stops sharing it. Clones on each Mac stay, and so do its notes, for when it's added again.",
                  action: "Remove",
                  onConfirm: () => {
                    sync.delete("Repository", repository.id);
                    void navigate({
                      to: "/$workspace/repositories",
                      params: { workspace: workspace.url_key },
                    });
                  },
                }),
            },
          ]}
        />
      </PageHeader>
      <PageBody>
        <p className="font-mono text-xs text-muted">{repository.remote_key}</p>
        <Notes repository={repository} notes={notes} />
        <RepositorySettings key={repository.updated_at} repository={repository} />
      </PageBody>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** Empty is none: what the server keeps as null. */
const orNull = (value: string) => value.trim() || null;

function RepositorySettings({ repository }: { repository: Repository }) {
  const { sync } = useWorkspace();
  const [name, setName] = useState(repository.name);
  const [branch, setBranch] = useState(repository.default_branch);
  const [group, setGroup] = useState(repository.group_name ?? "");
  const [updateBy, setUpdateBy] = useState<UpdateBy | "">(repository.update_by ?? "");
  const [check, setCheck] = useState(repository.check_command ?? "");
  const [specFolder, setSpecFolder] = useState(repository.spec_folder ?? "");
  const [specsInApp, setSpecsInApp] = useState(repository.specs_in_app);
  const changed =
    name !== repository.name ||
    branch !== repository.default_branch ||
    group !== (repository.group_name ?? "") ||
    updateBy !== (repository.update_by ?? "") ||
    check !== (repository.check_command ?? "") ||
    specFolder !== (repository.spec_folder ?? "") ||
    specsInApp !== repository.specs_in_app;

  const save = (event: FormEvent) => {
    event.preventDefault();
    sync.update("Repository", repository.id, {
      name: name.trim(),
      default_branch: branch.trim(),
      group_name: orNull(group),
      update_by: updateBy || null,
      check_command: orNull(check),
      spec_folder: orNull(specFolder),
      specs_in_app: specsInApp,
    });
  };

  return (
    <Section title="How it's worked on">
      <form onSubmit={save} className="flex max-w-lg flex-col gap-4">
        <TextField label="Name" required value={name} onChange={(e) => setName(e.target.value)} />
        <TextField
          label="Default branch"
          required
          className="font-mono"
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
        />
        <TextField
          label="Group"
          placeholder="backend"
          hint="Repositories in a group are listed together."
          value={group}
          onChange={(e) => setGroup(e.target.value)}
        />
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-muted">Update from base by</span>
          <fieldset className="ui-segmented self-start" aria-label="Update from base by">
            {(
              [
                ["", "Its history"],
                ["merge", "Merge"],
                ["rebase", "Rebase"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                className="ui-segment"
                aria-pressed={updateBy === value}
                onClick={() => setUpdateBy(value)}
              >
                {label}
              </button>
            ))}
          </fieldset>
          <p className="text-xs text-faint">"Its history" picks what the branch has done before.</p>
        </div>
        <TextField
          label="Check"
          placeholder="bun run check"
          hint="What says the work is done. A loop runs it until it passes."
          className="font-mono"
          value={check}
          onChange={(e) => setCheck(e.target.value)}
        />
        <TextField
          label="Spec folder"
          placeholder="specs"
          className="font-mono"
          value={specFolder}
          onChange={(e) => setSpecFolder(e.target.value)}
        />
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={specsInApp}
            onChange={(e) => setSpecsInApp(e.target.checked)}
          />
          Keep specs in Layer instead of committing them
        </label>
        <div>
          <Button type="submit" disabled={!changed || !name.trim() || !branch.trim()}>
            Save
          </Button>
        </div>
      </form>
    </Section>
  );
}

/** What agents learned about the repository, for the next task on it. */
function Notes({ repository, notes }: { repository: Repository; notes: Note[] }) {
  const { sync } = useWorkspace();
  const [editing, setEditing] = useState<Note | "new" | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const full = notes.length >= NOTES_PER_REPO;

  return (
    <Section title="Notes" aside={`${notes.length} of ${NOTES_PER_REPO}`}>
      <p className="text-muted">
        What agents learned here that holds beyond one task. Every agent on this repository reads
        them, on anyone's Mac.
      </p>
      {notes.length > 0 && (
        <ul className="-mx-3 flex flex-col gap-0.5">
          {notes.map((note) => (
            <li
              key={note.id}
              className="flex items-start gap-3 rounded-md px-3 py-2 hover:bg-hover"
            >
              <div className="min-w-0 flex-1">
                <p className="whitespace-pre-wrap">{note.text}</p>
                <p className="mt-1 truncate text-xs text-faint">
                  {note.paths.length > 0 && (
                    <span className="font-mono">{note.paths.join(", ")} · </span>
                  )}
                  {note.source || "Someone"} · checked {ago(note.checked_at)}
                </p>
              </div>
              <MoreMenu
                label="Note actions"
                items={[
                  {
                    label: "Still holds",
                    onSelect: () =>
                      sync.update("Note", note.id, { checked_at: new Date().toISOString() }),
                  },
                  { label: "Edit", onSelect: () => setEditing(note) },
                  {
                    label: "Forget",
                    danger: true,
                    onSelect: () =>
                      setConfirmation({
                        title: "Forget this note?",
                        body: "No agent on this repository reads it again.",
                        action: "Forget",
                        onConfirm: () => sync.delete("Note", note.id),
                      }),
                  },
                ]}
              />
            </li>
          ))}
        </ul>
      )}
      <div>
        {full ? (
          <p className="text-muted">Forget one that no longer holds to remember another.</p>
        ) : (
          <button
            type="button"
            className={buttonClass("secondary")}
            onClick={() => setEditing("new")}
          >
            Add note
          </button>
        )}
      </div>
      <CreateDialog open={editing !== null} onClose={() => setEditing(null)} width="32rem">
        {editing && (
          <NoteForm
            repository={repository}
            note={editing === "new" ? undefined : editing}
            onDone={() => setEditing(null)}
          />
        )}
      </CreateDialog>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </Section>
  );
}

function NoteForm({
  repository,
  note,
  onDone,
}: {
  repository: Repository;
  note?: Note;
  onDone: () => void;
}) {
  const { sync, me } = useWorkspace();
  const [text, setText] = useState(note?.text ?? "");
  const [paths, setPaths] = useState(note?.paths.join(", ") ?? "");
  const length = [...text.trim()].length;

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const list = paths
      .split(",")
      .map((p) => p.trim())
      .filter(Boolean);
    const now = new Date().toISOString();
    if (note) {
      sync.update("Note", note.id, { text: text.trim(), paths: list, checked_at: now });
    } else {
      sync.create("Note", {
        repo: repository.remote_key,
        text: text.trim(),
        paths: list,
        source: me.user.name,
        written_at: now,
        checked_at: now,
      });
    }
    onDone();
  };

  return (
    <form
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) submit(event);
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>{repository.name}</span>
        <span>›</span>
        <Dialog.Title className="font-normal">{note ? "Edit note" : "Add note"}</Dialog.Title>
      </div>
      <div className="flex flex-col gap-4 px-4 py-4">
        <div className="flex flex-col gap-1.5">
          <label htmlFor="note-text" className="text-xs font-medium text-muted">
            Note
          </label>
          <textarea
            id="note-text"
            required
            // biome-ignore lint/a11y/noAutofocus: the dialog opens to write the note
            autoFocus
            rows={4}
            placeholder="Run the migrations before the tests: they expect the new columns."
            className="ui-field rounded-md px-3 py-2"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <p className={`text-xs ${length > NOTE_MAX ? "text-danger-ink" : "text-faint"}`}>
            {length} of {NOTE_MAX} characters
          </p>
        </div>
        <TextField
          label="About"
          placeholder="migrations/, src/db.rs"
          hint="Files or folders, from the repository's root. Leave empty for all of it."
          className="font-mono"
          value={paths}
          onChange={(e) => setPaths(e.target.value)}
        />
      </div>
      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Dialog.Close render={<Button variant="secondary" />}>Cancel</Dialog.Close>
        <Button type="submit" disabled={length === 0 || length > NOTE_MAX}>
          {note ? "Save" : "Add"}
        </Button>
      </div>
    </form>
  );
}
