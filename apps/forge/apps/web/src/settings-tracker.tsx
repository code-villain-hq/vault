// Settings for teams, their workflows, and labels.
import { Dialog } from "@base-ui/react/dialog";
import { type FormEvent, useState } from "react";
import { api, messageOf } from "./api";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { useLabels, useTeamStates, useTeams } from "./data";
import { StateIcon } from "./icons";
import { PageHeader } from "./layout";
import { CreateDialog, MoreMenu, NewButton } from "./page-parts";
import { COLORS, ColorButton, ColorChoice } from "./palette";
import { Button, ErrorMessage } from "./ui";
import { useIsAdmin, useWorkspace } from "./workspace";

export function TeamsSettingsPage() {
  const isAdmin = useIsAdmin();
  const teams = useTeams();
  const [creating, setCreating] = useState(false);

  return (
    <>
      <PageHeader title="Teams">
        {isAdmin && (
          <>
            <NewButton label="New team" onClick={() => setCreating(true)} />
            <CreateDialog open={creating} onClose={() => setCreating(false)}>
              <NewTeamForm onDone={() => setCreating(false)} />
            </CreateDialog>
          </>
        )}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-10 px-6 py-8">
          {!isAdmin && (
            <p className="text-muted">Teams and their workflows are set up by workspace admins.</p>
          )}
          {teams.map((team) => (
            <TeamSettings key={team.id} teamId={team.id} editable={isAdmin} />
          ))}
        </div>
      </div>
    </>
  );
}

/** Making a team: its name and the key its issue identifiers start with. */
function NewTeamForm({ onDone }: { onDone: () => void }) {
  const { workspace } = useWorkspace();
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);

  const create = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    try {
      await api("POST", `/api/w/${workspace.id}/teams`, { name: name.trim(), key });
      onDone();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <form onSubmit={create}>
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Teams</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New team</Dialog.Title>
      </div>
      <div className="flex flex-col gap-2 px-4 py-3">
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
          autoFocus
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setKey(suggestKey(e.target.value));
          }}
          placeholder="Team name, such as Design"
          aria-label="Team name"
          className="bg-transparent text-title font-bold outline-none placeholder:text-faint"
        />
        <p className="text-muted">Each team has its own issues and workflow.</p>
        <label className="flex items-center gap-2 text-xs text-muted">
          Issues are numbered
          <input
            aria-label="Team key"
            maxLength={5}
            value={key}
            onChange={(e) => setKey(e.target.value.toUpperCase())}
            placeholder="KEY"
            className="w-16 rounded-md ui-field px-2 py-1 font-mono text-fg uppercase outline-none placeholder:text-faint"
          />
          <span className="font-mono">-1, -2, …</span>
        </label>
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-3 px-4 py-3">
        <Button type="submit" disabled={!name.trim() || !key}>
          Create team
        </Button>
      </div>
    </form>
  );
}

function suggestKey(name: string) {
  const key = name
    .replace(/[^A-Za-z0-9]/g, "")
    .slice(0, 3)
    .toUpperCase();
  return /^[A-Z]/.test(key) ? key : "";
}

