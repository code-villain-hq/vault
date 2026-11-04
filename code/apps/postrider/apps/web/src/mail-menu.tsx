// A mail's menu in a list, on a right click or ⇧F10 on the focused row:
// Reply, Reply all and Forward open the mail and answer it; Mark as unread
// or read, Flag or Unflag, Archive, Move to… and Delete are done from the
// list, as the keys beside them do on an open mail. A conversation in the
// Inbox is marked and moved all together, as its row is. Mail that waits to
// be sent (the outbox) can't be marked or moved yet, and a draft isn't
// answered.
import { ContextMenu } from "@base-ui/react/context-menu";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useState } from "react";
import type { Answering } from "./compose";
import { item, popup } from "./folder-menu";
import { type From, mailKey, openedFrom, type Row, rememberOpened, unreadRow } from "./inbox";
import { flaggedRow, flagToggle, readToggle, useMarkMail } from "./marks";
import { conversationGoing, MoveDialog, useMoveMail } from "./move";
import { useWorkspace } from "./workspace";

/** An answer asked for from a list's menu, which the mail page starts once it's open. */
let answerAsked: { account: string; uid: number; how: Answering } | undefined;

/** The answer asked for the mail `account`/`uid`, once: the mail page takes it. */
export function takeAnswerAsked(account: string, uid: number): Answering | undefined {
  const asked = answerAsked;
  if (asked?.account !== account || asked.uid !== uid) return undefined;
  answerAsked = undefined;
  return asked.how;
}

type Entry =
  | { label: string; keys: string; run: () => void; disabled?: boolean }
  | `line ${string}`;

/**
 * A list's row, an `<li>`, with its menu around it. `close` closes it,
 * open beside the list, once it's moved away.
 */
export function MailMenu({
  row,
  from,
  close,
  children,
}: {
  row: Row;
  from: From;
  close?: () => void;
  children: ReactNode;
}) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const markMail = useMarkMail();
  const moveMail = useMoveMail();
  const [moving, setMoving] = useState(false);
  const { mail, conversation } = row;
  const folder = mail.folder ?? from.folder;
  const kept = folder?.toLowerCase();
  const waits = (conversation?.mails ?? [mail]).some((m) => m.unsent);

  const answer = (how: Answering) => {
    answerAsked = { account: mail.account_id, uid: mail.uid, how };
    rememberOpened(mailKey(mail));
    void navigate({
      to: "/$workspace/mail/$account/$uid",
      params: { workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) },
      search: openedFrom(mail, from),
    });
  };

  const fileAway = (to: Parameters<typeof moveMail>[1]) => {
    moveMail(conversationGoing(conversation) ?? [{ mail, folder }], to, from);
    close?.();
  };

  const entries: Entry[] = [];
  if (kept !== "drafts") {
    entries.push(
      { label: "Reply", keys: "R", run: () => answer("reply") },
      { label: "Reply all", keys: "A", run: () => answer("all") },
      { label: "Forward", keys: "F", run: () => answer("forward") },
      "line answers",
    );
  }
  entries.push(
    {
      label: unreadRow(row) ? "Mark as read" : "Mark as unread",
      keys: "⇧U",
      run: () => markMail(readToggle(row)),
      disabled: waits,
    },
    {
      label: flaggedRow(row) ? "Unflag" : "Flag",
      keys: "S",
      run: () => markMail(flagToggle(row)),
      disabled: waits,
    },
    "line marks",
  );
  if (kept !== "archive") {
    entries.push({ label: "Archive", keys: "E", run: () => fileAway("archive"), disabled: waits });
  }
  entries.push({ label: "Move to…", keys: "V", run: () => setMoving(true), disabled: waits });
  if (kept !== "trash") {
    entries.push({ label: "Delete", keys: "⌫", run: () => fileAway("trash"), disabled: waits });
  }

  return (
    <>
      <ContextMenu.Root>
        <ContextMenu.Trigger render={<li />}>{children}</ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Positioner className="z-50 outline-none">
            <ContextMenu.Popup finalFocus={false} className={popup}>
              {entries.map((entry) =>
                typeof entry === "string" ? (
                  <ContextMenu.Separator key={entry} className="my-1 h-px bg-line" />
                ) : (
                  <ContextMenu.Item
                    key={entry.label}
                    className={item}
                    disabled={entry.disabled}
                    onClick={entry.run}
                  >
                    {entry.label}
                    <span className="ml-auto pl-6 text-xs text-faint">{entry.keys}</span>
                  </ContextMenu.Item>
                ),
              )}
            </ContextMenu.Popup>
          </ContextMenu.Positioner>
        </ContextMenu.Portal>
      </ContextMenu.Root>
      <MoveDialog
        open={moving}
        onOpenChange={setMoving}
        from={folder ?? "inbox"}
        onMove={(to) => fileAway({ to })}
      />
    </>
  );
}
