// Projects and initiatives: lists and detail pages.
import { Dialog } from "@base-ui/react/dialog";
import type {
  InitiativePatch,
  InitiativeStatus,
  Issue,
  ProjectPatch,
  ProjectStatus,
} from "@forge/types";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { Mountain } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { formatDate, useUsers } from "./data";
import { FilterBar, FilterButton } from "./filters";
import { Avatar, ProjectIcon } from "./icons";
import { applyFilters, type Filters } from "./issue-filter";
import { IssueList, SharedFields } from "./issue-list";
import { PageHeader } from "./layout";
import { useNewIssue } from "./new-issue";
import {
  CreateDialog,
  chip,
  DateChip,
  MoreMenu,
  NewButton,
  PersonChip,
  StatusChip,
} from "./page-parts";
import { Picker } from "./picker";
import { Button } from "./ui";
import { useIsAdmin, useRecords, useWorkspace } from "./workspace";

const PROJECT_STATUSES: { value: ProjectStatus; label: string; color: string }[] = [
  { value: "planned", label: "Planned", color: "var(--faint)" },
  { value: "started", label: "In progress", color: "var(--info)" },
  { value: "paused", label: "Paused", color: "var(--muted)" },
  { value: "completed", label: "Completed", color: "var(--ok)" },
  { value: "canceled", label: "Canceled", color: "var(--faint)" },
];

const INITIATIVE_STATUSES: { value: InitiativeStatus; label: string; color: string }[] = [
  { value: "planned", label: "Planned", color: "var(--faint)" },
  { value: "active", label: "Active", color: "var(--info)" },
  { value: "completed", label: "Completed", color: "var(--ok)" },
];

/** Share of a project's issues that are closed (completed or canceled). */
function progress(issues: Issue[]) {
  if (issues.length === 0) return 0;
  return issues.filter((i) => i.completed_at).length / issues.length;
}

function ProgressBar({ value }: { value: number }) {
  return (
    <span className="flex items-center gap-2 text-xs text-muted">
      <span className="h-1.5 w-16 overflow-hidden rounded-full bg-line">
        {/* Progress is detail, not a signal: grey until the work is all done. */}
        <span
          className={`block h-full rounded-full ${value === 1 ? "bg-ok" : "bg-muted"}`}
          style={{ width: `${value * 100}%` }}
        />
      </span>
      {Math.round(value * 100)}%
    </span>
  );
}

function StatusDot({ color }: { color: string }) {
  return (
    <span
      className="size-2 shrink-0 rounded-full"
      style={{ background: color }}
      aria-hidden="true"
    />
  );
}

