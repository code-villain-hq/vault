import { useStore } from "../../store";

/**
 * What the sidebar says under Tasks when it lists none: that a search found
 * nothing (TASK-19), and where unstarted tickets are; or, with no search, where
 * the first task comes from.
 */
export function NoTasks({ query, epics, repos }: { query: string; epics: number; repos: number }) {
  const setView = useStore((s) => s.setView);
  const link = (label: string, view: "repos" | "tickets") => (
    // oxlint-disable-next-line jsx-a11y/anchor-is-valid -- a link in the sentence; a view has no URL
    <a
      className="ui-link"
      href="#"
      onClick={(e) => {
        e.preventDefault();
        setView(view);
      }}
    >
      {label}
    </a>
  );
  return (
    <div className="muted" style={{ padding: "4px 8px 10px", lineHeight: 1.5 }}>
      {query ? (
        epics > 0 ? (
          "No other task matches."
        ) : (
          <>
            Nothing here matches “{query}”. A ticket not started yet is in{" "}
            {link("Tickets → Find work", "tickets")}.
          </>
        )
      ) : repos === 0 ? (
        <>No repositories yet — {link("add some in the Repos tab", "repos")}.</>
      ) : (
        <>No tasks yet. A task starts from a ticket — {link("pick one in Tickets", "tickets")}.</>
      )}
    </div>
  );
}
