import type { ReactNode } from "react";
import { ChevronIcon } from "../icons";
import { statusClass } from "./status";

/// One status on your own list, when Mine is grouped by status (TKT-14).
export function StatusGroup({
  name,
  category,
  count,
  closed,
  onToggle,
  children,
}: {
  name: string;
  category: string;
  count: number;
  closed: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className="epic">
      <div className="epic-head" onClick={onToggle}>
        <span className={`chev${closed ? "" : " open"}`}><ChevronIcon /></span>
        <span className={`ui-status ${statusClass(category)}`}>{name}</span>
        <span className="count">{count}</span>
      </div>
      {!closed && <div className="epic-body">{children}</div>}
    </div>
  );
}
