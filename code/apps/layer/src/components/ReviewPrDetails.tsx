import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useStore } from "zustand";
import { checkRunsView } from "../lib/pr-checks";
import type { ReviewPrReaders } from "../lib/review-pr-readers";
import type { ReviewPrDetails as Snapshot } from "../lib/types";
import { PrCheckList } from "./PrChecks";
import { PrComments } from "./PrComments";
import { PrDetailDisclosure } from "./PrDetailDisclosure";
import { ErrorMessage } from "./ui";
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

  return (
    <div className="review-pr-details" title="">
      <PrDetailDisclosure
        label="Comments"
        open={commentsOpen}
        onToggle={() => readers.toggle(pr, "comments")}
      >
        <PrComments read={read} onOpen={onOpen} />
      </PrDetailDisclosure>
      <PrDetailDisclosure
        label="Check runs"
        open={checksOpen}
        onToggle={() => readers.toggle(pr, "checks")}
      >
        {read.status === "loading" && (
          <p className="muted" role="status">
            Reading check runs…
          </p>
        )}
        {read.status === "failed" && <ErrorMessage>{read.message}</ErrorMessage>}
        {read.status === "ready" &&
          (sameHead(pr.head_sha, read.data.head_sha) ? (
            <PrCheckList view={checkRunsView(read.data)} onOpen={onOpen} />
          ) : (
            <p className="ui-tint-warn error-message" role="status">
              Can’t confirm check runs for the head shown in the queue. Refresh to check again.
            </p>
          ))}
      </PrDetailDisclosure>
      {(commentsOpen || checksOpen) && linkError === key && (
        <ErrorMessage>Couldn’t open the browser. Try again.</ErrorMessage>
      )}
    </div>
  );
}
