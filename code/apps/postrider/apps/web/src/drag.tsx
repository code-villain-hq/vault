// A row of a list dragged onto a folder in the sidebar moves its mail
// there, as V does: one mail, or from the Inbox all of a conversation's
// Inbox mail. The Inbox, Archive, Junk, Trash and your own folders take
// it, but not the folder it's in. Dropped on Flagged, it's flagged and
// stays where it is.

import type { Folder } from "@postrider/types";
import { type DragEvent, useState } from "react";
import type { From } from "./inbox";
import { useMarkMail } from "./marks";
import { type Going, useMoveMail } from "./move";

/** What a row carries while it's dragged. */
export type Carried = {
  going: Going[];
  /** The list it's dragged from. */
  from: From;
  /** Closes the mail open beside the list, when it's among them. */
  close?: () => void;
};

let carried: Carried | undefined;

/** A row taken up: it carries its mail, and looks like itself on the way. */
export function carry(event: DragEvent<HTMLElement>, what: Carried, label: string) {
  carried = what;
  const row = event.currentTarget;
  const rect = row.getBoundingClientRect();
  // Moved onto a folder, or flagged (a link to it) onto Flagged.
  event.dataTransfer.effectAllowed = "linkMove";
  // A link carries its address; the row carries what it's about.
  event.dataTransfer.clearData();
  event.dataTransfer.setData("text/plain", label);
  event.dataTransfer.setDragImage(row, event.clientX - rect.left, event.clientY - rect.top);
}

/** The row is put down, on a folder or not. */
export function putDown() {
  carried = undefined;
}

/** Whether a sidebar folder takes mail: what V offers, and Trash, as ⌫ does. */
export function takesMail(folder: Pick<Folder, "role">): boolean {
  return (
    folder.role === null ||
    folder.role === "archive" ||
    folder.role === "junk" ||
    folder.role === "trash"
  );
}

/** What's carried that isn't in the folder `key` already. */
function goingTo(key: string): Going[] {
  const to = key.toLowerCase();
  return carried?.going.filter(({ folder }) => (folder ?? "inbox").toLowerCase() !== to) ?? [];
}

/**
 * A sidebar folder, `key`, that a row can be dropped on: the handlers its
 * link wears, and `over` while a row is held above it.
 */
export function useDrop(key: string) {
  const moveMail = useMoveMail();
  const [over, setOver] = useState(false);
  const handlers = {
    onDragOver: (event: DragEvent<HTMLElement>) => {
      if (goingTo(key).length === 0) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      setOver(true);
    },
    onDragLeave: (event: DragEvent<HTMLElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(false);
    },
    onDrop: (event: DragEvent<HTMLElement>) => {
      setOver(false);
      const what = carried;
      const going = goingTo(key);
      if (!what || going.length === 0) return;
      event.preventDefault();
      carried = undefined;
      moveMail(going, { to: key }, what.from);
      what.close?.();
    },
  };
  return { over, handlers };
}

/** Flagged in the sidebar: a row dropped on it is flagged, and stays in its list. */
export function useFlagDrop() {
  const markMail = useMarkMail();
  const [over, setOver] = useState(false);
  // The mail the row opens, as S flags it: a conversation's newest in the
  // Inbox. A row flagged already, any of it, isn't flagged again.
  const target = () => {
    const going = carried?.going ?? [];
    return going.some(({ mail }) => mail.flagged) ? undefined : going.at(-1)?.mail;
  };
  const handlers = {
    onDragOver: (event: DragEvent<HTMLElement>) => {
      if (!target()) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "link";
      setOver(true);
    },
    onDragLeave: (event: DragEvent<HTMLElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(false);
    },
    onDrop: (event: DragEvent<HTMLElement>) => {
      setOver(false);
      const mail = target();
      if (!mail) return;
      event.preventDefault();
      carried = undefined;
      markMail([{ mail, marks: { flagged: true } }]);
    },
  };
  return { over, handlers };
}

/**
 * A file dropped on the window does nothing, rather than open in
 * Postrider's place; text fields still take dropped text.
 */
export function keepFilesOut() {
  const files = (event: globalThis.DragEvent) =>
    event.dataTransfer?.types.includes("Files") ?? false;
  // A place that takes files (a mail you write) says so first.
  window.addEventListener("dragover", (event) => {
    if (!files(event) || event.defaultPrevented) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "none";
  });
  window.addEventListener("drop", (event) => {
    if (files(event)) event.preventDefault();
  });
}
