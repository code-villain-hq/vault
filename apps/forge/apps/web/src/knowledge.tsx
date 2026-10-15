// Services, specs and decisions: the knowledge Forge keeps in step with the code.

import { Dialog } from "@base-ui/react/dialog";
import { Popover } from "@base-ui/react/popover";
import type {
  Decision,
  DecisionPatch,
  DecisionStatus,
  Domain,
  Issue,
  Service,
  Spec,
  SpecStatus,
  SpecVersion,
} from "@forge/types";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { FolderCode, Folders, GitBranch, Replace, UsersRound } from "lucide-react";
import { Children, cloneElement, isValidElement, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { conventionWarnings } from "./convention";
import { byId, formatDate, identifier, servicesOf, timeAgo, useStates, useTeams } from "./data";
import { ICON_NAMES } from "./icon-set";
import { Avatar, ProjectIcon, ServiceIcon, StateIcon } from "./icons";
import { IssueList, SharedFields } from "./issue-list";
import { useOpenSpecChanges } from "./issue-spec-change";
import { PageHeader } from "./layout";
import { Markdown, MarkdownField } from "./markdown";
import { useNewIssue } from "./new-issue";
import { CreateDialog, chip, DateChip, Dot, MoreMenu, NewButton, StatusChip } from "./page-parts";
import { COLORS, ColorChoice, CustomColor, RAINBOW } from "./palette";
import { Picker } from "./picker";
import { EditableHeader } from "./planning";
import { Redirect } from "./redirect";
import { useGitHubStatus } from "./settings-github";
import {
  type HistoryEntry,
  historyEntries,
  lastChanged,
  type PendingChange,
  pendingChanges,
  specConflicts,
} from "./spec-change";
import {
  DriftChecks,
  SuggestionCard,
  TextDiff,
  useAiConfigured,
  useOpenSuggestions,
} from "./suggestions";
import { Button } from "./ui";
import {
  aroundRequirements,
  requirementBody,
  requirementStatuses,
  requirements,
  VERDICTS,
} from "./verdicts";
import { useIsAdmin, useRecords, useSyncStatus, useWorkspace } from "./workspace";

const SPEC_STATUSES: { value: SpecStatus; label: string; color: string }[] = [
  { value: "draft", label: "Draft", color: "var(--faint)" },
  { value: "active", label: "Active", color: "var(--ok)" },
  { value: "deprecated", label: "Deprecated", color: "var(--muted)" },
];

const DECISION_STATUSES: { value: DecisionStatus; label: string; color: string }[] = [
  { value: "proposed", label: "Proposed", color: "var(--warn)" },
  { value: "accepted", label: "Accepted", color: "var(--ok)" },
  { value: "superseded", label: "Superseded", color: "var(--muted)" },
  { value: "overridden", label: "Overridden", color: "var(--danger)" },
];

/**
 * A page of records, made from the + in its header (`action`). Rows stand on
 * their own, with room around them instead of ruled lines.
 */
function ListPage({
  title,
  empty,
  action,
  parent,
  children,
}: {
  title: string;
  /** The page this one sits in, shown before the title. */
  parent?: React.ReactNode;
  empty: string | null;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  const listItems = (suffix: string) =>
    Children.map(children, (child, index) =>
      isValidElement(child)
        ? cloneElement(child, { key: `${suffix}-${child.key ?? index}` })
        : child,
    );
  return (
    <>
      <PageHeader title={title} parent={parent}>
        {action}
      </PageHeader>
      <main className="min-h-0 flex-1 overflow-y-auto">
        {empty && <p className="px-6 py-16 text-center text-muted">{empty}</p>}
        <ul className="flex flex-col gap-0.5 p-2">{listItems("o")}</ul>
      </main>
    </>
  );
}

/** The first line of a Markdown text, without its marks, to show in a list. */
function summary(markdown: string): string {
  const line = markdown.split("\n").find((l) => l.trim()) ?? "";
  return line
    .replace(/^[#>\-*\s]+/, "")
    .replace(/[*_`]/g, "")
    .trim();
}

/** "3 specs", "1 spec", or nothing for none. */
function count(n: number, one: string, many: string): string | null {
  if (n === 0) return null;
  return `${n} ${n === 1 ? one : many}`;
}

/** The one value every item has, if there are several items and they all agree. */
function sharedValue<T>(items: T[], value: (item: T) => string | null): string | null {
  const first = items[0] ? value(items[0]) : null;
  return items.length > 1 && first && items.every((item) => value(item) === first) ? first : null;
}

export function ServicesPage() {
  const { workspace } = useWorkspace();
  const services = useRecords("Service");
  const issues = useRecords("Issue");
  const specs = useRecords("Spec");
  const decisions = useRecords("Decision");
  const states = useStates();
  const teams = useTeams();
  // A repository or team every service shares tells them apart no better than
  // nothing, so it isn't shown; it is once they differ.
  const repository = sharedValue(services, (s) => s.repository);
  const teamId = sharedValue(services, (s) => s.team_id);
  const team = teams.find((t) => t.id === teamId);

  const [creating, setCreating] = useState(false);

  return (
    <ListPage
      title="Services"
      empty={
        services.length === 0
          ? "No services yet. A service is something that exists in your system, tied to where its code lives."
          : null
      }
      action={
        <>
          <NewButton label="New service" onClick={() => setCreating(true)} />
          <CreateDialog open={creating} onClose={() => setCreating(false)}>
            <NewServiceForm onDone={() => setCreating(false)} />
          </CreateDialog>
        </>
      }
    >
      {[...services]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((service) => {
          const open = issues.filter(
            (i) =>
              i.service_id === service.id &&
              !["completed", "canceled"].includes(states.get(i.state_id)?.type ?? ""),
          ).length;
          const counts = [
            count(open, "open issue", "open issues"),
            count(specs.filter((s) => servicesOf(s).includes(service.id)).length, "spec", "specs"),
            count(
              decisions.filter((d) => d.service_id === service.id).length,
              "decision",
              "decisions",
            ),
          ].filter(Boolean);
          return (
            <li key={service.id}>
              <Link
                to="/$workspace/service/$service"
                params={{ workspace: workspace.url_key, service: service.id }}
                className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-hover"
              >
                <IconTile
                  icon={service.icon}
                  color={service.color}
                  size="size-8"
                  iconSize="size-4"
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-medium">{service.name}</span>
                  {summary(service.description) && (
                    <span className="truncate text-xs text-muted">
                      {summary(service.description)}
                    </span>
                  )}
                </span>
                {!repository && (
                  <span className="w-40 truncate font-mono text-xs text-faint">
                    {service.repository ?? ""}
                  </span>
                )}
                {!team && (
                  <span className="w-24 text-xs text-muted">
                    {teams.find((t) => t.id === service.team_id)?.name}
                  </span>
                )}
                <span className="w-56 text-right text-xs text-muted">{counts.join(" · ")}</span>
              </Link>
            </li>
          );
        })}
    </ListPage>
  );
}

export function ServicePage() {
  const { service: id } = useParams({ strict: false }) as { service: string };
  const { sync, workspace } = useWorkspace();
  const service = useRecords("Service").find((s) => s.id === id);
  const specs = useRecords("Spec")
    .filter((s) => servicesOf(s).includes(id))
    .sort((a, b) => a.title.localeCompare(b.title));
  const decisions = useRecords("Decision").filter((d) => d.service_id === id);
  const issues = useRecords("Issue");
  const states = useStates();
  const newIssue = useNewIssue();
  const navigate = useNavigate();
  const suggestions = useOpenSuggestions(id);
  const ai = useAiConfigured();
  const isAdmin = useIsAdmin();
  const [creatingSpec, setCreatingSpec] = useState(false);
  const [recording, setRecording] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const openIssues = useMemo(
    () =>
      issues.filter(
        (i) =>
          i.service_id === id &&
          !["completed", "canceled"].includes(states.get(i.state_id)?.type ?? ""),
      ),
    [issues, states, id],
  );

  if (!service) return <PageHeader title="Service not found" />;
  const ws = workspace.url_key;
  // Forge AI's parts only show when there's something in them, or a model to fill them.
  const showCurrentState = ai === true || Boolean(service.current_state.trim());
  return (
    <>
      <PageHeader
        title={service.name}
        parent={
          <Link to="/$workspace/services" params={{ workspace: ws }} className="hover:text-fg">
            Services
          </Link>
        }
      >
        <NewButton
          label="New issue (C)"
          onClick={() => newIssue({ teamId: service.team_id ?? undefined })}
        />
        {isAdmin && (
          <MoreMenu
            label="Service actions"
            items={[
              {
                label: "Delete service",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the service “${service.name}”?`,
                    body: "It's deleted for everyone. Its issues, specs and decisions stay, without a service.",
                    action: "Delete service",
                    onConfirm: () => {
                      sync.delete("Service", service.id);
                      void navigate({ to: "/$workspace/services", params: { workspace: ws } });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-y-auto">
          <EditableHeader
            name={service.name}
            description={service.description}
            onName={(name) => sync.update("Service", service.id, { name })}
            onDescription={(description) => sync.update("Service", service.id, { description })}
            descriptionPlaceholder="What it's for…"
            icon={<ServiceIdentity service={service} />}
            properties={<ServiceProperties service={service} />}
          />
          <div className="flex flex-col gap-8 px-8 py-6">
            {suggestions.length > 0 && (
              <section className="flex flex-col gap-3">
                <h2 className="text-xs font-medium text-muted">Open suggestions</h2>
                {suggestions.map((suggestion) => (
                  <SuggestionCard key={suggestion.id} suggestion={suggestion} />
                ))}
              </section>
            )}
            <RecordList title="Specs" action="+ New spec" onAdd={() => setCreatingSpec(true)}>
              {specs.map((spec) => (
                <SpecListItem key={spec.id} spec={spec} withDomain exceptService={service.id} />
              ))}
            </RecordList>
            <CreateDialog open={creatingSpec} onClose={() => setCreatingSpec(false)}>
              <NewSpecForm serviceIds={[service.id]} onDone={() => setCreatingSpec(false)} />
            </CreateDialog>
            <RecordList
              title="Decisions"
              action="+ Record decision"
              onAdd={() => setRecording(true)}
            >
              {decisions.map((decision) => (
                <DecisionListItem key={decision.id} decision={decision} />
              ))}
            </RecordList>
            {/* The same New decision dialog as on Decisions, starting with this service. */}
            <CreateDialog open={recording} onClose={() => setRecording(false)}>
              <NewDecisionForm serviceId={service.id} onDone={() => setRecording(false)} />
            </CreateDialog>
          </div>
          <div className="flex items-center gap-3 px-8 pb-2">
            <h2 className="text-xs font-medium text-muted">Open issues</h2>
            <SharedFields issues={openIssues} except={["service"]} />
          </div>
          <IssueList issues={openIssues} empty="No open issues for this service." />
          {(showCurrentState || ai) && (
            <div className="flex flex-col gap-8 px-8 py-6">
              {showCurrentState && (
                <div>
                  <MarkdownField
                    label="Current state"
                    value={service.current_state}
                    onSave={(current_state) =>
                      sync.update("Service", service.id, {
                        current_state,
                        current_state_updated_at: new Date().toISOString(),
                      })
                    }
                    empty="What the code does today. Forge's AI keeps this up to date from the code."
                  />
                  {service.current_state_updated_at && (
                    <p
                      className="mt-2 text-xs text-faint"
                      title={new Date(service.current_state_updated_at).toLocaleString()}
                    >
                      Updated {timeAgo(service.current_state_updated_at)}
                    </p>
                  )}
                </div>
              )}
              {ai && (
                <DriftChecks serviceId={service.id} hasRepository={Boolean(service.repository)} />
              )}
            </div>
          )}
        </main>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** The first colour of the palette no service has yet, so new services look different. */
function unusedColor(taken: { color: string }[]): string {
  return COLORS.find((c) => !taken.some((t) => t.color === c)) ?? COLORS[0] ?? "#5b5bd6";
}

/** An icon in its colour on a faint tile of the same colour. */
function IconTile({
  icon,
  color,
  size = "size-9",
  iconSize = "size-5",
}: {
  icon: string;
  color: string;
  size?: string;
  iconSize?: string;
}) {
  return (
    <span
      className={`flex ${size} shrink-0 items-center justify-center rounded-md`}
      style={{ background: `color-mix(in srgb, ${color} 16%, transparent)` }}
    >
      <ServiceIcon service={{ icon, color }} className={iconSize} />
    </span>
  );
}

/**
 * A service's icon on its tile; clicking it picks another icon from the set,
 * searchable by name, and a palette colour or any colour.
 */
function IconPicker({
  icon,
  color,
  onChange,
}: {
  icon: string;
  color: string;
  onChange: (change: { icon?: string; color?: string }) => void;
}) {
  const [query, setQuery] = useState("");
  const words = query.trim().toLowerCase();
  const shown = ICON_NAMES.filter((name) => name.replace(/-/g, " ").includes(words));
  const custom = !COLORS.includes(color);
  const [mixing, setMixing] = useState(false);
  return (
    <Popover.Root
      onOpenChange={(open) => {
        if (!open) return;
        setQuery("");
        setMixing(false);
      }}
    >
      <Popover.Trigger
        aria-label="Icon and colour"
        className="self-start rounded-md outline-none hover:brightness-125 focus-visible:ring-2 focus-visible:ring-accent"
      >
        <IconTile icon={icon} color={color} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Popover.Popup className="flex w-[27rem] flex-col rounded-lg border border-line bg-surface shadow-lg shadow-black/10 outline-none">
            <div className="border-b border-line px-4 py-4">
              {mixing ? (
                <CustomColor
                  value={color}
                  onChange={(c) => onChange({ color: c })}
                  onDone={() => setMixing(false)}
                />
              ) : (
                <div className="flex items-center gap-4">
                  <ColorChoice large value={color} onChange={(c) => onChange({ color: c })} />
                  <span className="h-6 w-px bg-line" />
                  <button
                    type="button"
                    aria-label="Any colour"
                    title="Any colour"
                    onClick={() => setMixing(true)}
                    className={`size-6 rounded-full outline-offset-2 ${custom ? "outline-2 outline-fg" : ""}`}
                    style={{ background: RAINBOW }}
                  />
                </div>
              )}
            </div>
            <input
              // biome-ignore lint/a11y/noAutofocus: the picker opens to search
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search icons…"
              aria-label="Search icons"
              className="h-11 border-b border-line bg-transparent px-4 outline-none placeholder:text-faint"
            />
            <div className="grid max-h-72 grid-cols-12 gap-0.5 overflow-y-auto px-3 py-2.5">
              {shown.map((name) => (
                <button
                  key={name}
                  type="button"
                  title={name.replace(/-/g, " ")}
                  aria-label={`Icon ${name}`}
                  aria-pressed={name === icon}
                  onClick={() => onChange({ icon: name })}
                  className={`flex size-8 items-center justify-center rounded-md hover:bg-hover ${name === icon ? "bg-hover" : ""}`}
                >
                  <ServiceIcon service={{ icon: name, color }} className="size-5" />
                </button>
              ))}
              {shown.length === 0 && (
                <p className="col-span-12 px-1 py-2 text-muted">No icon by that name</p>
              )}
            </div>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

/** A service's icon above its name, to pick another icon or colour. */
function ServiceIdentity({ service }: { service: Service }) {
  const { sync } = useWorkspace();
  return (
    <IconPicker
      icon={service.icon}
      color={service.color}
      onChange={(change) => sync.update("Service", service.id, change)}
    />
  );
}

/** Making a service: its icon, name, what it's for and team, then its page opens. */
function NewServiceForm({ onDone }: { onDone: () => void }) {
  const { sync, workspace } = useWorkspace();
  const services = useRecords("Service");
  const teams = useTeams();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [icon, setIcon] = useState("server");
  const [color, setColor] = useState(() => unusedColor(services));
  const [teamId, setTeamId] = useState(teams.length === 1 ? (teams[0]?.id ?? "") : "");
  const team = teams.find((t) => t.id === teamId);

  const create = () => {
    if (!name.trim()) return;
    const id = sync.create("Service", {
      name: name.trim(),
      description,
      icon,
      color,
      team_id: teamId || undefined,
    });
    onDone();
    void navigate({
      to: "/$workspace/service/$service",
      params: { workspace: workspace.url_key, service: id },
    });
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
        <span>Services</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New service</Dialog.Title>
      </div>

      <div className="flex flex-col gap-3 px-4 py-3">
        <IconPicker
          icon={icon}
          color={color}
          onChange={(change) => {
            if (change.icon) setIcon(change.icon);
            if (change.color) setColor(change.color);
          }}
        />
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Service name"
          aria-label="Service name"
          className="bg-transparent text-lg font-medium outline-none placeholder:text-faint"
        />
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="What it's for… (Markdown)"
          aria-label="What it's for"
          rows={3}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
      </div>

      <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
        <Picker
          label="Team"
          options={[
            { value: "", label: "No team" },
            ...teams.map((t) => ({ value: t.id, label: t.name })),
          ]}
          selected={[teamId]}
          onSelect={setTeamId}
          triggerClassName="h-7 px-2 border border-line rounded-md text-xs"
        >
          {team?.name ?? "Team"}
        </Picker>
      </div>

      <div className="flex items-center justify-end gap-3 border-t border-line px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!name.trim()}>
          Create service
        </Button>
      </div>
    </form>
  );
}

/** A chip that opens a small panel to edit what it shows. */
function ChipPopover({
  label,
  children,
  panel,
}: {
  label: string;
  children: React.ReactNode;
  panel: React.ReactNode;
}) {
  return (
    <Popover.Root>
      <Popover.Trigger aria-label={label} title={label} className={chip}>
        {children}
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Popover.Popup className="flex w-80 flex-col gap-2 rounded-lg border border-line bg-surface p-3 shadow-lg shadow-black/10 outline-none">
            <span className="text-xs font-medium text-muted">{label}</span>
            {panel}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

/**
 * A service's properties as a row of chips under its name, as in Linear,
 * instead of a column of fields: its team, its repository and its paths.
 */
function ServiceProperties({ service }: { service: Service }) {
  const { sync } = useWorkspace();
  const teams = useTeams();
  const { status: github } = useGitHubStatus();
  const repositories = (github?.installations ?? []).flatMap((i) =>
    i.repositories.map((r) => `github:${r.full_name}`),
  );
  const team = teams.find((t) => t.id === service.team_id);
  const repository = service.repository?.replace(/^github:/, "");
  return (
    <div className="-mt-1 -ml-2 flex flex-wrap items-center gap-1">
      <Picker
        label="Team"
        options={[
          { value: "", label: "No team" },
          ...teams.map((t) => ({ value: t.id, label: t.name })),
        ]}
        selected={[service.team_id ?? ""]}
        onSelect={(value) => sync.update("Service", service.id, { team_id: value || null })}
        triggerClassName={chip}
      >
        <UsersRound className="size-3.5" aria-hidden="true" />
        {team?.name ?? "Team"}
      </Picker>
      <ChipPopover
        label="Repository"
        panel={
          <>
            <input
              aria-label="Repository"
              placeholder="github:owner/repo or a folder on the server"
              list="forge-repositories"
              defaultValue={service.repository ?? ""}
              key={service.repository}
              onBlur={(e) => {
                const value = e.target.value.trim();
                if (value !== (service.repository ?? "")) {
                  sync.update("Service", service.id, { repository: value || null });
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
              }}
              className="h-8 w-full rounded-md border border-line-strong bg-transparent px-2 font-mono text-xs outline-none placeholder:font-sans placeholder:text-faint focus:border-accent"
            />
            <datalist id="forge-repositories">
              {repositories.map((r) => (
                <option key={r} value={r} />
              ))}
            </datalist>
          </>
        }
      >
        <GitBranch className="size-3.5" aria-hidden="true" />
        <span className="truncate">{repository ?? "Repository"}</span>
      </ChipPopover>
      <ChipPopover
        label="Paths"
        panel={
          <textarea
            aria-label="Paths"
            placeholder={"One glob per line, e.g.\ncrates/sync/**"}
            defaultValue={service.paths.join("\n")}
            key={service.paths.join("\n")}
            onBlur={(e) => {
              const paths = e.target.value
                .split("\n")
                .map((p) => p.trim())
                .filter(Boolean);
              if (paths.join("\n") !== service.paths.join("\n"))
                sync.update("Service", service.id, { paths });
            }}
            className="field-sizing-content min-h-20 w-full resize-none rounded-md border border-line-strong bg-transparent p-2 font-mono text-xs outline-none placeholder:font-sans placeholder:text-faint focus:border-accent"
          />
        }
      >
        <FolderCode className="size-3.5" aria-hidden="true" />
        {service.paths.length === 0
          ? "Paths"
          : service.paths.length === 1
            ? service.paths[0]
            : `${service.paths.length} paths`}
      </ChipPopover>
    </div>
  );
}

function RecordList({
  title,
  action,
  onAdd,
  children,
}: {
  title: string;
  action: string;
  onAdd: () => void;
  children: React.ReactNode;
}) {
  return (
    <section>
      <div className="mb-1 flex items-center justify-between">
        <h2 className="text-xs font-medium text-muted">{title}</h2>
        <button type="button" className="text-xs text-muted hover:text-fg" onClick={onAdd}>
          {action}
        </button>
      </div>
      <ul className="-mx-3 flex flex-col gap-0.5">{children}</ul>
    </section>
  );
}

export function SpecRow({ spec }: { spec: Spec }) {
  const { workspace } = useWorkspace();
  const status = SPEC_STATUSES.find((s) => s.value === spec.status);
  return (
    <li>
      <Link
        to="/$workspace/spec/$spec"
        params={{ workspace: workspace.url_key, spec: spec.id }}
        className="flex h-9 items-center gap-3 rounded-lg px-3 hover:bg-hover"
      >
        <span className="min-w-0 flex-1 truncate">{spec.title}</span>
        {spec.status !== "active" && <span className="text-xs text-faint">{status?.label}</span>}
      </Link>
    </li>
  );
}

export function DecisionRow({
  decision,
  withService,
}: {
  decision: Decision;
  /** Its service's icon at the end, where decisions of several services share a list. */
  withService?: boolean;
}) {
  const { workspace } = useWorkspace();
  const status = DECISION_STATUSES.find((s) => s.value === decision.status);
  const service = useRecords("Service").find((s) => s.id === decision.service_id);
  return (
    <li>
      <Link
        to="/$workspace/decision/$decision"
        params={{ workspace: workspace.url_key, decision: decision.id }}
        className="flex h-9 items-center gap-3 rounded-lg px-3 hover:bg-hover"
      >
        <span
          className="min-w-0 flex-1 truncate"
          title={decision.decided_on ? `Decided ${formatDate(decision.decided_on)}` : undefined}
        >
          {decision.title}
        </span>
        {decision.status !== "accepted" && (
          <span className="text-xs text-faint">{status?.label}</span>
        )}
        {withService && service && (
          <span title={service.name}>
            <ServiceIcon service={service} />
          </span>
        )}
      </Link>
    </li>
  );
}

/**
 * A decision in a list of its own: its title and the first line of what was
 * decided; its status only when it isn't accepted.
 */
function DecisionListItem({ decision }: { decision: Decision }) {
  const { workspace } = useWorkspace();
  const status = DECISION_STATUSES.find((s) => s.value === decision.status);
  // Older decisions repeat their title as what was decided; that says nothing new.
  const plain = (text: string) =>
    text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, " ")
      .trim();
  const decided = summary(decision.decision);
  const what = plain(decision.title).includes(plain(decided)) ? "" : decided;
  return (
    <li>
      <Link
        to="/$workspace/decision/$decision"
        params={{ workspace: workspace.url_key, decision: decision.id }}
        title={decision.decided_on ? `Decided ${formatDate(decision.decided_on)}` : undefined}
        className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-hover"
      >
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium">{decision.title}</span>
          {what && <span className="truncate text-xs text-muted">{what}</span>}
        </span>
        {decision.status !== "accepted" && (
          <span className="shrink-0 text-xs text-faint">{status?.label}</span>
        )}
      </Link>
    </li>
  );
}

/** What a spec is for: the first line of its Purpose, or of its text. */
function specSummary(body: string): string {
  const purpose = body.split(/^## Purpose\s*$/m)[1];
  return summary((purpose ?? body).replace(/^#.*$/gm, ""));
}

/** In a URL, the "domain" of specs that have none. */
const NO_DOMAIN = "none";

/** `/specs`, from before the Specs page became Domains. */
export function SpecsRedirect() {
  const { workspace } = useWorkspace();
  return <Redirect to="/$workspace/domains" params={{ workspace: workspace.url_key }} />;
}

const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name);

/**
 * The map of the specs: one row per domain, with what it covers and how many
 * specs it has. Opening one shows its specs.
 */
export function DomainsPage() {
  const domains = useRecords("Domain");
  const specs = useRecords("Spec");
  const [creating, setCreating] = useState(false);
  const known = new Set(domains.map((d) => d.id));
  const loose = specs.filter((s) => !s.domain_id || !known.has(s.domain_id));
  return (
    <ListPage
      title="Domains"
      empty={
        domains.length === 0 && specs.length === 0
          ? "No domains yet. A domain is an area of the product, such as Accounts; its specs say how that area works."
          : null
      }
      action={
        <>
          <NewButton label="New domain" onClick={() => setCreating(true)} />
          <CreateDialog open={creating} onClose={() => setCreating(false)}>
            <NewDomainForm onDone={() => setCreating(false)} />
          </CreateDialog>
        </>
      }
    >
      {[...domains].sort(byName).map((domain) => (
        <DomainRow
          key={domain.id}
          domain={domain}
          specs={specs.filter((s) => s.domain_id === domain.id)}
        />
      ))}
      {loose.length > 0 && <DomainRow domain={null} specs={loose} />}
    </ListPage>
  );
}

/**
 * A domain on the Domains page, shaped like a service row: its tile, its name
 * and what it covers (or its spec titles), the icons of the services its specs
 * touch, and how many specs it has.
 */
function DomainRow({ domain, specs }: { domain: Domain | null; specs: Spec[] }) {
  const { workspace } = useWorkspace();
  const touched = useRecords("Service").filter((service) =>
    specs.some((spec) => servicesOf(spec).includes(service.id)),
  );
  const hint = (domain && summary(domain.description)) || specs.map((s) => s.title).join(" · ");
  return (
    <li>
      <Link
        to="/$workspace/domains/$domain"
        params={{ workspace: workspace.url_key, domain: domain?.id ?? NO_DOMAIN }}
        className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-hover"
      >
        <IconTile
          icon={domain?.icon ?? "folder"}
          color={domain?.color ?? "var(--muted)"}
          size="size-8"
          iconSize="size-4"
        />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium">{domain?.name ?? "No domain"}</span>
          {hint && <span className="truncate text-xs text-muted">{hint}</span>}
        </span>
        <span className="flex shrink-0 items-center gap-1.5">
          {touched.map((service) => (
            <span key={service.id} title={service.name}>
              <ServiceIcon service={service} />
            </span>
          ))}
        </span>
        <span className="w-14 shrink-0 text-right text-xs text-faint">
          {count(specs.length, "spec", "specs")}
        </span>
      </Link>
    </li>
  );
}

/**
 * A domain's page: its icon, name and what it covers, edited in place, then
 * its specs. "No domain" lists the specs that have none.
 */
export function DomainPage() {
  const { domain: param } = useParams({ strict: false }) as { domain: string };
  const { sync, workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const domain = useRecords("Domain").find((d) => d.id === param) ?? null;
  const allSpecs = useRecords("Spec");
  const known = new Set(useRecords("Domain").map((d) => d.id));
  const specs = allSpecs
    .filter((s) => (domain ? s.domain_id === domain.id : !s.domain_id || !known.has(s.domain_id)))
    .sort((a, b) => a.title.localeCompare(b.title));
  const [creating, setCreating] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const navigate = useNavigate();
  if (!domain && param !== NO_DOMAIN) return <PageHeader title="Domain not found" />;
  const ws = workspace.url_key;
  return (
    <>
      <PageHeader
        title={domain?.name ?? "No domain"}
        parent={
          <Link to="/$workspace/domains" params={{ workspace: ws }} className="hover:text-fg">
            Domains
          </Link>
        }
      >
        <NewButton label="New spec" onClick={() => setCreating(true)} />
        <CreateDialog open={creating} onClose={() => setCreating(false)}>
          <NewSpecForm domainId={domain?.id ?? null} onDone={() => setCreating(false)} />
        </CreateDialog>
        {domain && isAdmin && (
          <MoreMenu
            label="Domain actions"
            items={[
              {
                label: "Delete domain",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the domain “${domain.name}”?`,
                    body: "Its specs stay, without a domain.",
                    action: "Delete domain",
                    onConfirm: () => {
                      sync.delete("Domain", domain.id);
                      void navigate({ to: "/$workspace/domains", params: { workspace: ws } });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <main className="min-h-0 flex-1 overflow-y-auto">
        {domain ? (
          <EditableHeader
            name={domain.name}
            description={domain.description}
            onName={(name) => sync.update("Domain", domain.id, { name })}
            onDescription={(description) => sync.update("Domain", domain.id, { description })}
            descriptionPlaceholder="Add a description…"
            icon={
              <IconPicker
                icon={domain.icon}
                color={domain.color}
                onChange={(change) => sync.update("Domain", domain.id, change)}
              />
            }
          />
        ) : (
          <div className="flex items-center gap-3 px-8 pt-6 pb-2">
            <IconTile icon="folder" color="var(--muted)" />
            <h2 className="text-2xl font-semibold tracking-tight">No domain</h2>
          </div>
        )}
        <ul className="flex flex-col gap-0.5 p-2 px-5">
          {specs.map((spec) => (
            <SpecListItem key={spec.id} spec={spec} />
          ))}
          {specs.length === 0 && (
            <li className="px-3 py-6 text-muted">No specs in this domain yet.</li>
          )}
        </ul>
      </main>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** Making a domain: its icon and colour, name and what it covers, then its page opens. */
function NewDomainForm({ onDone }: { onDone: () => void }) {
  const { sync, workspace } = useWorkspace();
  const domains = useRecords("Domain");
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [icon, setIcon] = useState("folder");
  const [color, setColor] = useState(() => unusedColor(domains));
  const taken = domains.some((d) => d.name.trim().toLowerCase() === name.trim().toLowerCase());

  const create = () => {
    if (!name.trim() || taken) return;
    const id = sync.create("Domain", { name: name.trim(), description, icon, color });
    onDone();
    void navigate({
      to: "/$workspace/domains/$domain",
      params: { workspace: workspace.url_key, domain: id },
    });
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
        <span>Domains</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New domain</Dialog.Title>
      </div>
      <div className="flex flex-col gap-3 px-4 py-3">
        <div className="flex items-center gap-3">
          <IconPicker
            icon={icon}
            color={color}
            onChange={(change) => {
              if (change.icon) setIcon(change.icon);
              if (change.color) setColor(change.color);
            }}
          />
          <input
            // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Domain name, such as Accounts"
            aria-label="Domain name"
            className="min-w-0 flex-1 bg-transparent text-lg font-medium outline-none placeholder:text-faint"
          />
        </div>
        {taken && <p className="text-xs text-warn">There's already a domain with that name.</p>}
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="What this area covers… (Markdown)"
          aria-label="What this area covers"
          rows={3}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
      </div>
      <div className="flex items-center justify-end gap-3 border-t border-line px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!name.trim() || taken}>
          Create domain
        </Button>
      </div>
    </form>
  );
}

/** A spec's domain, picked from the domains. */
function DomainPicker({
  value,
  onChange,
  triggerClassName,
  empty,
}: {
  value: string | null;
  onChange: (id: string | null) => void;
  triggerClassName: string;
  empty: React.ReactNode;
}) {
  const domains = [...useRecords("Domain")].sort(byName);
  const current = domains.find((d) => d.id === value);
  return (
    <Picker
      label="Domain"
      options={[
        { value: "", label: "No domain" },
        ...domains.map((d) => ({ value: d.id, label: d.name, icon: <ServiceIcon service={d} /> })),
      ]}
      selected={[value ?? ""]}
      onSelect={(id) => onChange(id || null)}
      triggerClassName={triggerClassName}
    >
      {current ? (
        <>
          <ServiceIcon service={current} />
          {current.name}
        </>
      ) : (
        empty
      )}
    </Picker>
  );
}

/**
 * A spec on its domain's page: its title and what it's for, what wants
 * attention, and the icons of the services it belongs to.
 */
function SpecListItem({
  spec,
  withDomain = false,
  exceptService,
}: {
  spec: Spec;
  /** Its domain's tile in front, where specs from several domains share a list. */
  withDomain?: boolean;
  /** The service whose page this is, which every spec here belongs to. */
  exceptService?: string;
}) {
  const { workspace } = useWorkspace();
  const pending = usePendingChanges(spec);
  const domain = useRecords("Domain").find((d) => d.id === spec.domain_id);
  const services = useRecords("Service").filter(
    (s) => s.id !== exceptService && servicesOf(spec).includes(s.id),
  );
  const status = SPEC_STATUSES.find((s) => s.value === spec.status);
  const about = specSummary(spec.body);
  return (
    <li>
      <Link
        to="/$workspace/spec/$spec"
        params={{ workspace: workspace.url_key, spec: spec.id }}
        className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-hover"
      >
        {withDomain && (
          <span title={domain?.name ?? "No domain"}>
            <IconTile
              icon={domain?.icon ?? "folder"}
              color={domain?.color ?? "var(--muted)"}
              size="size-8"
              iconSize="size-4"
            />
          </span>
        )}
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium">{spec.title}</span>
          {about && <span className="truncate text-xs text-muted">{about}</span>}
        </span>
        {pending.length > 0 && (
          <span className="shrink-0 text-xs text-warn">
            {pending.length === 1 ? "1 change on the way" : `${pending.length} changes on the way`}
          </span>
        )}
        {spec.status !== "active" && (
          <span className="shrink-0 text-xs text-faint">{status?.label}</span>
        )}
        <span className="flex shrink-0 items-center gap-1.5">
          {services.map((service) => (
            <span key={service.id} title={service.name}>
              <ServiceIcon service={service} />
            </span>
          ))}
        </span>
      </Link>
    </li>
  );
}

/** Making a spec: its title and service, then its page opens to write it. */
function NewSpecForm({
  domainId: inDomain = null,
  serviceIds: inServices = [],
  onDone,
}: {
  /** The domain it starts in, e.g. the one whose page it was made on. */
  domainId?: string | null;
  /** The services it starts with, e.g. the one whose page it was made on. */
  serviceIds?: string[];
  onDone: () => void;
}) {
  const { sync, workspace } = useWorkspace();
  const services = useRecords("Service");
  const navigate = useNavigate();
  const [title, setTitle] = useState("");
  const [serviceIds, setServiceIds] = useState<string[]>(inServices);
  const [domainId, setDomainId] = useState<string | null>(inDomain);
  const chosen = services.filter((s) => serviceIds.includes(s.id));
  // Where it's made: its domain's page, a service's page, or else Domains.
  const startedIn =
    useRecords("Domain").find((d) => d.id === inDomain)?.name ??
    services.find((s) => s.id === inServices[0])?.name ??
    "Domains";

  const create = () => {
    if (!title.trim()) return;
    const id = sync.create("Spec", {
      title: title.trim(),
      service_ids: serviceIds,
      domain_id: domainId ?? undefined,
    });
    onDone();
    void navigate({
      to: "/$workspace/spec/$spec",
      params: { workspace: workspace.url_key, spec: id },
    });
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
        <span>{startedIn}</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New spec</Dialog.Title>
      </div>
      <div className="flex flex-col gap-2 px-4 py-3">
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a title
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Spec title, such as “Invite links”"
          aria-label="Spec title"
          className="bg-transparent text-lg font-medium outline-none placeholder:text-faint"
        />
        <p className="text-muted">You write what it says on its page, in OpenSpec's convention.</p>
      </div>
      <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
        <DomainPicker
          value={domainId}
          onChange={setDomainId}
          triggerClassName="h-7 px-2 border border-line rounded-md text-xs"
          empty="Domain"
        />
        <Picker
          label="Services"
          multiple
          options={services.map((s) => ({
            value: s.id,
            label: s.name,
            icon: <ServiceIcon service={s} />,
          }))}
          selected={serviceIds}
          onSelect={(id) =>
            setServiceIds((ids) => (ids.includes(id) ? ids.filter((s) => s !== id) : [...ids, id]))
          }
          triggerClassName="h-7 px-2 border border-line rounded-md text-xs"
        >
          {chosen.length === 0
            ? "Services"
            : chosen.map((s) => <ServiceIcon key={s.id} service={s} />)}
          {chosen.length === 1 && chosen[0]?.name}
        </Picker>
      </div>
      <div className="flex items-center justify-end gap-3 border-t border-line px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!title.trim()}>
          Create spec
        </Button>
      </div>
    </form>
  );
}

export function SpecPage() {
  const { spec: id } = useParams({ strict: false }) as { spec: string };
  const { sync, workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const spec = useRecords("Spec").find((s) => s.id === id);
  const domain = useRecords("Domain").find((d) => d.id === spec?.domain_id);
  const users = useRecords("User");
  const navigate = useNavigate();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const versions = useSpecHistory(spec);
  if (!spec) return <PageHeader title="Spec not found" />;
  const author = users.find((u) => u.id === spec.created_by);
  const ws = workspace.url_key;
  return (
    <>
      <PageHeader
        title={spec.title}
        parent={
          <Link
            to="/$workspace/domains/$domain"
            params={{ workspace: ws, domain: domain?.id ?? NO_DOMAIN }}
            className="flex items-center gap-1.5 hover:text-fg"
          >
            {domain && <ServiceIcon service={domain} />}
            {domain?.name ?? "No domain"}
          </Link>
        }
      >
        {isAdmin && (
          <MoreMenu
            label="Spec actions"
            items={[
              {
                label: "Delete spec",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the spec “${spec.title}”?`,
                    body: "It's deleted for everyone, and issues no longer show it. This can't be undone.",
                    action: "Delete spec",
                    onConfirm: () => {
                      sync.delete("Spec", spec.id);
                      void navigate({
                        to: "/$workspace/domains/$domain",
                        params: { workspace: ws, domain: domain?.id ?? NO_DOMAIN },
                      });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <main className="min-h-0 flex-1 overflow-y-auto">
        <article className="spec-doc flex flex-col">
        <EditableHeader
          name={spec.title}
          description=""
          onName={(title) => sync.update("Spec", spec.id, { title })}
          onDescription={() => {}}
          hideDescription
          properties={
            <div className="-ml-1 flex flex-wrap items-center gap-1">
              <StatusChip
                statuses={SPEC_STATUSES}
                value={spec.status}
                onChange={(status) =>
                  sync.update("Spec", spec.id, { status: status as SpecStatus })
                }
              />
              <DomainPicker
                value={spec.domain_id ?? null}
                onChange={(domain_id) => sync.update("Spec", spec.id, { domain_id })}
                triggerClassName={chip}
                empty={
                  <>
                    <Folders className="size-3.5" aria-hidden="true" />
                    Domain
                  </>
                }
              />
              <ServicesChip
                value={servicesOf(spec)}
                onChange={(service_ids) => sync.update("Spec", spec.id, { service_ids })}
              />
              <ProjectChip
                value={spec.project_id}
                onChange={(project_id) => sync.update("Spec", spec.id, { project_id })}
              />
            </div>
          }
        />
        <div className="px-8 pb-16">
          <div className="border-t border-line pt-6">
          <ChangesOnTheWay spec={spec} />
          <MarkdownField
            label="Spec"
            hideLabel
            value={spec.body}
            onSave={(body) => sync.update("Spec", spec.id, { body })}
            empty="Write how this should work, in Markdown."
            notes={(body) => <ConventionWarnings body={body} />}
            view={(body) => <SpecText spec={spec} body={body} versions={versions} />}
          />
          <SpecHistory versions={versions} />
          <p className="mt-6 flex items-center gap-2 text-xs text-faint">
            <Avatar user={author} size="size-4" />
            <span>
              Written by {author?.name ?? "someone"} ·{" "}
              <time dateTime={spec.updated_at} title={new Date(spec.updated_at).toLocaleString()}>
                updated {timeAgo(spec.updated_at)}
              </time>
            </span>
          </p>
          </div>
        </div>
        </article>
      </main>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** An issue's identifier and title, linking to it. */
function IssueName({ issue }: { issue: Issue }) {
  const { workspace } = useWorkspace();
  const teams = byId(useRecords("Team"));
  const id = identifier(issue, teams);
  return (
    <Link
      to="/$workspace/issue/$identifier"
      params={{ workspace: workspace.url_key, identifier: id }}
      className="hover:underline"
    >
      <span className="text-muted">{id}</span> {issue.title}
    </Link>
  );
}

/**
 * The versions of the spec's text, oldest first, refreshed whenever the
 * server has a new version of it. `null` when they couldn't be fetched.
 */
function useSpecHistory(spec: Spec | undefined): SpecVersion[] | null | undefined {
  const { workspace } = useWorkspace();
  const [loaded, setLoaded] = useState<{ id: string; versions: SpecVersion[] | null }>();
  const id = spec?.id;
  const updatedAt = spec?.updated_at;
  const online = useSyncStatus().connection === "online";
  useEffect(() => {
    // Re-run when the server's copy changes, so the new version is in the log,
    // and when the connection comes back.
    void updatedAt;
    if (!id || !online) return;
    let current = true;
    api<SpecVersion[]>("GET", `/api/w/${workspace.id}/specs/${id}/history`)
      .then((versions) => current && setLoaded({ id, versions }))
      // A history already shown stays; only a first try that fails says so.
      .catch(() => current && setLoaded((was) => (was?.id === id ? was : { id, versions: null })));
    return () => {
      current = false;
    };
  }, [workspace.id, id, updatedAt, online]);
  // Another spec's history isn't this one's.
  return loaded && loaded.id === id ? loaded.versions : undefined;
}

/** The spec changes of issues not done or canceled yet that will change the spec (FOR-75). */
function usePendingChanges(spec: Spec): (PendingChange & { issue: Issue })[] {
  const changes = useRecords("SpecChange");
  const issues = byId(useRecords("Issue"));
  const states = useStates();
  const open = (id: string) => {
    const issue = issues.get(id);
    const type = issue && states.get(issue.state_id)?.type;
    return type !== undefined && type !== "completed" && type !== "canceled";
  };
  return pendingChanges(spec, changes, open).flatMap((pending) => {
    const issue = pending.change.issue_id ? issues.get(pending.change.issue_id) : undefined;
    return issue ? [{ ...pending, issue }] : [];
  });
}

/**
 * Changes on the way that the requirement rows can't show: rewrites, which
 * change more than requirements, and any change to a spec without them.
 */
function ChangesOnTheWay({ spec }: { spec: Spec }) {
  const pending = usePendingChanges(spec);
  const states = useStates();
  const hasRequirements = requirements(spec.body).length > 0;
  const shown = pending.filter(
    ({ change, touches, adds }) =>
      !hasRequirements ||
      change.specs.some((d) => d.spec_id === spec.id && d.rewrite) ||
      (touches.length === 0 && adds.length === 0),
  );
  if (shown.length === 0) return null;
  return (
    <section className="mb-6">
      <h2 className="mb-1 text-sm font-medium">On the way</h2>
      <p className="mb-2 text-xs text-muted">
        These issues will change this spec. Each change takes effect when its issue is done.
      </p>
      <ul className="flex flex-col gap-2 text-sm">
        {shown.map(({ change, issue, what }) => {
          const state = states.get(issue.state_id);
          return (
            <li key={change.id} className="flex flex-col">
              <span className="flex items-center gap-2">
                <span className="min-w-0 truncate">
                  <IssueName issue={issue} />
                </span>
                {state && (
                  <span className="flex shrink-0 items-center gap-1 text-xs text-muted">
                    <StateIcon state={state} />
                    {state.name}
                  </span>
                )}
              </span>
              <span className="text-muted">{what}</span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** Who made a version: the issue whose spec change it was, or the person who saved it. */
function VersionAuthor({ version }: { version: SpecVersion }) {
  const changes = byId(useRecords("SpecChange"));
  const issues = byId(useRecords("Issue"));
  const users = byId(useRecords("User"));
  if (version.spec_change_id) {
    const change = changes.get(version.spec_change_id);
    const issue = change?.issue_id ? issues.get(change.issue_id) : undefined;
    // Its issue may be deleted since; what it did stays in the history.
    return issue ? <IssueName issue={issue} /> : <>An issue since deleted</>;
  }
  return <>{users.get(version.actor_id)?.name ?? "Someone"}</>;
}

/** Each version of the spec's text, newest first, opening to what it changed. */
function SpecHistory({ versions }: { versions: SpecVersion[] | null | undefined }) {
  if (versions === undefined) return null;
  return (
    <section className="mt-10 border-t border-line pt-6">
      <h2 className="mb-3 text-[13px] font-medium text-muted">History</h2>
      {versions === null ? (
        <p className="text-xs text-muted">The history shows once Forge is reachable.</p>
      ) : (
        <ul className="-mx-3 flex flex-col gap-0.5 text-sm">
          {historyEntries(versions).map((entry, index) => (
            <HistoryRow
              // Versions have no id, and two can share a time; newest first, so
              // the oldest keeps its key as new ones arrive.
              // biome-ignore lint/suspicious/noArrayIndexKey: counted from the oldest, so stable
              key={versions.length - index}
              entry={entry}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function HistoryRow({ entry: { version, before, what } }: { entry: HistoryEntry }) {
  const actor = byId(useRecords("User")).get(version.actor_id);
  const line = (
    <>
      <Avatar user={actor} />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline justify-between gap-3">
          <VersionAuthor version={version} />
          <time
            className="shrink-0 text-xs text-faint tabular-nums"
            dateTime={version.at}
            title={new Date(version.at).toLocaleString()}
          >
            {timeAgo(version.at)}
          </time>
        </span>
        <span className="mt-0.5 block text-xs leading-relaxed text-muted">{what}</span>
      </span>
    </>
  );
  if (version.created) return <li className="flex items-start gap-2.5 px-2 py-2">{line}</li>;
  return (
    <li>
      <details>
        <summary className="flex cursor-pointer list-none items-start gap-2.5 rounded-md px-2 py-2 hover:bg-hover [&::-webkit-details-marker]:hidden">
          {line}
        </summary>
        <div className="px-3 pb-3">
          <TextDiff label={what} before={before} after={version.body} />
        </div>
      </details>
    </li>
  );
}

/** Where a behaviour spec doesn't follow OpenSpec's convention; prose shows nothing. */
function ConventionWarnings({ body }: { body: string }) {
  const warnings = conventionWarnings(body);
  if (warnings.length === 0) return null;
  return (
    <div className="rounded-md border border-line px-3 py-2 text-xs">
      <p className="text-warn">
        This spec doesn't follow OpenSpec's convention for behaviour specs yet. It's saved anyway.
      </p>
      <ul className="mt-1 list-disc pl-4 text-muted">
        {warnings.map((warning) => (
          <li key={warning}>{warning}</li>
        ))}
      </ul>
    </div>
  );
}

/**
 * A spec as one text: its Purpose, then each requirement as a row that opens
 * to its sentence and scenarios, then whatever follows. Prose shows as is.
 */
function SpecText({
  spec,
  body,
  versions,
}: {
  spec: Spec;
  body: string;
  versions: SpecVersion[] | null | undefined;
}) {
  const { before, after } = aroundRequirements(body);
  if (requirements(body).length === 0) return <Markdown text={body} />;
  return (
    <>
      {before && <Markdown text={before} />}
      <Requirements spec={spec} body={body} versions={versions} />
      {after && <Markdown text={after} />}
    </>
  );
}

const UPCOMING = { change: "change", remove: "remove", rename: "rename" } as const;

/**
 * Each requirement of a spec, opening to its text, what open issues will do
 * to it, and the latest drift check's verdict on it. Requirements open issues
 * will add follow, faint, until they're done.
 */
function Requirements({
  spec,
  body,
  versions,
}: {
  spec: Spec;
  body: string;
  versions: SpecVersion[] | null | undefined;
}) {
  const verdicts = useRecords("RequirementVerdict").filter((v) => v.spec_id === spec.id);
  const services = useRecords("Service").filter((s) => servicesOf(spec).includes(s.id));
  // The service whose code changed last decides whether a verdict is stale.
  const service = [...services].sort((a, b) =>
    (b.code_changed_at ?? "").localeCompare(a.code_changed_at ?? ""),
  )[0];
  const statuses = requirementStatuses(body, verdicts, service);
  const pending = usePendingChanges(spec);
  const teams = byId(useRecords("Team"));
  const changedIn = useMemo(() => lastChanged(versions ?? []), [versions]);
  // What the server holds issues back for: the same requirement changed or
  // added by two open issues, or a rewrite of the spec another one changes.
  const conflicts = specConflicts(useOpenSpecChanges()).filter((c) => c.spec_id === spec.id);
  const conflicted = new Set(conflicts.flatMap((c) => c.requirement ?? []));
  const rewritten = conflicts.some((c) => c.requirement === null);
  if (statuses.length === 0) return null;
  const upcoming = (name: string) =>
    pending.flatMap(({ change, issue, touches }) =>
      touches
        .filter((touch) => touch.name === name)
        .map((touch) => ({ key: `${change.id}-${touch.upcoming}`, issue, touch })),
    );
  const added = pending.flatMap(({ change, issue, adds }) =>
    adds.map((requirement) => ({ key: `${change.id}-${requirement.name}`, issue, requirement })),
  );
  const count = (verdict: string | null) =>
    statuses.filter((s) => (s.verdict?.verdict ?? null) === verdict).length;
  const summary = [
    ...Object.entries(VERDICTS).map(([verdict, { label }]) => [count(verdict), label] as const),
    [count(null), "not checked yet"] as const,
  ]
    .filter(([n]) => n > 0)
    .map(([n, label]) => `${n} ${label.toLowerCase()}`)
    .join(" · ");
  return (
    <section className="mt-8 mb-2 border-t border-line pt-6">
      <h2 className="mb-2 flex items-baseline gap-2 text-[13px] font-medium text-muted">
        Requirements
        <span className="text-xs font-normal text-faint">{summary}</span>
      </h2>
      {!service && (
        <p className="mb-2 text-xs text-muted">
          Drift checks give each requirement a verdict once the spec belongs to a service.
        </p>
      )}
      {rewritten && (
        <p className="mb-2 text-xs text-danger">
          An open issue rewrites this spec while another one changes it. Only one issue changes a
          spec's requirements at a time: the one that came after can't start until they settle it.
        </p>
      )}
      <ul className="-mx-3 flex flex-col gap-0.5">
        {statuses.map(({ requirement, verdict, stale }) => {
          const shown = verdict ? VERDICTS[verdict.verdict] : null;
          const coming = upcoming(requirement.name);
          const clash = coming.length > 1 || conflicted.has(requirement.name);
          const changed = changedIn.get(requirement.name);
          return (
            <li key={requirement.name}>
              <details>
                <summary className="flex cursor-pointer list-none items-center gap-2.5 rounded-md px-2 py-1.5 text-sm hover:bg-hover [&::-webkit-details-marker]:hidden">
                  {/* Colour only once a check has said something; until then an empty ring, like an unchecked status. */}
                  {shown ? (
                    <Dot color={shown.color} />
                  ) : (
                    <span
                      className="size-2 shrink-0 rounded-full border border-line-strong"
                      aria-hidden="true"
                    />
                  )}
                  <span className="min-w-0 flex-1 truncate">{requirement.name}</span>
                  {coming.map(({ key, issue, touch }) => (
                    <span
                      key={key}
                      className={`shrink-0 text-xs ${clash ? "text-danger" : "text-warn"}`}
                    >
                      {identifier(issue, teams)} will {UPCOMING[touch.upcoming]} this
                    </span>
                  ))}
                  {stale && (
                    <span className="text-xs text-faint">
                      {stale === "requirement" ? "requirement changed since" : "code changed since"}
                    </span>
                  )}
                  <span className={`text-xs ${stale ? "text-faint" : "text-muted"}`}>
                    {shown?.label}
                  </span>
                </summary>
                <div className="flex flex-col gap-2 px-3 pb-3 pl-7 text-sm">
                  <Markdown text={requirementBody(requirement)} />
                  {clash && (
                    <p className="text-xs text-danger">
                      Only one issue changes a requirement at a time. The ones that came after the
                      first can't start until they settle it.
                    </p>
                  )}
                  {coming.map(({ key, issue, touch }) => (
                    <div key={key} className="flex flex-col gap-1">
                      <p className="text-xs text-warn">
                        <IssueName issue={issue} />{" "}
                        {touch.upcoming === "change"
                          ? "will change it, once it's done:"
                          : touch.upcoming === "remove"
                            ? "will remove it, once it's done."
                            : `will rename it to “${touch.to}”, once it's done.`}
                      </p>
                      {touch.upcoming === "change" && touch.after && (
                        <TextDiff
                          label={`What ${identifier(issue, teams)} changes in “${requirement.name}”`}
                          before={requirement.text}
                          after={touch.after}
                        />
                      )}
                    </div>
                  ))}
                  {verdict ? (
                    <>
                      <Markdown text={verdict.evidence} />
                      <p className="text-xs text-faint">
                        Checked {timeAgo(verdict.checked_at)}
                        {verdict.commit_sha && ` at ${verdict.commit_sha.slice(0, 7)}`}
                        {stale &&
                          (stale === "requirement"
                            ? ". The requirement was edited after that; the next check judges it again."
                            : ". The code changed after that; the next check judges it again.")}
                      </p>
                    </>
                  ) : (
                    <p className="text-xs text-muted">
                      The next drift check of the service gives this one a verdict.
                    </p>
                  )}
                  {changed && (
                    <p className="text-xs text-faint">
                      Last changed by <VersionAuthor version={changed} /> · {timeAgo(changed.at)}
                    </p>
                  )}
                </div>
              </details>
            </li>
          );
        })}
        {added.map(({ key, issue, requirement }) => (
          <li key={key}>
            <details>
              <summary className="flex cursor-pointer list-none items-center gap-2 rounded-lg px-3 py-2 text-sm text-muted hover:bg-hover [&::-webkit-details-marker]:hidden">
                <span aria-hidden="true" className="w-2 shrink-0 text-center text-warn">
                  +
                </span>
                <span className="min-w-0 flex-1 truncate">{requirement.name}</span>
                <span
                  className={`shrink-0 text-xs ${conflicted.has(requirement.name) ? "text-danger" : "text-warn"}`}
                >
                  {identifier(issue, teams)} will add this
                </span>
              </summary>
              <div className="flex flex-col gap-2 px-3 pb-3 pl-7 text-sm">
                <p className="text-xs text-warn">
                  <IssueName issue={issue} /> will add it, once it's done.
                </p>
                {conflicted.has(requirement.name) && (
                  <p className="text-xs text-danger">
                    Another open issue adds or changes a requirement with this name. Only one issue
                    changes a requirement at a time: the one that came after can't start until they
                    settle it.
                  </p>
                )}
                <Markdown text={requirementBody(requirement)} />
              </div>
            </details>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function DecisionsPage() {
  const decisions = useRecords("Decision");
  const [creating, setCreating] = useState(false);
  const newestFirst = [...decisions].sort((a, b) =>
    (b.decided_on ?? b.created_at).localeCompare(a.decided_on ?? a.created_at),
  );
  const inForce = newestFirst.filter((d) => d.status === "accepted" || d.status === "proposed");
  const past = newestFirst.filter((d) => d.status !== "accepted" && d.status !== "proposed");
  return (
    <ListPage
      title="Decisions"
      empty={
        decisions.length === 0
          ? "No decisions yet. Record what was decided, so Forge can tell when the code or new issues stop respecting it."
          : null
      }
      action={
        <>
          <NewButton label="New decision" onClick={() => setCreating(true)} />
          <CreateDialog open={creating} onClose={() => setCreating(false)}>
            <NewDecisionForm onDone={() => setCreating(false)} />
          </CreateDialog>
        </>
      }
    >
      {inForce.map((decision) => (
        <DecisionRow key={decision.id} decision={decision} withService />
      ))}
      {past.length > 0 && <li className="px-3 pt-5 pb-1 text-xs text-muted">No longer in force</li>}
      {past.map((decision) => (
        <DecisionRow key={decision.id} decision={decision} withService />
      ))}
    </ListPage>
  );
}

/** Recording a decision: its title, what was decided and its service, then its page opens. */
function NewDecisionForm({
  serviceId: inService = null,
  onDone,
}: {
  /** The service it starts with, e.g. the one whose page it was made on. */
  serviceId?: string | null;
  onDone: () => void;
}) {
  const { sync, workspace } = useWorkspace();
  const navigate = useNavigate();
  const [title, setTitle] = useState("");
  const [decided, setDecided] = useState("");
  const [serviceId, setServiceId] = useState<string | null>(inService);

  const create = () => {
    if (!title.trim()) return;
    const id = sync.create("Decision", {
      title: title.trim(),
      decision: decided,
      service_id: serviceId ?? undefined,
      decided_on: new Date().toISOString().slice(0, 10),
    });
    onDone();
    void navigate({
      to: "/$workspace/decision/$decision",
      params: { workspace: workspace.url_key, decision: id },
    });
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
        <span>Decisions</span>
        <span>›</span>
        <Dialog.Title className="font-normal">New decision</Dialog.Title>
      </div>
      <div className="flex flex-col gap-2 px-4 py-3">
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a title
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Decision title, such as “Email and password login first”"
          aria-label="Decision title"
          className="bg-transparent text-lg font-medium outline-none placeholder:text-faint"
        />
        <textarea
          value={decided}
          onChange={(e) => setDecided(e.target.value)}
          placeholder="What was decided… (Markdown)"
          aria-label="What was decided"
          rows={3}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
      </div>
      <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
        <ServiceChip value={serviceId} onChange={setServiceId} />
      </div>
      <div className="flex items-center justify-end gap-3 border-t border-line px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!title.trim()}>
          Record decision
        </Button>
      </div>
    </form>
  );
}

export function DecisionPage() {
  const { decision: id } = useParams({ strict: false }) as { decision: string };
  const { sync, workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const decisions = useRecords("Decision");
  const decision = decisions.find((d) => d.id === id);
  const navigate = useNavigate();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  if (!decision) return <PageHeader title="Decision not found" />;
  const update = (patch: DecisionPatch) => sync.update("Decision", decision.id, patch);
  const replacement = decisions.find((d) => d.id === decision.superseded_by_id);
  const replaces = decisions.filter((d) => d.superseded_by_id === decision.id);
  const ws = workspace.url_key;

  return (
    <>
      <PageHeader
        title={decision.title}
        parent={
          <Link to="/$workspace/decisions" params={{ workspace: ws }} className="hover:text-fg">
            Decisions
          </Link>
        }
      >
        {isAdmin && (
          <MoreMenu
            label="Decision actions"
            items={[
              {
                label: "Delete decision",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete the decision “${decision.title}”?`,
                    body: "It's deleted for everyone, and issues no longer show it. This can't be undone.",
                    action: "Delete decision",
                    onConfirm: () => {
                      sync.delete("Decision", decision.id);
                      void navigate({ to: "/$workspace/decisions", params: { workspace: ws } });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <main className="min-h-0 flex-1 overflow-y-auto">
        <EditableHeader
          name={decision.title}
          description=""
          onName={(title) => update({ title })}
          onDescription={() => {}}
          hideDescription
          properties={
            <div className="-ml-2 flex flex-wrap items-center gap-1">
              <StatusChip
                statuses={DECISION_STATUSES}
                value={decision.status}
                onChange={(status) => update({ status: status as DecisionStatus })}
              />
              <DateChip
                label="Decided on"
                value={decision.decided_on}
                onChange={(decided_on) => update({ decided_on })}
              />
              <ServiceChip
                value={decision.service_id}
                onChange={(service_id) => update({ service_id })}
              />
              <ProjectChip
                value={decision.project_id}
                onChange={(project_id) => update({ project_id })}
              />
              <Picker
                label="Superseded by"
                options={[
                  { value: "", label: "Nothing" },
                  ...decisions
                    .filter((d) => d.id !== decision.id)
                    .map((d) => ({ value: d.id, label: d.title })),
                ]}
                selected={[decision.superseded_by_id ?? ""]}
                onSelect={(value) =>
                  update(
                    value
                      ? { superseded_by_id: value, status: "superseded" }
                      : { superseded_by_id: null },
                  )
                }
                triggerClassName={chip}
              >
                <Replace className="size-3.5" aria-hidden="true" />
                <span className="truncate">
                  {replacement ? `Superseded by ${replacement.title}` : "Superseded by…"}
                </span>
              </Picker>
            </div>
          }
        />
        <div className="flex flex-col gap-8 px-8 py-6">
          {replacement && (
            <p className="text-muted">
              Superseded by{" "}
              <Link
                to="/$workspace/decision/$decision"
                params={{ workspace: ws, decision: replacement.id }}
                className="text-fg underline underline-offset-4"
              >
                {replacement.title}
              </Link>
            </p>
          )}
          <MarkdownField
            label="Context"
            value={decision.context}
            onSave={(context) => update({ context })}
            empty="The situation that called for a decision."
          />
          <MarkdownField
            label="Decision"
            value={decision.decision}
            onSave={(text) => update({ decision: text })}
            empty="What was decided."
          />
          <MarkdownField
            label="Consequences"
            value={decision.consequences}
            onSave={(consequences) => update({ consequences })}
            empty="What follows from it."
          />
          {replaces.length > 0 && (
            <section>
              <h2 className="mb-1 text-xs font-medium text-muted">Replaces</h2>
              <ul className="-mx-3 flex flex-col gap-0.5">
                {replaces.map((old) => (
                  <DecisionListItem key={old.id} decision={old} />
                ))}
              </ul>
            </section>
          )}
        </div>
      </main>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** The services a spec belongs to, as a chip: their icons, and names while there are few. */
function ServicesChip({ value, onChange }: { value: string[]; onChange: (ids: string[]) => void }) {
  const services = useRecords("Service");
  const chosen = services.filter((s) => value.includes(s.id));
  return (
    <Picker
      label="Services"
      multiple
      options={services.map((s) => ({
        value: s.id,
        label: s.name,
        icon: <ServiceIcon service={s} />,
      }))}
      selected={value}
      onSelect={(id) =>
        onChange(value.includes(id) ? value.filter((s) => s !== id) : [...value, id])
      }
      triggerClassName={chip}
    >
      {chosen.length === 0 ? (
        <>
          <ServiceIcon />
          Services
        </>
      ) : (
        <>
          {chosen.map((s) => (
            <ServiceIcon key={s.id} service={s} />
          ))}
          <span className="truncate">
            {chosen.length <= 2
              ? chosen.map((s) => s.name).join(", ")
              : `${chosen.length} services`}
          </span>
        </>
      )}
    </Picker>
  );
}

/** One service, as a chip. */
function ServiceChip({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  const services = useRecords("Service");
  const current = services.find((s) => s.id === value);
  return (
    <Picker
      label="Service"
      options={[
        { value: "", label: "No service" },
        ...services.map((s) => ({ value: s.id, label: s.name, icon: <ServiceIcon service={s} /> })),
      ]}
      selected={[value ?? ""]}
      onSelect={(id) => onChange(id || null)}
      triggerClassName={chip}
    >
      <ServiceIcon service={current} />
      {current?.name ?? "Service"}
    </Picker>
  );
}

/** A project, as a chip. */
function ProjectChip({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  const projects = useRecords("Project");
  const current = projects.find((p) => p.id === value);
  return (
    <Picker
      label="Project"
      options={[
        { value: "", label: "No project" },
        ...projects.map((p) => ({ value: p.id, label: p.name, icon: <ProjectIcon /> })),
      ]}
      selected={[value ?? ""]}
      onSelect={(id) => onChange(id || null)}
      triggerClassName={chip}
    >
      <ProjectIcon />
      <span className="truncate">{current?.name ?? "Project"}</span>
    </Picker>
  );
}
