import type { CheckRunsView } from "../lib/pr-checks";
import { ExternalIcon } from "./icons";

const labels = {
  passing: "Checks passing",
  failing: "Checks failing",
  pending: "Checks pending",
  cancelled: "Checks cancelled",
  skipped: "Checks skipped or neutral",
  empty: "No checks reported",
  unknown: "Checks unknown",
  incomplete: "Checks incomplete",
  unavailable: "Checks unavailable",
};

const tones = {
  failing: " ui-status-danger",
  pending: " ui-status-warn",
  cancelled: " ui-status-warn",
  unknown: " ui-status-warn",
  incomplete: " ui-status-warn",
  unavailable: " ui-status-warn",
  passing: "",
  skipped: "",
  empty: "",
};

export function PrCheckStatus({
  view,
  refresh,
}: {
  view: CheckRunsView;
  refresh?: "loading" | "failed";
}) {
  if (refresh) {
    return (
      <span className={`ui-status${refresh === "failed" ? tones.unavailable : ""}`}>
        {refresh === "loading" ? "Refreshing checks…" : "Checks not refreshed"}
      </span>
    );
  }
  return (
    <span className={`ui-status${tones[view.summary]}`}>
      {labels[view.summary]}
      {view.state === "incomplete" && view.summary !== "incomplete" && " · incomplete"}
    </span>
  );
}

/** Check Runs only, not the Reviews queue's overall GitHub CI rollup. */
export function PrCheckList({
  view,
  onOpen,
}: {
  view: CheckRunsView;
  onOpen: (url: string) => void;
}) {
  return (
    <>
      {view.state === "unavailable" && (
        <p className="ui-tint-danger error-message" role="alert">
          Couldn’t read check runs. Try again.
        </p>
      )}
      {view.state === "incomplete" && (
        <p className="ui-tint-warn error-message" role="status">
          The check run list is incomplete. Try again.
        </p>
      )}
      {view.summary === "empty" && <p className="muted">No checks reported.</p>}
      {view.runs.length > 0 && (
        <ul aria-label="Check runs" style={{ padding: 0, margin: 0 }}>
          {view.runs.map(({ name, label, outcome, url }) => (
            <li key={name} className="check">
              <span className="name" style={{ overflowWrap: "anywhere" }}>
                {name}
              </span>
              <span className={`ui-status${tones[outcome]}`}>{label}</span>
              {url && (
                <button
                  type="button"
                  className="ui-button ui-icon-button"
                  title="Open on GitHub"
                  aria-label={`Open ${name} on GitHub`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onOpen(url);
                  }}
                >
                  <ExternalIcon />
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
