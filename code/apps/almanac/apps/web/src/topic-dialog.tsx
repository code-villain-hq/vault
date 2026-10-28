// Changing what Almanac picked for a topic: its name, what it sticks to,
// its group, its icon and its page's shape. The same form changes a topic
// Almanac proposes, before it starts.

import type { Topic } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, messageOf } from "@code-villain/web";
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
  shape: string;
  title: string;
}

export function TopicDialog({ topic, onClose }: { topic: Topic; onClose: () => void }) {
  const { sync } = useWorkspace();
  // The workspace's own topic keeps its icon and its place above Calendar.
  const own = isOwn(topic);
  return (
    <TopicForm
      heading={`Edit ${topic.name}`}
      action="Save"
      fields={{ ...topic, title: topic.title ?? "" }}
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
  const [shape, setShape] = useState(fields.shape);
  const [title, setTitle] = useState(fields.title);
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
        shape,
        title: title.trim(),
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
              <div className="flex gap-3">
                <label className="flex min-w-0 flex-1 flex-col gap-1.5">
                  <span className="text-xs font-medium text-muted">Group</span>
                  <input
                    value={group}
                    onChange={(e) => setGroup(e.target.value)}
                    placeholder="Home"
                    className={`${field} h-9`}
                  />
                </label>
                <div className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-muted">Page</span>
                  <fieldset className="ui-segmented" aria-label="Page">
                    {(
                      [
                        ["cards", "Cards"],
                        ["document", "Document"],
                        ["routine", "Routine"],
                      ] as const
                    ).map(([value, label]) => (
                      <button
                        key={value}
                        type="button"
                        className="ui-segment"
                        aria-pressed={shape === value}
                        onClick={() => setShape(value)}
                      >
                        {label}
                      </button>
                    ))}
                  </fieldset>
                </div>
              </div>
              {shape === "document" && (
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-muted">The document's title</span>
                  <input
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                    className={`${field} h-9`}
                  />
                </label>
              )}
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
