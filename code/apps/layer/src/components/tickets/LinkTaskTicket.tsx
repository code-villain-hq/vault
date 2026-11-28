import { useLayoutEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { ticketConnectionScope, useTrackerName } from "../../lib/tracker";
import type { TicketPage } from "../../lib/types";
import { useStore } from "../../store";
import { BusyLabel, ErrorMessage, Modal, useFailure } from "../ui";

/**
 * Link ticket… in a Work task's header (TASK-21): an existing ticket for a
 * task started without one. Start work would make a second task.
 */
export function LinkTaskTicket({ taskId, scope }: { taskId: string; scope: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button className="ui-link" onClick={() => setOpen(true)}>
        Link ticket…
      </button>
      {open && <LinkTicketDialog taskId={taskId} scope={scope} onClose={() => setOpen(false)} />}
    </>
  );
}

function LinkTicketDialog({
  taskId,
  scope,
  onClose,
}: {
  taskId: string;
  scope: string;
  onClose: () => void;
}) {
  const tracker = useTrackerName();
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<{
    revision: number;
    page: TicketPage;
    trackerScope: string;
  } | null>(null);
  const [picked, setPicked] = useState("");
  const [searching, setSearching] = useState(false);
  const [linking, setLinking] = useState(false);
  const revision = useRef(0);
  const writing = useRef(false);
  const failure = useFailure();

  // Closing the dialog also drops an answer still on its way.
  useLayoutEffect(
    () => () => {
      revision.current += 1;
    },
    [],
  );

  // An answer counts only for the search still shown, the task still
  // selected, and the tracker it was asked of.
  function current(at: number) {
    const s = useStore.getState();
    return (
      revision.current === at &&
      s.view === "work" &&
      s.selectedTask === taskId &&
      ticketConnectionScope(s.settings) === scope
    );
  }

  function invalidate() {
    revision.current += 1;
    setResult(null);
    setPicked("");
    setSearching(false);
    failure.clear();
    return revision.current;
  }

  function cancel() {
    if (writing.current) return;
    invalidate();
    onClose();
  }

  async function find() {
    const text = query.trim();
    if (!text || writing.current || !current(revision.current)) return;
    const at = invalidate();
    setSearching(true);
    try {
      const page = await api.ticketsFind(text, "anyone", true, [], "", true);
      if (!current(at)) return;
      if (page.tracker_scope) setResult({ revision: at, page, trackerScope: page.tracker_scope });
      else failure.fail("Could not verify the tracker connection. Search for the ticket again.");
    } catch (e) {
      if (current(at)) failure.fail(e);
    } finally {
      if (current(at)) setSearching(false);
    }
  }

  async function link() {
    if (writing.current || !result || !current(result.revision)) return;
    if (!result.page.issues.some((ticket) => ticket.key === picked)) return;
    writing.current = true;
    setLinking(true);
    failure.clear();
    try {
      // The backend reports the saved link as an event, with any warning
      // about the agents' context. Only a refusal is a failed link.
      await api.linkTaskTicket(taskId, picked, result.trackerScope);
      onClose();
    } catch (e) {
      if (current(result.revision)) failure.fail(e);
    } finally {
      writing.current = false;
      setLinking(false);
    }
  }

  return (
    <Modal
      title="Link ticket"
      onClose={cancel}
      toolbar={
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            void find();
          }}
        >
          <input
            aria-label="Search tickets"
            className="ui-field"
            type="search"
            autoFocus
            placeholder={`Search ${tracker} by key or title…`}
            value={query}
            disabled={linking}
            onChange={(e) => {
              invalidate();
              setQuery(e.target.value);
            }}
          />
          <button
            className="ui-button"
            type="submit"
            disabled={linking || searching || !query.trim()}
          >
            <BusyLabel busy={searching} idle="Search" working="Searching…" />
          </button>
        </form>
      }
      footer={
        <>
          <button className="ui-button" disabled={linking} onClick={cancel}>
            Cancel
          </button>
          <button
            className="ui-button ui-button-primary"
            disabled={linking || !picked}
            onClick={() => void link()}
          >
            <BusyLabel busy={linking} idle="Link ticket" working="Linking…" />
          </button>
        </>
      }
    >
      <ErrorMessage>{failure.error}</ErrorMessage>
      {!result && !failure.error && (
        <p className="muted">
          Find the ticket this task is for. The task keeps its branch, checkouts and spec.
        </p>
      )}
      {result?.page.issues.length === 0 && <p className="muted">No tickets match this search.</p>}
      {result?.page.issues.map((ticket) => (
        <label
          key={ticket.key}
          className={`repo-pick ui-option${picked === ticket.key ? " is-active" : ""}`}
        >
          <input
            type="radio"
            name="ticket"
            value={ticket.key}
            checked={picked === ticket.key}
            disabled={linking}
            onChange={() => setPicked(ticket.key)}
          />
          <span className="mono">{ticket.key}</span>
          <span>{ticket.summary}</span>
        </label>
      ))}
      {result?.page.more && <p className="muted">{tracker} had more matches. Narrow the search.</p>}
    </Modal>
  );
}
