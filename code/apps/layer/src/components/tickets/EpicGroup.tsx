import type { MouseEvent, ReactNode } from "react";
import type { Ticket } from "../../lib/types";
import { ChevronIcon } from "../icons";
import { IssueTypeIcon, type TypeMap } from "../IssueType";

/// One epic on your own list: its header and, unless closed, its tickets.
export function EpicGroup({
  epic,
  typeName,
  types,
  closed,
  taskId,
  tracker,
  link,
  onToggle,
  onContextMenu,
  onWork,
  onFile,
  onOpen,
  children,
}: {
  epic: { key: string; summary: string; issue?: Ticket; issues: Ticket[]; live: number };
  typeName: string;
  types: TypeMap;
  closed: boolean;
  /** The task already running for the epic itself, when it is yours. */
  taskId: string | null;
  /** The tracker's name, and the epic's link in it. */
  tracker: string;
  link: string | null;
  onToggle: () => void;
  onContextMenu: (e: MouseEvent) => void;
  onWork: () => void;
  onFile: () => void;
  onOpen: () => void;
  children: ReactNode;
}) {
  return (
    <div className="epic">
      <div className="epic-head" onClick={onToggle} onContextMenu={onContextMenu}>
        <span className={`chev${closed ? "" : " open"}`}><ChevronIcon /></span>
        {epic.key && <IssueTypeIcon types={types} name={typeName} size={18} />}
        {epic.key ? <span className="ui-chip key-chip">{epic.key}</span> : null}
        <span className="title">{epic.summary || (epic.key ? epic.key : "No epic")}</span>
        <span className="count">{epic.issues.length}</span>
        {epic.live > 0 && (
          <span className="ui-status ui-status-ok" title="Tickets in progress, or with a task open here">
            {epic.live} in flight
          </span>
        )}
        <div className="spacer" />
        {/*
          Working on the epic itself is only possible when it is one of
          your own issues: otherwise all the app has is the key and title
          its children carry, which is not enough to start from.
        */}
        {epic.issue && (
          <button
            className="ui-button"
            onClick={(e) => {
              e.stopPropagation();
              onWork();
            }}
          >
            {taskId ? "Open task" : "Start work"}
          </button>
        )}
        {/*
          The tracker itself is always reachable: the key is enough for a link,
          so every epic gets the same way out rather than some of them
          appearing actionable and the rest not.
        */}
        {epic.key && (
          <button
            className="ui-button"
            title={`File a new ticket under ${epic.key}`}
            onClick={(e) => {
              e.stopPropagation();
              onFile();
            }}
          >
            + ticket
          </button>
        )}
        {epic.key && link && (
          <button
            className="ui-link"
            title={`Open ${epic.key} in ${tracker}`}
            onClick={(e) => {
              e.stopPropagation();
              onOpen();
            }}
          >
            {tracker} ↗
          </button>
        )}
      </div>

      {!closed && <div className="epic-body">{children}</div>}
    </div>
  );
}
