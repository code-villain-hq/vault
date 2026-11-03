// Every sidebar item's menu, on a right click or ⇧F10 on the focused item:
// Mark all as read, greyed where nothing is unread, Rename… and Delete
// folder… on your own folders, Empty Trash… and Empty Junk…. What can't be
// undone asks first; marking read can be, with Undo where its count was.
import { ContextMenu } from "@base-ui/react/context-menu";
import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import type { Folder, MarkedRead } from "@postrider/types";
import { useNavigate, useParams } from "@tanstack/react-router";
import { type KeyboardEvent, type ReactNode, useRef, useState, useSyncExternalStore } from "react";
import { api, messageOf } from "./api";
import { askRename, markAllSeen } from "./folders";
import { useFolders } from "./mail";
import { useWorkspace } from "./workspace";

/** How long Undo stays where the count was. */
const UNDO_MS = 30_000;

type Marking = {
  /** The folder being marked read, until its servers answer. */
  marking?: string;
  /** The folder marked read last, with the id Undo takes. */
  marked?: { key: string; id: string };
  undoing?: boolean;
};

let marking: Marking = {};
let forget: ReturnType<typeof setTimeout> | undefined;
const watchers = new Set<() => void>();

function set(next: Marking) {
  marking = next;
  clearTimeout(forget);
  if (next.marked) forget = setTimeout(() => set({}), UNDO_MS);
  for (const watcher of watchers) watcher();
}

function watch(watcher: () => void) {
  watchers.add(watcher);
  return () => void watchers.delete(watcher);
}

function useMarking(): Marking {
  return useSyncExternalStore(watch, () => marking);
}

/**
 * Whether the sidebar's count for the folder `key` names gives way: while
 * it's marked read, and to Undo after.
 */
export function useCountHidden(key: string): boolean {
  const { marking: busy, marked } = useMarking();
  return same(busy, key) || same(marked?.key, key);
}

function same(a: string | undefined, b: string) {
  return a !== undefined && a.toLowerCase() === b.toLowerCase();
}

const item =
  "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none data-disabled:opacity-60";

