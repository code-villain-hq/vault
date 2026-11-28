import { openUrl } from "@tauri-apps/plugin-opener";
import { useId, useState, type KeyboardEvent } from "react";
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

function sameHead(queue: string, detail: string): boolean {
  const oid = /^[a-f0-9]{40}$/i;
  return oid.test(queue) && oid.test(detail) && queue.toLowerCase() === detail.toLowerCase();
}

export function ReviewPrDetails({ pr, readers }: { pr: Target; readers: ReviewPrReaders }) {
  const key = readers.keyOf(pr);
  const entry = useStore(readers.store, (entries) => entries.get(key));
  const commentsOpen = entry?.comments ?? false;
  const checksOpen = entry?.checks ?? false;
  const read = entry?.read ?? { status: "idle" as const };
  const [linkError, setLinkError] = useState<string | null>(null);
  const onOpen = (link: string) => {
    setLinkError(null);
    void openUrl(link).catch(() => setLinkError(key));
  };

  const id = useId();
  const open = commentsOpen ? "comments" : checksOpen ? "checks" : null;
  const tabs = [
    ["comments", "Comments"],
    ["checks", "Check runs"],
  ] as const;
  // The surrounding card has its own click and keyboard actions. The whole
  // detail is for reading, including clicks that select text.
  const keep = (event: KeyboardEvent) => {
    if (
      ["Enter", " ", "ContextMenu"].includes(event.key) ||
      (event.shiftKey && event.key === "F10")
    )
      event.stopPropagation();
  };

  return (
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- an event boundary, not a new control
    <div
      className="review-pr-details"
      title=""
      onClick={(event) => event.stopPropagation()}
      onKeyDown={keep}
      onContextMenu={(event) => event.stopPropagation()}
    >
      {/* Tabs, not two disclosures: opening one never moves the other. */}
      <div className="ui-tabs" role="tablist" aria-label="Pull request details">
        {tabs.map(([section, label]) => (
          <button
            key={section}
            type="button"
            className="ui-tab"
            role="tab"
            id={`${id}-${section}`}
            aria-selected={open === section}
            aria-controls={`${id}-panel`}
            title={open === section ? `Hide ${label.toLowerCase()}` : undefined}
            onClick={() => readers.toggle(pr, section)}
          >
            {label}
          </button>
        ))}
      </div>
      {open && (
        <div
          className="review-pr-panel"
          role="tabpanel"
          id={`${id}-panel`}
          aria-labelledby={`${id}-${open}`}
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
              <PrCheckList view={checkRunsView(read.data)} onOpen={onOpen} />
            ) : (
              <p className="ui-tint-warn error-message" role="status">
                Can’t confirm check runs for the head shown in the queue. Refresh to check again.
              </p>
            ))}
          {linkError === key && <ErrorMessage>Couldn’t open the browser. Try again.</ErrorMessage>}
        </div>
      )}
    </div>
  );
}
