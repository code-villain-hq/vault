import { openUrl } from "@tauri-apps/plugin-opener";
import { useId, useState, type KeyboardEvent, type ReactNode } from "react";
import { useStore } from "zustand";
import { checkRunsView } from "../lib/pr-checks";
import type { ReviewPrReaders } from "../lib/review-pr-readers";
import type { ReviewPrDetails as Snapshot } from "../lib/types";
import { PrCheckList } from "./PrChecks";
import { PrComments } from "./PrComments";
import { ErrorMessage } from "./ui";
import "./pr-details.css";
import "./review-pr-details.css";

type Target = Pick<Snapshot, "repo" | "number" | "url" | "head_sha">;
type Checks = "passing" | "failing" | "pending" | "none";

function sameHead(queue: string, detail: string): boolean {
  const oid = /^[a-f0-9]{40}$/i;
  return oid.test(queue) && oid.test(detail) && queue.toLowerCase() === detail.toLowerCase();
}

/** GitHub's rollup, as the button that opens the runs behind it (REV-13). */
const CHECKS: Record<Checks, { mark: string; tone: string; label: string }> = {
  failing: { mark: "✕", tone: "danger", label: "Checks failing" },
  pending: { mark: "◷", tone: "warn", label: "Checks running" },
  passing: { mark: "✓", tone: "ok", label: "Checks pass" },
  none: { mark: "", tone: "", label: "No checks" },
};

/**
 * A card's standing line, and what it says opened below it (REV-12): the
 * checks and the comments are the buttons that show them, so nothing is
 * open until one is asked for, and neither moves when the other opens.
 */
export function ReviewPrDetails({
  pr,
  readers,
  checks,
  unresolved,
  children,
}: {
  pr: Target;
  readers: ReviewPrReaders;
  checks: Checks;
  /** Unresolved threads, where they are counted: the comments button says so. */
  unresolved?: string;
  /** What the card says of where it stands, before the two buttons. */
  children?: ReactNode;
}) {
  const key = readers.keyOf(pr);
  const entry = useStore(readers.store, (entries) => entries.get(key));
  const open = entry?.comments ? "comments" : entry?.checks ? "checks" : null;
  const read = entry?.read ?? { status: "idle" as const };
  const [linkError, setLinkError] = useState<string | null>(null);
  const onOpen = (link: string) => {
    setLinkError(null);
    void openUrl(link).catch(() => setLinkError(key));
  };
  const id = useId();
  const rollup = CHECKS[checks];
  // The surrounding card has its own click and keyboard actions. The
  // buttons and what they open are for reading, including selecting text.
  const keep = (event: KeyboardEvent) => {
    if (
      ["Enter", " ", "ContextMenu"].includes(event.key) ||
      (event.shiftKey && event.key === "F10")
    )
      event.stopPropagation();
  };
  const boundary = {
    onClick: (event: { stopPropagation: () => void }) => event.stopPropagation(),
    onKeyDown: keep,
    onContextMenu: (event: { stopPropagation: () => void }) => event.stopPropagation(),
  };

  return (
    <>
      <div className="review-state">
        {children}
        <span className="spacer" />
        <span className="review-toggles" {...boundary}>
          <button
            type="button"
            className="ui-button review-toggle"
            aria-expanded={open === "checks"}
            aria-controls={`${id}-panel`}
            title={open === "checks" ? "Hide check runs" : "Show check runs"}
            onClick={() => readers.toggle(pr, "checks")}
          >
            {rollup.mark && <span className={`mark ${rollup.tone}`}>{rollup.mark}</span>}
            {rollup.label}
          </button>
          <button
            type="button"
            className="ui-button review-toggle"
            aria-expanded={open === "comments"}
            aria-controls={`${id}-panel`}
            title={open === "comments" ? "Hide comments" : "Show comments"}
            onClick={() => readers.toggle(pr, "comments")}
          >
            {unresolved ? <span className="mark warn">{unresolved} unresolved</span> : "Comments"}
          </button>
        </span>
      </div>
      {open && (
        <div
          className="review-pr-details"
          id={`${id}-panel`}
          role="region"
          aria-label={open === "comments" ? "Comments" : "Check runs"}
          title=""
          {...boundary}
        >
          {open === "comments" && <PrComments read={read} onOpen={onOpen} />}
          {open === "checks" && read.status === "loading" && (
            <p className="muted" role="status">
              Reading check runs…
            </p>
          )}
          {open === "checks" && read.status === "failed" && (
            <ErrorMessage>{read.message}</ErrorMessage>
          )}
          {open === "checks" &&
            read.status === "ready" &&
            (sameHead(pr.head_sha, read.data.head_sha) ? (
              <PrCheckList view={checkRunsView(read.data)} onOpen={onOpen} fold />
            ) : (
              <p className="ui-tint-warn error-message" role="status">
                Can’t confirm check runs for the head shown in the queue. Refresh to check again.
              </p>
            ))}
          {linkError === key && <ErrorMessage>Couldn’t open the browser. Try again.</ErrorMessage>}
        </div>
      )}
    </>
  );
}
