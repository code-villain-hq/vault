import type { StoreApi } from "zustand";
import type { State } from "../store";
import { api } from "./api";
import { keep, reuse } from "./same";
import { issueForTask, taskUsesTracker } from "./tracker";
import type { Settings, TaskTicketLinked, Ticket } from "./types";

type Set = StoreApi<State>["setState"];
type Get = StoreApi<State>["getState"];
type Links = State["linkedTickets"];

const sameUrl = (a: string | null | undefined, b: string | null | undefined) =>
  !!a && !!b && a.replace(/\/$/, "").toLowerCase() === b.replace(/\/$/, "").toLowerCase();

/** The ticket at this link's address, from tickets just read. */
function find(tickets: Ticket[], link: TaskTicketLinked): Ticket | undefined {
  return tickets.find((t) => t.key === link.key && sameUrl(t.url, link.url));
}

function reads(settings: Settings | null, link: TaskTicketLinked): boolean {
  return taskUsesTracker(settings, { issue_key: link.key, issue_url: link.url });
}

/** Each link's status, from tickets just read; the rest as they were. */
function withStatuses(links: Links, settings: Settings | null, tickets: Ticket[]): Links {
  const next = { ...links };
  for (const link of Object.values(links)) {
    const ticket = reads(settings, link) ? find(tickets, link) : undefined;
    if (ticket)
      next[link.task_id] = {
        ...link,
        status: ticket.status,
        status_category: ticket.status_category,
      };
  }
  return keep(links, next);
}

/**
 * Your ticket list (TKT-1), and the status of every ticket linked to a task
 * this run (TASK-21). A ticket linked from Work is often not yours, or done,
 * so it is not on your list: those are read by key, after the list.
 */
export function issueRefresher(set: Set, get: Get, epoch: () => number): State["refreshIssues"] {
  return async (opts) => {
    const settings = get().settings;
    if (settings?.tracker == null) return;
    const at = epoch();
    const current = () => at === epoch() && get().settings?.tracker != null;
    // A timer tick should not flash the spinner over a list already on screen.
    const spin = !opts?.quiet || !get().issuesLoaded;
    if (spin) set({ issuesLoading: true });
    try {
      const [page, issueTypes, priorities] = await Promise.all([
        api.ticketsMine(),
        // Types rarely change and are cached in the backend; a failure here
        // must not stop the issues themselves from showing.
        api.ticketTypes().catch(() => get().issueTypes),
        // Asked once per connection, as rarely changed. Without them a sort
        // by priority keeps the order it had.
        get().priorities.length > 0 ? get().priorities : api.ticketPriorities().catch(() => []),
      ]);
      // As for reviews: an answer from a site disconnected meanwhile is not kept.
      if (!current()) return;
      set((s) => ({
        issues: reuse(s.issues, page.issues, (i) => i.key),
        linkedTickets: withStatuses(s.linkedTickets, settings, page.issues),
        issuesTruncated: page.more,
        issueTypes: keep(s.issueTypes, issueTypes),
        priorities: keep(s.priorities, priorities),
        issuesLoaded: true,
      }));
      const outside = Object.values(get().linkedTickets)
        .filter((link) => reads(settings, link) && !find(page.issues, link))
        .map((link) => link.key);
      // Not waited for: Start work waits for your list, not for these badges.
      // A failure leaves the last status known.
      if (outside.length > 0)
        void api
          .ticketsByKey([...new Set(outside)].slice(0, 100))
          .then((tickets) => {
            if (current())
              set((s) => ({ linkedTickets: withStatuses(s.linkedTickets, settings, tickets) }));
          })
          .catch(() => {});
    } catch (e) {
      if (at === epoch() && !opts?.quiet) get().fail(e);
    } finally {
      if (at === epoch() && spin) set({ issuesLoading: false });
    }
  };
}

/**
 * A ticket linked to a task, from the header or by an agent. The task list
 * is read first: an older poll may not know of the link yet, and the status
 * is kept only once the task has it.
 */
export function ticketLinker(set: Set, get: Get): State["taskTicketLinked"] {
  return async (link) => {
    await get().refreshTasks();
    set((s) => {
      const task = s.tasks.find((t) => t.id === link.task_id);
      if (task?.issue_key !== link.key || !sameUrl(task.issue_url, link.url)) return s;
      return { linkedTickets: { ...s.linkedTickets, [link.task_id]: link } };
    });
  };
}

/** The status the Work header shows for a task's ticket, from your list or a link this run. */
export function headerTicket(
  s: State,
  task: { id: string; issue_key: string | null; issue_url: string | null },
): Pick<Ticket, "status" | "status_category"> | undefined {
  if (!task.issue_key) return;
  const mine = issueForTask(s.issues, task);
  if (mine) return mine;
  const link = s.linkedTickets[task.id];
  return link &&
    link.key === task.issue_key &&
    sameUrl(link.url, task.issue_url) &&
    reads(s.settings, link)
    ? link
    : undefined;
}