function TeamSettings({ teamId, editable }: { teamId: string; editable: boolean }) {
  const { sync } = useWorkspace();
  const team = useTeams().find((t) => t.id === teamId);
  const states = useTeamStates(teamId);
  if (!team) return null;

  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center gap-3">
        <input
          aria-label="Team name"
          readOnly={!editable}
          defaultValue={team.name}
          key={team.name}
          onBlur={(e) => {
            const name = e.target.value.trim();
            if (name && name !== team.name) sync.update("Team", team.id, { name });
          }}
          className="flex-1 bg-transparent text-title font-bold outline-none"
        />
        <label className="flex items-center gap-2 text-xs text-muted">
          Key
          <input
            aria-label="Team key"
            readOnly={!editable}
            defaultValue={team.key}
            key={team.key}
            maxLength={5}
            onBlur={(e) => {
              const key = e.target.value.toUpperCase();
              if (key && key !== team.key) sync.update("Team", team.id, { key });
            }}
            className="w-14 rounded-md bg-transparent px-2 py-1 font-mono text-fg uppercase outline-none hover:bg-hover focus:bg-hover"
          />
        </label>
      </div>
      <ul className="-mx-3 flex flex-col gap-0.5">
        {states.map((state) => (
          <li
            key={state.id}
            className="flex items-center gap-3 rounded-lg px-3 py-1.5 hover:bg-hover"
          >
            <StateIcon state={state} />
            <input
              aria-label={`Name of ${state.name}`}
              readOnly={!editable}
              defaultValue={state.name}
              key={state.name}
              onBlur={(e) => {
                const name = e.target.value.trim();
                if (name && name !== state.name) sync.update("WorkflowState", state.id, { name });
              }}
              className="flex-1 bg-transparent outline-none"
            />
            <span className="text-xs text-faint">{state.type}</span>
            <ColorButton
              label={`Colour of ${state.name}`}
              value={state.color}
              disabled={!editable}
              onChange={(color) => sync.update("WorkflowState", state.id, { color })}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

export function LabelsSettingsPage() {
  const { sync } = useWorkspace();
  const isAdmin = useIsAdmin();
  const labels = useLabels();
  const [creating, setCreating] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  return (
    <>
      <PageHeader title="Labels">
        <NewButton label="New label" onClick={() => setCreating(true)} />
        <CreateDialog open={creating} onClose={() => setCreating(false)}>
          <NewLabelForm onDone={() => setCreating(false)} />
        </CreateDialog>
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-6 px-6 py-8">
          {labels.length === 0 && <p className="text-muted">No labels yet.</p>}
          <ul className="-mx-3 flex flex-col gap-0.5">
            {labels.map((label) => (
              <li
                key={label.id}
                className="flex items-center gap-3 rounded-lg px-3 py-1.5 hover:bg-hover"
              >
                <ColorButton
                  label={`Colour of ${label.name}`}
                  value={label.color}
                  onChange={(c) => sync.update("Label", label.id, { color: c })}
                />
                <input
                  aria-label={`Name of label ${label.name}`}
                  defaultValue={label.name}
                  key={label.name}
                  onBlur={(e) => {
                    const next = e.target.value.trim();
                    if (next && next !== label.name) sync.update("Label", label.id, { name: next });
                  }}
                  className="flex-1 bg-transparent outline-none"
                />
                {isAdmin && (
                  <MoreMenu
                    label={`Actions for ${label.name}`}
                    items={[
                      {
                        label: "Delete label",
                        danger: true,
                        onSelect: () =>
                          setConfirmation({
                            title: `Delete the label “${label.name}”?`,
                            body: "It's removed from every issue. This can't be undone.",
                            action: "Delete label",
                            onConfirm: () => sync.delete("Label", label.id),
                          }),
                      },
                    ]}
                  />
                )}
              </li>
            ))}
          </ul>
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** Making a label: its name and colour. */
function NewLabelForm({ onDone }: { onDone: () => void }) {
  const { sync } = useWorkspace();
  const labels = useLabels();
  const [name, setName] = useState("");
  const [color, setColor] = useState(
    () => COLORS.find((c) => !labels.some((l) => l.color === c)) ?? COLORS[0] ?? "#9fa9ff",
  );

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!name.trim()) return;
        sync.create("Label", { name: name.trim(), color });
        onDone();
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Labels</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New label</Dialog.Title>
      </div>
      <div className="flex flex-col gap-4 px-4 py-3">
        <div className="flex items-center gap-3">
          <span className="size-3.5 shrink-0 rounded-full" style={{ background: color }} />
          <input
            // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Label name, such as Bug"
            aria-label="Label name"
            className="min-w-0 flex-1 bg-transparent text-title font-bold outline-none placeholder:text-faint"
          />
        </div>
        <ColorChoice large value={color} onChange={setColor} />
      </div>
      <div className="flex items-center justify-end gap-3 px-4 py-3">
        <Button type="submit" disabled={!name.trim()}>
          Create label
        </Button>
      </div>
    </form>
  );
}
