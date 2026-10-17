// The filter menu and the bar of filters in use, over a list or board of
// issues (see issue-filter.ts for what they mean). "F" opens the menu.
import { X } from "lucide-react";
import { useEffect, useMemo } from "react";
import { PRIORITY_LABELS, useLabels, useUsers } from "./data";
import { Avatar, LabelDot, PriorityIcon, ProjectIcon } from "./icons";
import { FIELDS, type FilterField, type Filters, toggle } from "./issue-filter";
import { isTyping } from "./new-issue";
import { Picker, type PickerOption } from "./picker";
import { useRecords } from "./workspace";

/** What each field can be filtered by. */
function useOptions(): Record<FilterField, PickerOption[]> {
  const users = useUsers();
  const labels = useLabels();
  const projects = useRecords("Project");
  const services = useRecords("Service");
  return useMemo(
    () => ({
      assignee: [
        { value: "", label: "No assignee", icon: <Avatar user={undefined} size="size-4" /> },
        ...users.map((u) => ({
          value: u.id,
          label: u.name,
          icon: <Avatar user={u} size="size-4" />,
        })),
      ],
      priority: [1, 2, 3, 4, 0].map((p) => ({
        value: String(p),
        label: PRIORITY_LABELS[p] ?? "",
        icon: <PriorityIcon priority={p} />,
      })),
      label: labels.map((l) => ({
        value: l.id,
        label: l.name,
        icon: <LabelDot color={l.color} />,
      })),
      project: [
        { value: "", label: "No project" },
        ...[...projects]
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((p) => ({ value: p.id, label: p.name, icon: <ProjectIcon /> })),
      ],
      service: [
        { value: "", label: "No service" },
        ...[...services]
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((s) => ({ value: s.id, label: s.name })),
      ],
    }),
    [users, labels, projects, services],
  );
}

const FILTER_BUTTON = "forge-filter";

/** The "Filter" menu, for the page header: every value of every field. */
export function FilterButton({
  filters,
  onChange,
}: {
  filters: Filters;
  onChange: (filters: Filters) => void;
}) {
  const options = useOptions();
  const all = FIELDS.flatMap(({ field, name }) =>
    options[field].map((o) => ({
      ...o,
      value: `${field}:${o.value}`,
      label: `${name}: ${o.label}`,
    })),
  );
  const selected = FIELDS.flatMap(({ field }) =>
    (filters[field] ?? []).map((v) => `${field}:${v}`),
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "f" || event.metaKey || event.ctrlKey || event.altKey) return;
      if (
        isTyping(event.target) ||
        document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")
      )
        return;
      event.preventDefault();
      document.getElementById(FILTER_BUTTON)?.click();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <Picker
      label="Filter"
      options={all}
      selected={selected}
      multiple
      triggerId={FILTER_BUTTON}
      onSelect={(value) => {
        const [field, ...rest] = value.split(":");
        onChange(toggle(filters, field as FilterField, rest.join(":")));
      }}
      triggerClassName="h-7 px-2 text-xs text-muted hover:text-fg"
    >
      Filter <kbd className="text-faint">F</kbd>
    </Picker>
  );
}

/** The filters in use, under the page header; each can be changed or removed. */
export function FilterBar({
  filters,
  onChange,
}: {
  filters: Filters;
  onChange: (filters: Filters) => void;
}) {
  const options = useOptions();
  const active = FIELDS.filter(({ field }) => (filters[field]?.length ?? 0) > 0);
  if (active.length === 0) return null;
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 px-4 pb-2 text-xs">
      {active.map(({ field, name }) => {
        const chosen = options[field].filter((o) => filters[field]?.includes(o.value));
        return (
          <span key={field} className="flex items-center ui-chip">
            <Picker
              label={name}
              options={options[field]}
              selected={filters[field] ?? []}
              multiple
              onSelect={(value) => onChange(toggle(filters, field, value))}
              triggerClassName="h-7 pl-2 pr-1 hover:bg-transparent hover:text-fg"
            >
              <span className="text-muted">{name}</span>
              <span className="text-faint">{chosen.length > 1 ? "is any of" : "is"}</span>
              <span>{chosen.map((o) => o.label).join(", ")}</span>
            </Picker>
            <button
              type="button"
              aria-label={`Remove the ${name.toLowerCase()} filter`}
              onClick={() => onChange({ ...filters, [field]: [] })}
              className="flex h-7 items-center rounded-r-full px-1.5 text-faint outline-none hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
            >
              <X size={12} strokeWidth={1.75} aria-hidden="true" />
            </button>
          </span>
        );
      })}
      <button type="button" onClick={() => onChange({})} className="text-faint hover:text-fg">
        Clear
      </button>
    </div>
  );
}
