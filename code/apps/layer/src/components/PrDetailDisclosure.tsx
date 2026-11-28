import { useId, type ReactNode } from "react";
import { ChevronIcon } from "./icons";
import "./pr-details.css";

export function PrDetailDisclosure({
  label,
  open,
  onToggle,
  children,
}: {
  label: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const id = useId();
  // The surrounding card can have its own click and keyboard actions. The
  // entire detail is for reading, including clicks that select text.
  return (
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- an event boundary, not a new control
    <div
      className="pr-detail"
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (
          ["Enter", " ", "ContextMenu"].includes(event.key) ||
          (event.shiftKey && event.key === "F10")
        )
          event.stopPropagation();
      }}
      onContextMenu={(event) => event.stopPropagation()}
    >
      <button
        type="button"
        className="ui-button"
        aria-expanded={open}
        aria-controls={id}
        onClick={onToggle}
      >
        <span aria-hidden="true" className={`chev${open ? " open" : ""}`}>
          <ChevronIcon />
        </span>
        {label}
      </button>
      <div id={id} hidden={!open}>
        {open && children}
      </div>
    </div>
  );
}