export function ProjectsPage() {
  const { workspace } = useWorkspace();
  const projects = useRecords("Project");
  const issues = useRecords("Issue");
  const users = useUsers();
  const [creating, setCreating] = useState(false);

  const sorted = useMemo(
    () =>
      [...projects].sort(
        (a, b) =>
          PROJECT_STATUSES.findIndex((s) => s.value === a.status) -
            PROJECT_STATUSES.findIndex((s) => s.value === b.status) || a.name.localeCompare(b.name),
      ),
    [projects],
  );

  return (
    <>
      <PageHeader title="Projects">
        <NewButton label="New project" onClick={() => setCreating(true)} />
        <CreateDialog open={creating} onClose={() => setCreating(false)}>
          <NewPlanForm kind="Project" onDone={() => setCreating(false)} />
        </CreateDialog>
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        {sorted.length === 0 && (
          <p className="px-4 py-16 text-center text-muted">
            No projects yet. A project is a set of issues working toward one outcome.
          </p>
        )}
        <ul className="flex flex-col gap-0.5 p-2">
          {sorted.map((project) => {
            const status = PROJECT_STATUSES.find((s) => s.value === project.status);
            const lead = users.find((u) => u.id === project.lead_id);
            const about = firstLine(project.description);
            const own = issues.filter((i) => i.project_id === project.id);
            return (
              <li key={project.id}>
                <Link
                  to="/$workspace/project/$project"
                  params={{ workspace: workspace.url_key, project: project.id }}
                  className="flex items-center gap-3 rounded-md px-3 py-2.5 hover:bg-hover"
                >
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-hover">
                    <ProjectIcon />
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate font-medium">{project.name}</span>
                    {about && <span className="truncate text-xs text-muted">{about}</span>}
                  </span>
                  <span className="flex w-28 shrink-0 items-center gap-2 text-xs text-muted">
                    {status && <StatusDot color={status.color} />}
                    {status?.label}
                  </span>
                  {own.length > 0 && <ProgressBar value={progress(own)} />}
                  {project.target_date && (
                    <span className="w-16 shrink-0 text-right text-xs text-muted">
                      {formatDate(project.target_date)}
                    </span>
                  )}
                  {lead && <Avatar user={lead} />}
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </>
  );
}

/** The first line of a Markdown text, without its marks, to show in a list. */
function firstLine(markdown: string): string {
  const line = markdown.split("\n").find((l) => l.trim()) ?? "";
  return line
    .replace(/^[#>\-*\s]+/, "")
    .replace(/[*_`]/g, "")
    .trim();
}

/** Making a project or an initiative: its name and what it's about, then its page opens. */
function NewPlanForm({ kind, onDone }: { kind: "Project" | "Initiative"; onDone: () => void }) {
  const { sync, workspace } = useWorkspace();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const noun = kind === "Project" ? "project" : "initiative";

  const create = () => {
    if (!name.trim()) return;
    const id = sync.create(kind, { name: name.trim(), description });
    onDone();
    if (kind === "Project") {
      void navigate({
        to: "/$workspace/project/$project",
        params: { workspace: workspace.url_key, project: id },
      });
    } else {
      void navigate({
        to: "/$workspace/initiative/$initiative",
        params: { workspace: workspace.url_key, initiative: id },
      });
    }
  };

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
        <span>{kind === "Project" ? "Projects" : "Initiatives"}</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New {noun}</Dialog.Title>
      </div>
      <div className="flex flex-col gap-2 px-4 py-3">
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={kind === "Project" ? "Project name, such as Launch" : "Initiative name"}
          aria-label={`${kind} name`}
          className="bg-transparent text-title font-bold outline-none placeholder:text-faint"
        />
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="What it's about… (Markdown)"
          aria-label="What it's about"
          rows={3}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
      </div>
      <div className="flex items-center justify-end gap-3 px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!name.trim()}>
          Create {noun}
        </Button>
      </div>
    </form>
  );
}

export function ProjectPage() {
  const { project: id } = useParams({ strict: false }) as { project: string };
  const { sync, workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const users = useUsers();
  const project = useRecords("Project").find((p) => p.id === id);
  const initiatives = useRecords("Initiative");
  const issues = useRecords("Issue");
  const newIssue = useNewIssue();
  const navigate = useNavigate();
  const [filters, setFilters] = useState<Filters>({});
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const projectIssues = useMemo(() => issues.filter((i) => i.project_id === id), [issues, id]);
  const shown = useMemo(() => applyFilters(projectIssues, filters), [projectIssues, filters]);

  if (!project) return <PageHeader title="Project not found" />;
  const ws = workspace.url_key;
  const update = (patch: ProjectPatch) => sync.update("Project", project.id, patch);
  return (
    <>
      <PageHeader
        title={project.name}
        parent={
          <Link to="/$workspace/projects" params={{ workspace: ws }} className="hover:text-fg">
            Projects
          </Link>
        }
        meta={<SharedFields issues={shown} except={["project"]} />}
      >
        {projectIssues.length > 0 && <ProgressBar value={progress(projectIssues)} />}
        <FilterButton filters={filters} onChange={setFilters} />
        <NewButton label="New issue (C)" onClick={() => newIssue({ projectId: project.id })} />
        {isAdmin && (
          <MoreMenu
            label="Project actions"
            items={[
              {
                label: "Delete project",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the project “${project.name}”?`,
                    body: "It's deleted for everyone. Its issues stay, without a project.",
                    action: "Delete project",
                    onConfirm: () => {
                      sync.delete("Project", project.id);
                      void navigate({ to: "/$workspace/projects", params: { workspace: ws } });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <EditableHeader
          name={project.name}
          description={project.description}
          onName={(name) => update({ name })}
          onDescription={(description) => update({ description })}
          descriptionPlaceholder="What it's about…"
          properties={
            <div className="-mt-1 -ml-2 flex flex-wrap items-center gap-1">
              <StatusChip
                statuses={PROJECT_STATUSES}
                value={project.status}
                onChange={(status) => update({ status: status as ProjectStatus })}
              />
              <PersonChip
                label="Lead"
                users={users}
                value={project.lead_id}
                onChange={(lead_id) => update({ lead_id })}
              />
              <Picker
                label="Initiative"
                options={[
                  { value: "", label: "No initiative" },
                  ...initiatives.map((i) => ({ value: i.id, label: i.name })),
                ]}
                selected={[project.initiative_id ?? ""]}
                onSelect={(value) => update({ initiative_id: value || null })}
                triggerClassName={chip}
              >
                <Mountain size={15} strokeWidth={1.75} aria-hidden="true" />
                <span className="truncate">
                  {initiatives.find((i) => i.id === project.initiative_id)?.name ?? "Initiative"}
                </span>
              </Picker>
              <DateChip
                label="Target date"
                value={project.target_date}
                onChange={(target_date) => update({ target_date })}
              />
            </div>
          }
        />
        <FilterBar filters={filters} onChange={setFilters} />
        <IssueList
          issues={shown}
          empty={
            projectIssues.length > 0
              ? "No issues match these filters."
              : "No issues in this project yet."
          }
          showProject={false}
        />
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** Name and description, edited in place and saved on blur. */
export function EditableHeader({
  name,
  description,
  onName,
  onDescription,
  hideDescription = false,
  icon,
  descriptionPlaceholder = "What is this about? (Markdown)",
  properties,
  className = "",
}: {
  name: string;
  description: string;
  onName: (name: string) => void;
  onDescription: (description: string) => void;
  /** For pages that edit their text elsewhere. */
  hideDescription?: boolean;
  /** Shown beside the name, such as a service's own icon. */
  icon?: ReactNode;
  descriptionPlaceholder?: string;
  /** A row of chips under the name and description, such as a service's team. */
  properties?: ReactNode;
  className?: string;
}) {
  return (
    <div className={`flex flex-col gap-3 px-8 pt-6 pb-2 ${className}`}>
      <div className="flex items-center gap-3">
        {icon}
        <input
          aria-label="Name"
          defaultValue={name}
          key={name}
          onBlur={(e) =>
            e.target.value.trim() && e.target.value !== name && onName(e.target.value.trim())
          }
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
          className="min-w-0 flex-1 bg-transparent text-title font-bold outline-none"
        />
      </div>
      {!hideDescription && (
        <textarea
          aria-label="Description"
          placeholder={descriptionPlaceholder}
          defaultValue={description}
          key={description}
          onBlur={(e) => e.target.value !== description && onDescription(e.target.value)}
          className="field-sizing-content min-h-6 resize-none bg-transparent text-muted outline-none placeholder:text-faint"
        />
      )}
      {properties}
    </div>
  );
}

export function InitiativesPage() {
  const { workspace } = useWorkspace();
  const initiatives = useRecords("Initiative");
  const projects = useRecords("Project");
  const users = useUsers();
  const [creating, setCreating] = useState(false);

  return (
    <>
      <PageHeader title="Initiatives">
        <NewButton label="New initiative" onClick={() => setCreating(true)} />
        <CreateDialog open={creating} onClose={() => setCreating(false)}>
          <NewPlanForm kind="Initiative" onDone={() => setCreating(false)} />
        </CreateDialog>
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        {initiatives.length === 0 && (
          <p className="px-4 py-16 text-center text-muted">
            No initiatives yet. An initiative groups projects toward a larger goal.
          </p>
        )}
        <ul className="flex flex-col gap-0.5 p-2">
          {[...initiatives]
            .sort((a, b) => a.name.localeCompare(b.name))
            .map((initiative) => {
              const status = INITIATIVE_STATUSES.find((s) => s.value === initiative.status);
              const count = projects.filter((p) => p.initiative_id === initiative.id).length;
              const owner = users.find((u) => u.id === initiative.owner_id);
              const about = firstLine(initiative.description);
              return (
                <li key={initiative.id}>
                  <Link
                    to="/$workspace/initiative/$initiative"
                    params={{ workspace: workspace.url_key, initiative: initiative.id }}
                    className="flex items-center gap-3 rounded-md px-3 py-2.5 hover:bg-hover"
                  >
                    <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-hover text-muted">
                      <Mountain size={15} strokeWidth={1.75} aria-hidden="true" />
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-medium">{initiative.name}</span>
                      {about && <span className="truncate text-xs text-muted">{about}</span>}
                    </span>
                    {count > 0 && (
                      <span className="shrink-0 text-xs text-faint">
                        {count} {count === 1 ? "project" : "projects"}
                      </span>
                    )}
                    <span className="flex w-24 shrink-0 items-center gap-2 text-xs text-muted">
                      {status && <StatusDot color={status.color} />}
                      {status?.label}
                    </span>
                    {initiative.target_date && (
                      <span className="w-16 shrink-0 text-right text-xs text-muted">
                        {formatDate(initiative.target_date)}
                      </span>
                    )}
                    {owner && <Avatar user={owner} />}
                  </Link>
                </li>
              );
            })}
        </ul>
      </div>
    </>
  );
}

export function InitiativePage() {
  const { initiative: id } = useParams({ strict: false }) as { initiative: string };
  const { sync, workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const initiative = useRecords("Initiative").find((i) => i.id === id);
  const projects = useRecords("Project").filter((p) => p.initiative_id === id);
  const issues = useRecords("Issue");
  const users = useUsers();
  const navigate = useNavigate();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  if (!initiative) return <PageHeader title="Initiative not found" />;
  const update = (patch: InitiativePatch) => sync.update("Initiative", initiative.id, patch);
  const ws = workspace.url_key;

  return (
    <>
      <PageHeader
        title={initiative.name}
        parent={
          <Link to="/$workspace/initiatives" params={{ workspace: ws }} className="hover:text-fg">
            Initiatives
          </Link>
        }
      >
        {isAdmin && (
          <MoreMenu
            label="Initiative actions"
            items={[
              {
                label: "Delete initiative",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the initiative “${initiative.name}”?`,
                    body: "It's deleted for everyone. Its projects stay, without an initiative.",
                    action: "Delete initiative",
                    onConfirm: () => {
                      sync.delete("Initiative", initiative.id);
                      void navigate({ to: "/$workspace/initiatives", params: { workspace: ws } });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <EditableHeader
          name={initiative.name}
          description={initiative.description}
          onName={(name) => update({ name })}
          onDescription={(description) => update({ description })}
          descriptionPlaceholder="What it's about…"
          properties={
            <div className="-mt-1 -ml-2 flex flex-wrap items-center gap-1">
              <StatusChip
                statuses={INITIATIVE_STATUSES}
                value={initiative.status}
                onChange={(status) => update({ status: status as InitiativeStatus })}
              />
              <PersonChip
                label="Owner"
                users={users}
                value={initiative.owner_id}
                onChange={(owner_id) => update({ owner_id })}
              />
              <DateChip
                label="Target date"
                value={initiative.target_date}
                onChange={(target_date) => update({ target_date })}
              />
            </div>
          }
        />
        <h2 className="px-8 pt-6 pb-2 text-xs font-medium text-muted">Projects</h2>
        {projects.length === 0 && (
          <p className="px-8 text-muted">Add projects to this initiative from a project's page.</p>
        )}
        <ul className="flex flex-col gap-0.5 px-5">
          {projects.map((project) => {
            const own = issues.filter((i) => i.project_id === project.id);
            return (
              <li key={project.id}>
                <Link
                  to="/$workspace/project/$project"
                  params={{ workspace: ws, project: project.id }}
                  className="flex h-10 items-center gap-3 rounded-md px-3 hover:bg-hover"
                >
                  <ProjectIcon />
                  <span className="min-w-0 flex-1 truncate">{project.name}</span>
                  {own.length > 0 && <ProgressBar value={progress(own)} />}
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}
