// Pickers for an issue's fields. Each comes compact (an icon, for list rows)
// or as a chip (icon and text, in the row of fields on the issue page).
import type { Issue } from "@forge/types";
import { useMemo } from "react";
import { AvatarWorkDot } from "./agent-work";
import { statusStyle } from "./contrast";
import { byId, PRIORITY_LABELS, useLabels, useStates, useTeamStates, useUsers } from "./data";
import { Avatar, LabelDot, PriorityIcon, ProjectIcon, ServiceIcon, StateIcon } from "./icons";
import { chip } from "./page-parts";
import { Picker } from "./picker";
import { useRecords, useWorkspace } from "./workspace";

type Variant = "compact" | "chip";

// The issue page's chips look like every record page's.
const chipTrigger = chip;
const compactTrigger = "size-7 shrink-0 justify-center";

/** A chip's text; fainter while the field is empty, so set fields stand out. */
function ChipText({ empty, children }: { empty?: boolean; children: React.ReactNode }) {
  return <span className={`max-w-48 truncate ${empty ? "text-faint" : ""}`}>{children}</span>;
}

export function StatusPicker({ issue, variant = "compact" }: { issue: Issue; variant?: Variant }) {
  const { sync } = useWorkspace();
  const states = useTeamStates(issue.team_id);
  const current = useStates().get(issue.state_id);
  return (
    <Picker
      label="Status"
      options={states.map((s) => ({
        value: s.id,
        label: s.name,
        icon: <StateIcon state={s} />,
      }))}
      selected={[issue.state_id]}
      onSelect={(state_id) => sync.update("Issue", issue.id, { state_id })}
      triggerClassName={
        variant === "chip"
          ? "ui-status flex h-7 items-center gap-1.5 px-2 outline-none focus-visible:ring-2 focus-visible:ring-accent"
          : compactTrigger
      }
      triggerStyle={variant === "chip" ? statusStyle(current?.color) : undefined}
    >
      {current && (
        <StateIcon
          state={current}
          color={variant === "chip" ? "currentColor" : undefined}
          size={variant === "chip" ? 12 : 15}
        />
      )}
      {variant === "chip" && <ChipText>{current?.name ?? "Status"}</ChipText>}
    </Picker>
  );
}

export function PriorityPicker({
  issue,
  variant = "compact",
}: {
  issue: Issue;
  variant?: Variant;
}) {
  const { sync } = useWorkspace();
  return (
    <Picker
      label="Priority"
      options={[0, 1, 2, 3, 4].map((p) => ({
        value: String(p),
        label: PRIORITY_LABELS[p] ?? "",
        icon: <PriorityIcon priority={p} />,
      }))}
      selected={[String(issue.priority)]}
      onSelect={(value) => sync.update("Issue", issue.id, { priority: Number(value) })}
      triggerClassName={variant === "chip" ? chipTrigger : compactTrigger}
    >
      <PriorityIcon priority={issue.priority} size={variant === "chip" ? 12 : 15} />
      {variant === "chip" && (
        <ChipText empty={issue.priority === 0}>
          {issue.priority === 0 ? "Priority" : PRIORITY_LABELS[issue.priority]}
        </ChipText>
      )}
    </Picker>
  );
}

export function AssigneePicker({
  issue,
  variant = "compact",
}: {
  issue: Issue;
  variant?: Variant;
}) {
  const { sync } = useWorkspace();
  const users = useUsers();
  const assignee = users.find((u) => u.id === issue.assignee_id);
  return (
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
      selected={[issue.assignee_id ?? ""]}
      onSelect={(value) => sync.update("Issue", issue.id, { assignee_id: value || null })}
      triggerClassName={variant === "chip" ? chipTrigger : compactTrigger}
    >
      <span className="relative flex">
        <Avatar user={assignee} size={variant === "chip" ? "size-4" : "size-5"} />
        <AvatarWorkDot issue={issue} assignee={assignee} />
      </span>
      {variant === "chip" && (
        <ChipText empty={!assignee}>{assignee?.name ?? "Unassigned"}</ChipText>
      )}
    </Picker>
  );
}

