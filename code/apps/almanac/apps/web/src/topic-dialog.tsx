// Changing what Almanac picked for a topic: its name, what it sticks to,
// its group and its icon. The same form changes a topic Almanac proposes,
// before it starts, and the template its blocks start from.

import type { Topic } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, messageOf } from "@code-villain/web";
import { BookOpen, CalendarDays, type LucideIcon, Sparkles, Sprout } from "lucide-react";
import { useState } from "react";
import { TOPIC_ICON_NAMES, topicIcon } from "./icons";
import { isOwn } from "./layout";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What the form changes. */
export interface TopicFields {
  name: string;
  sticks_to: string;
  group_name: string;
  icon: string;
  /** For a topic not started yet: the blocks it starts with. */
  template?: string;
}

/** What a topic may start as, with the blocks each gives it. */
const TEMPLATES: { value: string; label: string; blocks: string; Icon: LucideIcon }[] = [
  {
    value: "",
    label: "Almanac picks",
    blocks: "From the name and what it sticks to",
    Icon: Sparkles,
  },
  {
    value: "routine",
    label: "Routine",
    blocks: "Today · Week · You · Shelf · Advice",
    Icon: CalendarDays,
  },
  { value: "collection", label: "Collection", blocks: "Today · Week · Cards", Icon: Sprout },
  { value: "study", label: "Study", blocks: "Reading · Terms · What it says", Icon: BookOpen },
];

/** "Starts as": a template, or Almanac's pick when `picks`. */
export function StartsAs({
  value,
  picks = false,
  onChange,
}: {
  value: string;
  picks?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="mb-1.5 text-xs font-medium text-muted">Starts as</legend>
      <div className="grid grid-cols-2 gap-2">
        {TEMPLATES.filter((t) => picks || t.value).map(({ value: name, label, blocks, Icon }) => (
          <button
            key={name || "picks"}
            type="button"
            aria-pressed={value === name}
            onClick={() => onChange(name)}
            className="flex items-start gap-2.5 rounded-md px-3 py-2 text-left ring-1 ring-line ring-inset hover:bg-hover aria-pressed:bg-selected aria-pressed:ring-fg"
          >
            <Icon
              size={16}
              strokeWidth={1.75}
              aria-hidden="true"
              className="mt-0.5 shrink-0 text-muted"
            />
            <span className="flex min-w-0 flex-col">
              <span className="font-semibold">{label}</span>
              <span className="text-xs text-faint">{blocks}</span>
            </span>
          </button>
        ))}
      </div>
    </fieldset>
  );
}

export function TopicDialog({ topic, onClose }: { topic: Topic; onClose: () => void }) {
  const { sync } = useWorkspace();
  // The workspace's own topic keeps its icon and its place above Calendar.
  const own = isOwn(topic);
  return (
    <TopicForm
      heading={`Edit ${topic.name}`}
      action="Save"
      fields={topic}
      own={own}
      onSave={(fields) =>
        sync.update(
          "Topic",
          topic.id,
          own ? { name: fields.name, sticks_to: fields.sticks_to } : fields,
        )
      }
      onClose={onClose}
    />
  );
}

export function TopicForm({
  heading,
  action,
  fields,
  own = false,
  onSave,
  onClose,
}: {
  heading: string;
  action: string;
  fields: TopicFields;
  /** Only the name and what it sticks to. */
  own?: boolean;
  /** Saves the fields, trimmed; what it throws is shown. */
  onSave: (fields: TopicFields) => unknown;
  onClose: () => void;
}) {
  const [name, setName] = useState(fields.name);
  const [sticksTo, setSticksTo] = useState(fields.sticks_to);
  const [group, setGroup] = useState(fields.group_name);
  const [icon, setIcon] = useState(fields.icon);
  const [template, setTemplate] = useState(fields.template);
  const [problem, setProblem] = useState("");
  const [saving, setSaving] = useState(false);

  const save = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    setProblem("");
    try {
      await onSave({
        name: name.trim(),
        sticks_to: sticksTo.trim(),
        group_name: group.trim(),
        icon,
        ...(template === undefined ? {} : { template }),
      });
      onClose();
    } catch (e) {
      setProblem(messageOf(e));
      setSaving(false);
    }
  };

  const field = "ui-field rounded-md px-3";
  return (
    <CreateDialog open onClose={onClose} width="34rem">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            void save();
          }
        }}
      >
        <div className="px-4 pt-4 text-xs text-muted">
          <Dialog.Title className="font-normal">{heading}</Dialog.Title>
        </div>
        <div className="flex flex-col gap-3 px-4 py-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted">Name</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className={`${field} h-9`}
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-muted">What it sticks to</span>
            <textarea
              value={sticksTo}
              onChange={(e) => setSticksTo(e.target.value)}
              rows={3}
              className={`${field} resize-none py-2`}
            />
          </label>
          {!own && (
            <>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs font-medium text-muted">Group</span>
                <input
                  value={group}
                  onChange={(e) => setGroup(e.target.value)}
                  placeholder="Home"
                  className={`${field} h-9`}
                />
              </label>
              {template !== undefined && <StartsAs value={template} onChange={setTemplate} />}
              <fieldset className="flex flex-col gap-1.5">
                <legend className="mb-1.5 text-xs font-medium text-muted">Icon</legend>
                <div className="flex flex-wrap gap-1">
                  {TOPIC_ICON_NAMES.map((name) => {
                    const Icon = topicIcon(name);
                    return (
                      <button
                        key={name}
                        type="button"
                        aria-label={name}
                        title={name}
                        aria-pressed={icon === name}
                        onClick={() => setIcon(name)}
                        className="ui-button ui-icon-button aria-pressed:bg-selected"
                      >
                        <Icon size={15} strokeWidth={1.75} aria-hidden="true" />
                      </button>
                    );
                  })}
                </div>
              </fieldset>
            </>
          )}
          <ErrorMessage>{problem}</ErrorMessage>
        </div>
        <div className="flex items-center justify-end gap-2 px-4 pb-4">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!name.trim() || saving} title={`${action} (⌘↩)`}>
            {action}
          </Button>
        </div>
      </form>
    </CreateDialog>
  );
}
