// Changing what Almanac picked for a topic: its name, what it sticks to,
// its group, its icon and its page's shape.

import type { Topic } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog } from "@code-villain/web";
import { useState } from "react";
import { TOPIC_ICON_NAMES, topicIcon } from "./icons";
import { Button } from "./ui";
import { useWorkspace } from "./workspace";

export function TopicDialog({ topic, onClose }: { topic: Topic; onClose: () => void }) {
  const { sync } = useWorkspace();
  const [name, setName] = useState(topic.name);
  const [sticksTo, setSticksTo] = useState(topic.sticks_to);
  const [group, setGroup] = useState(topic.group_name);
  const [icon, setIcon] = useState(topic.icon);
  const [shape, setShape] = useState(topic.shape);
  const [title, setTitle] = useState(topic.title ?? "");

  const save = () => {
    if (!name.trim()) return;
    sync.update("Topic", topic.id, {
      name: name.trim(),
      sticks_to: sticksTo.trim(),
      group_name: group.trim(),
      icon,
      shape,
      title: title.trim(),
    });
    onClose();
  };

  const field = "ui-field rounded-md px-3";
  return (
    <CreateDialog open onClose={onClose} width="34rem">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            save();
          }
        }}
      >
        <div className="px-4 pt-4 text-xs text-muted">
          <Dialog.Title className="font-normal">Edit {topic.name}</Dialog.Title>
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
        </div>
        <div className="flex items-center justify-end gap-2 px-4 pb-4">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!name.trim()} title="Save (⌘↩)">
            Save
          </Button>
        </div>
      </form>
    </CreateDialog>
  );
}