/** A sidebar item with its menu around it. `own` is one of your own folders. */
export function FolderMenu({
  folder,
  own = false,
  children,
}: {
  folder: Folder;
  own?: boolean;
  children: ReactNode;
}) {
  const { workspace, place, refusals } = useWorkspace();
  const { moved } = useFolders();
  const navigate = useNavigate();
  const { folder: shown } = useParams({ strict: false }) as { folder?: string };
  const { marked, undoing } = useMarking();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const area = useRef<HTMLDivElement>(null);
  // An item that opens a dialog or a field keeps the focus there.
  const leaveFocus = useRef(false);
  const base = `/api/w/${workspace.id}/folders/${encodeURIComponent(folder.key)}`;

  const refused = (problem: unknown) =>
    refusals.dispatchEvent(new CustomEvent("refused", { detail: messageOf(problem) }));

  const markAllRead = () => {
    set({ marking: folder.key });
    api<MarkedRead>("POST", `${base}/read`, undefined, place).then(
      (done) => {
        markAllSeen(folder.key);
        moved();
        set(done.count > 0 ? { marked: { key: folder.key, id: done.id } } : {});
      },
      (problem: unknown) => {
        set({});
        refused(problem);
      },
    );
  };

  const undo = (id: string) => {
    set({ ...marking, undoing: true });
    api("POST", `/api/w/${workspace.id}/marked/${id}/undo`, undefined, place).then(
      () => {
        set({});
        moved();
      },
      (problem: unknown) => {
        set({ ...marking, undoing: false });
        refused(problem);
      },
    );
  };

  const leave = () => {
    if (shown?.toLowerCase() === folder.key.toLowerCase()) {
      void navigate({ to: "/$workspace", params: { workspace: workspace.url_key }, replace: true });
    }
  };

  const deleteFolder = () =>
    setConfirmation({
      title: `Delete ${folder.name}?`,
      body: `${folder.name} goes from every account, and so do the rules that sort mail into it.${
        folder.total > 0 ? ` ${goes(folder.total, "Its", "")} to the Trash.` : ""
      } This can't be undone.`,
      action: "Delete folder",
      onConfirm: () =>
        void api("DELETE", base, undefined, place).then(() => {
          // Its mail is in the Trash now: there's nothing left to undo.
          if (same(marking.marked?.key, folder.key)) set({});
          leave();
          moved();
        }, refused),
    });

  const empty = () => {
    const name = folder.role === "trash" ? "Trash" : "Junk";
    setConfirmation({
      title: folder.role === "trash" ? "Empty the Trash?" : "Empty Junk?",
      body: `${goes(folder.total, "The", " in it")} for good, in every account. This can't be undone.`,
      action: `Empty ${name}`,
      onConfirm: () =>
        void api("POST", `${base}/empty`, undefined, place).then(() => {
          if (same(marking.marked?.key, folder.key)) set({});
          moved();
        }, refused),
    });
  };

  const rename = () => {
    askRename(folder.key);
    void navigate({
      to: "/$workspace/folders/$folder",
      params: { workspace: workspace.url_key, folder: folder.key },
    });
  };

  const items: { label: string; run: () => void; asks?: boolean; disabled?: boolean }[] = [
    { label: "Mark all as read", run: markAllRead, disabled: folder.unread === 0 },
  ];
  if (own) {
    items.push({ label: "Rename…", run: rename, asks: true });
    items.push({ label: "Delete folder…", run: deleteFolder, asks: true });
  }
  if ((folder.role === "trash" || folder.role === "junk") && folder.total > 0) {
    items.push({
      label: folder.role === "trash" ? "Empty Trash…" : "Empty Junk…",
      run: empty,
      asks: true,
    });
  }

  // ⇧F10, or the menu key, on the focused item: the menu, under it.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!((event.key === "F10" && event.shiftKey) || event.key === "ContextMenu")) return;
    const link = event.target as HTMLElement;
    const rect = link.getBoundingClientRect();
    event.preventDefault();
    link.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + 8,
        clientY: rect.bottom,
      }),
    );
  };

  const undoHere = marked && same(marked.key, folder.key);
  return (
    <div className="relative">
      <ContextMenu.Root>
        <ContextMenu.Trigger ref={area} onKeyDown={onKeyDown}>
          {children}
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Positioner className="z-50 outline-none">
            <ContextMenu.Popup
              finalFocus={(closeType) =>
                leaveFocus.current
                  ? false
                  : closeType === "keyboard"
                    ? (area.current?.querySelector<HTMLElement>("a") ?? false)
                    : false
              }
              className="ui-menu min-w-52 origin-(--transform-origin) p-1 outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
            >
              {items.map(({ label, run, asks, disabled }) => (
                <ContextMenu.Item
                  key={label}
                  className={item}
                  disabled={disabled}
                  onClick={() => {
                    leaveFocus.current = asks === true;
                    run();
                  }}
                >
                  {label}
                </ContextMenu.Item>
              ))}
            </ContextMenu.Popup>
          </ContextMenu.Positioner>
        </ContextMenu.Portal>
      </ContextMenu.Root>
      {undoHere && (
        <button
          type="button"
          className="ui-link absolute top-1/2 right-2 -translate-y-1/2 text-xs"
          disabled={undoing}
          onClick={() => undo(marked.id)}
        >
          {undoing ? "Undoing…" : "Undo"}
        </button>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        onClose={() => {
          leaveFocus.current = false;
          setConfirmation(null);
        }}
      />
    </div>
  );
}

/** "Its 37 mails go", "The mail in it goes": what a folder holds, for its dialog. */
function goes(count: number, the: string, where: string) {
  return count === 1
    ? `${the} mail${where} goes`
    : `${the} ${count.toLocaleString("en")} mails${where} go`;
}