export function LabelsPicker({ issue, variant = "compact" }: { issue: Issue; variant?: Variant }) {
  const { sync } = useWorkspace();
  const labels = useLabels().filter((l) => !l.team_id || l.team_id === issue.team_id);
  const chosen = labels.filter((l) => issue.label_ids.includes(l.id));
  const toggle = (id: string) => {
    const label_ids = issue.label_ids.includes(id)
      ? issue.label_ids.filter((l) => l !== id)
      : [...issue.label_ids, id];
    sync.update("Issue", issue.id, { label_ids });
  };
  return (
    <Picker
      label="Labels"
      multiple
      options={labels.map((l) => ({
        value: l.id,
        label: l.name,
        icon: <LabelDot color={l.color} />,
      }))}
      selected={issue.label_ids}
      onSelect={toggle}
      triggerClassName={variant === "chip" ? `${chipTrigger} gap-3` : "px-1 h-7"}
    >
      {variant === "chip" ? (
        chosen.length === 0 ? (
          <ChipText empty>+ Label</ChipText>
        ) : (
          chosen.map((l) => (
            <span key={l.id} className="flex items-center gap-1.5">
              <LabelDot color={l.color} />
              {l.name}
            </span>
          ))
        )
      ) : (
        chosen.map((l) => <LabelChip key={l.id} name={l.name} color={l.color} />)
      )}
    </Picker>
  );
}

export function LabelChip({ name, color }: { name: string; color: string }) {
  return (
    <span className="ui-chip flex h-6 items-center gap-1.5 px-2">
      <LabelDot color={color} />
      {name}
    </span>
  );
}

/**
 * A list row's project or service: its name, muted. Empty, nothing shows
 * until the row is hovered, then an icon says where to set it.
 */
function CompactValue({ icon, name }: { icon: React.ReactNode; name: string | undefined }) {
  if (!name)
    return (
      <span className="flex opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
        {icon}
      </span>
    );
  return (
    <>
      {icon}
      <span className="max-w-32 truncate text-xs text-muted">{name}</span>
    </>
  );
}

export function ProjectPicker({ issue, variant = "compact" }: { issue: Issue; variant?: Variant }) {
  const { sync } = useWorkspace();
  const projects = useRecords("Project");
  const project = useMemo(() => byId(projects), [projects]).get(issue.project_id ?? "");
  return (
    <Picker
      label="Project"
      options={[
        { value: "", label: "No project" },
        ...projects.map((p) => ({ value: p.id, label: p.name, icon: <ProjectIcon /> })),
      ]}
      selected={[issue.project_id ?? ""]}
      onSelect={(value) => sync.update("Issue", issue.id, { project_id: value || null })}
      triggerClassName={variant === "chip" ? chipTrigger : "h-7 px-1.5"}
    >
      {variant === "chip" ? (
        <>
          <ProjectIcon size={12} />
          <ChipText empty={!project}>{project?.name ?? "Project"}</ChipText>
        </>
      ) : (
        <CompactValue icon={<ProjectIcon />} name={project?.name} />
      )}
    </Picker>
  );
}

export function ServicePicker({ issue, variant = "chip" }: { issue: Issue; variant?: Variant }) {
  const { sync } = useWorkspace();
  const services = useRecords("Service");
  const service = services.find((s) => s.id === issue.service_id);
  return (
    <Picker
      label="Service"
      options={[
        { value: "", label: "No service" },
        ...services.map((s) => ({ value: s.id, label: s.name, icon: <ServiceIcon service={s} /> })),
      ]}
      selected={[issue.service_id ?? ""]}
      onSelect={(value) => sync.update("Issue", issue.id, { service_id: value || null })}
      triggerClassName={variant === "chip" ? chipTrigger : "h-7 px-1.5"}
    >
      {variant === "chip" ? (
        <>
          <ServiceIcon service={service} className="size-3" />
          <ChipText empty={!service}>{service?.name ?? "Service"}</ChipText>
        </>
      ) : (
        <CompactValue icon={<ServiceIcon service={service} />} name={service?.name} />
      )}
    </Picker>
  );
}
