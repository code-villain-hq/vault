import { Dialog } from "@base-ui/react/dialog";
import { createContext, type ReactNode, useContext, useEffect, useRef, useState } from "react";
import { PRIORITY_LABELS, useLabels, useTeamStates, useTeams, useUsers } from "./data";
import { Avatar, LabelDot, PriorityIcon, ProjectIcon, StateIcon } from "./icons";
import { LabelChip } from "./issue-fields";
import { Picker } from "./picker";
import { Button } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/** What a new issue starts with, e.g. the team or project of the current page. */
export interface NewIssueDefaults {
  teamId?: string;
  stateId?: string;
  projectId?: string;
  parentId?: string;
  assigneeId?: string;
}

const Context = createContext<(defaults?: NewIssueDefaults) => void>(() => {});

/** Opens the new issue dialog. */
export function useNewIssue() {
  return useContext(Context);
}

/** Provides the dialog, and opens it on "c" anywhere in the workspace. */
export function NewIssueProvider({
  children,
  pageDefaults,
}: {
  children: ReactNode;
  /** Defaults from the page that's open, used by the "c" shortcut. */
  pageDefaults: NewIssueDefaults;
}) {
  const [open, setOpen] = useState<NewIssueDefaults | null>(null);
  const defaultsRef = useRef(pageDefaults);
  defaultsRef.current = pageDefaults;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "c" || event.metaKey || event.ctrlKey || event.altKey) return;
      if (
        isTyping(event.target) ||
        document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")
      )
        return;
      event.preventDefault();
      setOpen(defaultsRef.current);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <Context.Provider value={(defaults) => setOpen(defaults ?? defaultsRef.current)}>
      {children}
      <Dialog.Root open={open !== null} onOpenChange={(isOpen) => !isOpen && setOpen(null)}>
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 z-40 bg-black/40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
          <Dialog.Popup className="fixed top-[12vh] left-1/2 z-40 w-[min(40rem,calc(100vw-2rem))] -translate-x-1/2 rounded-xl border border-line bg-surface shadow-2xl shadow-black/30 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
            {open && <NewIssueForm defaults={open} onDone={() => setOpen(null)} />}
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </Context.Provider>
  );
}

function NewIssueForm({ defaults, onDone }: { defaults: NewIssueDefaults; onDone: () => void }) {
  const { sync } = useWorkspace();
  const teams = useTeams();
  const users = useUsers();
  const labels = useLabels();
  const projects = useRecords("Project");
  const issues = useRecords("Issue");

  const [teamId, setTeamId] = useState(defaults.teamId ?? teams[0]?.id ?? "");
  const states = useTeamStates(teamId);
  const firstState =
    states.find((s) => s.type === "backlog") ?? states.find((s) => s.type !== "triage");
  const [stateId, setStateId] = useState(defaults.stateId ?? "");
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [priority, setPriority] = useState(0);
  const [assigneeId, setAssigneeId] = useState(defaults.assigneeId ?? "");
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [projectId, setProjectId] = useState(defaults.projectId ?? "");

  const team = teams.find((t) => t.id === teamId);
  const state = states.find((s) => s.id === stateId) ?? firstState;
  const parent = issues.find((i) => i.id === defaults.parentId);

  const create = () => {
    if (!title.trim() || !team || !state) return;
    sync.create("Issue", {
      team_id: team.id,
      title: title.trim(),
      description,
      state_id: state.id,
      priority,
      assignee_id: assigneeId || undefined,
      label_ids: labelIds,
      project_id: projectId || undefined,
      parent_id: parent?.id,
    });
    onDone();
  };

  const chip = "h-7 px-2 border border-line rounded-md text-xs";

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        create();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          create();
        }
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <Picker
          label="Team"
          options={teams.map((t) => ({ value: t.id, label: `${t.name} (${t.key})` }))}
          selected={[teamId]}
          onSelect={(id) => {
            setTeamId(id);
            setStateId("");
            setLabelIds([]);
          }}
          triggerClassName="h-6 px-2 border border-line rounded-md"
        >
          {team?.key ?? "Team"}
        </Picker>
        <span>›</span>
        <Dialog.Title className="font-normal">
          {parent ? `New sub-issue of ${parent.title}` : "New issue"}
        </Dialog.Title>
      </div>

      <div className="flex flex-col gap-2 px-4 py-3">
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a title
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Issue title"
          aria-label="Issue title"
          className="bg-transparent text-lg font-medium outline-none placeholder:text-faint"
        />
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Add description… (Markdown)"
          aria-label="Description"
          rows={4}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
      </div>

      <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
        <Picker
          label="Status"
          options={states.map((s) => ({
            value: s.id,
            label: s.name,
            icon: <StateIcon state={s} />,
          }))}
          selected={[state?.id ?? ""]}
          onSelect={setStateId}
          triggerClassName={chip}
        >
          {state && <StateIcon state={state} />}
          {state?.name ?? "Status"}
        </Picker>
        <Picker
          label="Priority"
          options={[0, 1, 2, 3, 4].map((p) => ({
            value: String(p),
            label: PRIORITY_LABELS[p] ?? "",
            icon: <PriorityIcon priority={p} />,
          }))}
          selected={[String(priority)]}
          onSelect={(value) => setPriority(Number(value))}
          triggerClassName={chip}
        >
          <PriorityIcon priority={priority} />
          {PRIORITY_LABELS[priority]}
        </Picker>
        <Picker
          label="Assignee"
          options={[
            { value: "", label: "No assignee", icon: <Avatar user={undefined} size="size-4" /> },
            ...users.map((u) => ({
              value: u.id,
              label: u.name,
              icon: <Avatar user={u} size="size-4" />,
            })),
          ]}
          selected={[assigneeId]}
          onSelect={setAssigneeId}
          triggerClassName={chip}
        >
          <Avatar user={users.find((u) => u.id === assigneeId)} size="size-4" />
          {users.find((u) => u.id === assigneeId)?.name ?? "Assignee"}
        </Picker>
        <Picker
          label="Labels"
          multiple
          options={labels
            .filter((l) => !l.team_id || l.team_id === teamId)
            .map((l) => ({ value: l.id, label: l.name, icon: <LabelDot color={l.color} /> }))}
          selected={labelIds}
          onSelect={(id) =>
            setLabelIds((ids) => (ids.includes(id) ? ids.filter((l) => l !== id) : [...ids, id]))
          }
          triggerClassName={chip}
        >
          {labelIds.length === 0
            ? "Labels"
            : labels
                .filter((l) => labelIds.includes(l.id))
                .map((l) => <LabelChip key={l.id} name={l.name} color={l.color} />)}
        </Picker>
        <Picker
          label="Project"
          options={[
            { value: "", label: "No project" },
            ...projects.map((p) => ({ value: p.id, label: p.name, icon: <ProjectIcon /> })),
          ]}
          selected={[projectId]}
          onSelect={setProjectId}
          triggerClassName={chip}
        >
          <ProjectIcon />
          {projects.find((p) => p.id === projectId)?.name ?? "Project"}
        </Picker>
      </div>

      <div className="flex items-center justify-end gap-3 border-t border-line px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!title.trim() || !team}>
          Create issue
        </Button>
      </div>
    </form>
  );
}

export function isTyping(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}
