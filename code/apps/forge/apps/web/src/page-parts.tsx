// Pieces every record page shares, so the same job looks the same everywhere
// (see the "Design principles" spec): a + that opens a New dialog, a ⋯ menu
// for rare actions, and properties as quiet chips under the title.
import { chip } from "@code-villain/web";
import type { User } from "@forge/types";
import { CalendarDays } from "lucide-react";
import { statusStyle } from "./contrast";
import { formatDate } from "./data";
import { Avatar } from "./icons";
import { Picker } from "./picker";

export { CreateDialog, chip, MoreMenu, NewButton } from "@code-villain/web";

/**
 * A status's dot in its status ink: `status-<name>-ink` for the shared
 * colours, and for a chosen hue the ink `statusStyle` checks for it.
 * Inside a pill, `currentColor` takes the pill's ink.
 */
export function Dot({ color }: { color: string }) {
  const style =
    color === "currentColor"
      ? { background: color }
      : { ...statusStyle(color), background: "var(--status-ink)" };
  return <span className="size-2 shrink-0 rounded-full" style={style} aria-hidden="true" />;
}

/** A status as a chip under the title: its dot and name. */
export function StatusChip({
  statuses,
  value,
  onChange,
}: {
  statuses: { value: string; label: string; color: string }[];
  value: string;
  onChange: (value: string) => void;
}) {
  const current = statuses.find((s) => s.value === value);
  return (
    <Picker
      label="Status"
      options={statuses.map((s) => ({
        value: s.value,
        label: s.label,
        icon: <Dot color={s.color} />,
      }))}
      selected={[value]}
      onSelect={onChange}
      triggerClassName="ui-status flex h-7 max-w-64 items-center gap-1.5 px-2 outline-none focus-visible:ring-2 focus-visible:ring-accent"
      triggerStyle={statusStyle(current?.color)}
    >
      {current && <Dot color="currentColor" />}
      {current?.label}
    </Picker>
  );
}

/** A person as a chip, such as a project's lead: their avatar and name. */
export function PersonChip({
  label,
  users,
  value,
  onChange,
}: {
  label: string;
  users: User[];
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  const current = users.find((u) => u.id === value);
  return (
    <Picker
      label={label}
      options={[
        {
          value: "",
          label: `No ${label.toLowerCase()}`,
          icon: <Avatar user={undefined} size="size-4" />,
        },
        ...users.map((u) => ({
          value: u.id,
          label: u.name,
          icon: <Avatar user={u} size="size-4" />,
        })),
      ]}
      selected={[value ?? ""]}
      onSelect={(id) => onChange(id || null)}
      triggerClassName={chip}
    >
      <Avatar user={current} size="size-4" />
      {current?.name ?? label}
    </Picker>
  );
}

/** A date as a chip, such as a target date; clicking it opens the date picker. */
export function DateChip({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string | null;
  onChange: (date: string | null) => void;
}) {
  return (
    <label className={`${chip} relative`} title={label}>
      <CalendarDays size={12} strokeWidth={1.75} aria-hidden="true" />
      {value ? formatDate(value) : label}
      <input
        type="date"
        aria-label={label}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value || null)}
        onClick={(e) => e.currentTarget.showPicker?.()}
        className="absolute inset-0 cursor-pointer opacity-0"
      />
    </label>
  );
}
