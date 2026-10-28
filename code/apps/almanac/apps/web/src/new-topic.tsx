// Starting a topic: a name, and what it sticks to if you know. The server
// makes it at once, and Claude writes the rest while its page opens.

import type { StartedTopic, StartTopicRequest } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, messageOf, shortcutFree } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import { Button, ErrorMessage } from "./ui";
import { useIsGuest, useWorkspace } from "./workspace";

/** "C" anywhere in the workspace starts a topic. */
export function useNewTopicKey(open: (name?: string) => void) {
  const guest = useIsGuest();
  const latest = useRef(open);
  latest.current = open;
  useEffect(() => {
    if (guest) return;
    // "G then C" is Calendar's, so a "c" just after "g" isn't this one.
    let pendingG = Number.NEGATIVE_INFINITY;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.shiftKey || !shortcutFree(event)) return;
      const key = event.key.toLowerCase();
      const afterG = event.timeStamp - pendingG < 1500;
      pendingG = key === "g" ? event.timeStamp : Number.NEGATIVE_INFINITY;
      if (key !== "c" || afterG) return;
      event.preventDefault();
      latest.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [guest]);
}

/** Open while `name` is set: the name it starts with, maybe empty. */
export function NewTopicDialog({ name, onClose }: { name: string | null; onClose: () => void }) {
  return (
    <CreateDialog open={name !== null} onClose={onClose} width="32rem">
      <NewTopicForm first={name ?? ""} onDone={onClose} />
    </CreateDialog>
  );
}

function NewTopicForm({ first, onDone }: { first: string; onDone: () => void }) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const [name, setName] = useState(first);
  const [sticksTo, setSticksTo] = useState("");
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);

  const start = async () => {
    if (!name.trim() || starting) return;
    setStarting(true);
    setError("");
    try {
      const body: StartTopicRequest = { name: name.trim() };
      if (sticksTo.trim()) body.sticks_to = sticksTo.trim();
      const { id } = await api<StartedTopic>("POST", `/api/w/${workspace.id}/topics`, body);
      onDone();
      await navigate({
        to: "/$workspace/topics/$topic",
        params: { workspace: workspace.url_key, topic: id },
      });
    } catch (e) {
      setError(messageOf(e));
      setStarting(false);
    }
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void start();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          void start();
        }
      }}
    >
      <div className="px-4 pt-4 text-xs text-muted">
        <Dialog.Title className="font-normal">New topic</Dialog.Title>
      </div>
      <div className="flex flex-col gap-3 px-4 py-3">
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-muted">Name</span>
          <input
            // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Pflanzen"
            className="ui-field h-9 rounded-md px-3"
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-muted">What it sticks to</span>
          <textarea
            value={sticksTo}
            onChange={(e) => setSticksTo(e.target.value)}
            rows={3}
            placeholder="The plants at home: watering, light, repotting. Leave it empty and Almanac writes it."
            className="ui-field resize-none rounded-md px-3 py-2"
          />
        </label>
        <p className="text-xs text-faint">
          Almanac picks an icon and a group. You can change both later.
        </p>
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-2 px-4 pb-4">
        <Button variant="secondary" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={!name.trim() || starting} title="Start topic (⌘↩)">
          Start topic
        </Button>
      </div>
    </form>
  );
}
