// "Move to…", from an open mail (V): a folder to pick, by typing part of its
// name, or a new one to make with the mail in it, as the chat makes them.
// And the move itself, which E, ⌫, V and a row dropped on a folder share.

import { Dialog } from "@base-ui/react/dialog";
import type { Conversation, MailSummary, Folder as MailFolder, MoveMail } from "@postrider/types";
import { useNavigate } from "@tanstack/react-router";
import { Archive, Folder, FolderPlus, Inbox, MailWarning } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { api, messageOf } from "./api";
import { forgetListed } from "./folders";
import { type From, mailKey } from "./inbox";
import { useFolders, useInbox } from "./mail";
import { useWorkspace } from "./workspace";

/** A mail on its way, and the key of the folder it's in: undefined for the Inbox. */
export type Going = { mail: MailSummary; folder: string | undefined };

/** A conversation's Inbox mail, which goes all together; what you sent of it stays in Sent. */
export function conversationGoing(conversation: Conversation | undefined): Going[] | undefined {
  return conversation?.mails
    .filter((mail) => mail.folder === null)
    .map((mail) => ({ mail, folder: undefined }));
}

/**
 * Moves mail to its account's Archive or Trash, or to the folder `to`
 * names (a key, or a new folder's name), out of the list `from` showed it
 * in. It leaves the Inbox and that list at once; a server that refuses
 * says so in the frame's status, and the mail it kept comes back to the
 * Inbox.
 */
export function useMoveMail() {
  const { workspace, place, refusals } = useWorkspace();
  const { leave, stay } = useInbox();
  const { refresh } = useFolders();
  return (going: Going[], to: "archive" | "trash" | MoveMail, from: From) => {
    const listed = from.folder !== undefined || from.q !== undefined;
    const picked = typeof to === "object";
    void Promise.allSettled(
      going.map(({ mail, folder }) => {
        const fromInbox = (mail.folder ?? "inbox").toLowerCase() === "inbox";
        if (fromInbox) leave(mail.account_id, mail.uid);
        if (listed) {
          forgetListed(from, mailKey({ folder, account_id: mail.account_id, uid: mail.uid }));
        }
        const query = folder === undefined ? "" : `?${new URLSearchParams({ folder })}`;
        return api(
          "POST",
          `/api/w/${workspace.id}/mails/${mail.account_id}/${mail.uid}/${picked ? "move" : to}${query}`,
          picked ? to : undefined,
          place,
        ).catch((problem: unknown) => {
          if (fromInbox) stay(mail.account_id, mail.uid);
          throw problem;
        });
      }),
    ).then((done) => {
      refresh();
      const refused = done.find((one) => one.status === "rejected");
      if (refused) {
        refusals.dispatchEvent(new CustomEvent("refused", { detail: messageOf(refused.reason) }));
      }
    });
  };
}

/** Opens the list `from` names: a search, a sender's mail, a folder or the Inbox. */
export function openList(
  navigate: ReturnType<typeof useNavigate>,
  workspace: string,
  from: From,
): void {
  const params = { workspace };
  if (from.q !== undefined) {
    void navigate({ to: "/$workspace/search", params, search: { q: from.q } });
  } else if (from.sender !== undefined) {
    const sender = from.sender;
    void navigate({ to: "/$workspace/newsletters/$sender", params: { ...params, sender } });
  } else if (from.folder !== undefined) {
    const folder = from.folder;
    void navigate({ to: "/$workspace/folders/$folder", params: { ...params, folder } });
  } else {
    void navigate({ to: "/$workspace", params });
  }
}

type Choice = { key: string; label: ReactNode; icon: ReactNode };

/** Mail goes back to the Inbox, out of the way, or into a folder. */
const ICONS: Record<string, typeof Folder> = {
  inbox: Inbox,
  archive: Archive,
  junk: MailWarning,
};

export function MoveDialog({
  open,
  onOpenChange,
  from,
  onMove,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The key of the folder the mail is in, "inbox" for the Inbox. */
  from: string;
  /** A folder's key, or the name of a new one. */
  onMove: (to: string) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop className="ui-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup className="ui-dialog fixed top-[14vh] left-1/2 z-40 w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
          <Dialog.Title className="sr-only">Move to a folder</Dialog.Title>
          {open && (
            <Choices
              from={from}
              onPick={(to) => {
                onOpenChange(false);
                onMove(to);
              }}
            />
          )}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Choices({ from, onPick }: { from: string; onPick: (to: string) => void }) {
  const { folders } = useFolders();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const typed = query.trim();
  const wanted = typed.toLowerCase();
  const all: Pick<MailFolder, "key" | "name" | "role">[] = [
    { key: "inbox", name: "Inbox", role: null },
    ...folders,
  ];
  const choices: Choice[] = all
    .filter(
      (f) =>
        (f.role === null || f.role in ICONS) &&
        f.key.toLowerCase() !== from.toLowerCase() &&
        f.name.toLowerCase().includes(wanted),
    )
    .map((f) => {
      const Icon = ICONS[f.role ?? f.key] ?? Folder;
      return {
        key: f.key,
        label: f.name,
        icon: <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="text-muted" />,
      };
    });
  // A name no folder has yet, Sent's and Trash's included, makes one.
  const known = all.some((f) => f.name.toLowerCase() === wanted || f.key.toLowerCase() === wanted);
  if (typed && !known && typed !== "/") {
    choices.push({
      key: typed,
      label: <>New folder “{typed}”</>,
      icon: <FolderPlus size={15} strokeWidth={1.75} aria-hidden="true" className="text-muted" />,
    });
  }

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  return (
    <div>
      <input
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => Math.min(i + 1, choices.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(i - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            const choice = choices[active];
            if (choice) onPick(choice.key);
          }
        }}
        placeholder="Move to a folder, or name a new one"
        aria-label="Folder"
        role="combobox"
        aria-expanded="true"
        aria-controls="move-list"
        aria-activedescendant={choices[active] ? `move-${active}` : undefined}
        spellCheck={false}
        className="h-14 w-full bg-transparent px-4 text-query outline-none placeholder:text-faint"
      />
      <div ref={listRef} id="move-list" role="listbox" className="max-h-80 overflow-y-auto p-1.5">
        {choices.length === 0 && (
          <p className="px-3 py-6 text-center text-muted">Type a name for a new folder.</p>
        )}
        {choices.map((choice, index) => (
          <div
            key={choice.key}
            id={`move-${index}`}
            role="option"
            aria-selected={index === active}
            tabIndex={-1}
            data-index={index}
            onMouseMove={() => setActive(index)}
            onClick={() => onPick(choice.key)}
            onKeyDown={() => {}}
            className="ui-option flex h-9 cursor-default items-center gap-2.5 px-2.5"
          >
            {choice.icon}
            <span className="truncate">{choice.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
