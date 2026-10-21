// Changing many issues at once: the bar shown while issues are selected in a
// list (X selects). Each change is one transaction, so it applies to all of
// them or none.

import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import type { Issue, Operation } from "@forge/types";
import { X } from "lucide-react";
import { useMemo, useState } from "react";
import { PRIORITY_LABELS, STATE_TYPE_ORDER, useLabels, useUsers } from "./data";
import { Avatar, LabelDot, PriorityIcon, StateIcon } from "./icons";
import { Picker } from "./picker";
import { useMayDeleteIssue, useRecords, useWorkspace } from "./workspace";

const trigger = "h-7 px-2 text-xs text-muted hover:text-fg";

export function BulkBar({ issues, onClear }: { issues: Issue[]; onClear: () => void }) {
  const { sync } = useWorkspace();
  // One transaction: all of them or none, so only when you may delete each.
  const mayDelete = issues.every(useMayDeleteIssue());
  const states = useRecords("WorkflowState");
  const users = useUsers();
  const labels = useLabels();
  const teams = useMemo(() => new Set(issues.map((i) => i.team_id)), [issues]);

  const update = (change: (issue: Issue) => Record<string, unknown> | null) => {
    const operations: Operation[] = issues.flatMap((issue) => {
      const data = change(issue);
      return data
        ? [{ action: "update", model: "Issue", id: issue.id, data } satisfies Operation]
        : [];
    });
    if (operations.length > 0) sync.transact(operations);
  };

  // Statuses by name, since each team has its own; in workflow order.
  const statusNames = useMemo(() => {
    const seen = new Map<string, (typeof states)[number]>();
    for (const s of states) if (teams.has(s.team_id) && !seen.has(s.name)) seen.set(s.name, s);
    return [...seen.values()].sort(
      (a, b) =>
        STATE_TYPE_ORDER.indexOf(a.type) - STATE_TYPE_ORDER.indexOf(b.type) ||
        a.position - b.position,
    );
  }, [states, teams]);
  const usable = labels.filter((l) => !l.team_id || teams.has(l.team_id));
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-6 z-30 flex justify-center px-4">
      <div className="pointer-events-auto flex items-center gap-1 rounded-lg bg-surface px-2 py-1.5">
        <span className="px-2 text-xs font-medium">{issues.length} selected</span>
        <Picker
          label="Status"
          options={statusNames.map((s) => ({
            value: s.name,
            label: s.name,
            icon: <StateIcon state={s} />,
          }))}
          selected={[]}
          onSelect={(name) =>
            update((issue) => {
              const state = states.find((s) => s.team_id === issue.team_id && s.name === name);
              return state && state.id !== issue.state_id ? { state_id: state.id } : null;
            })
          }
          triggerClassName={trigger}
        >
          Status
        </Picker>
        <Picker
          label="Priority"
          options={[1, 2, 3, 4, 0].map((p) => ({
            value: String(p),
            label: PRIORITY_LABELS[p] ?? "",
            icon: <PriorityIcon priority={p} />,
          }))}
          selected={[]}
          onSelect={(value) =>
            update((issue) =>
              issue.priority === Number(value) ? null : { priority: Number(value) },
            )
          }
          triggerClassName={trigger}
        >
          Priority
        </Picker>
        <Picker
          label="Assignee"
          options={[
            { value: "", label: "No assignee", icon: <Avatar user={undefined} size="size-4" /> },
            ...users.map((u) => ({
              value: u.id,
              label: u.name,
              icon: <Avatar user={u} size="size-4" />,
            })),
          ]}
          selected={[]}
          onSelect={(value) =>
            update((issue) =>
              (issue.assignee_id ?? "") === value ? null : { assignee_id: value || null },
            )
          }
          triggerClassName={trigger}
        >
          Assignee
        </Picker>
        <Picker
          label="Labels"
          options={usable.map((l) => ({
            value: l.id,
            label: l.name,
            icon: <LabelDot color={l.color} />,
          }))}
          selected={usable
            .filter((l) => issues.every((i) => i.label_ids.includes(l.id)))
            .map((l) => l.id)}
          multiple
          onSelect={(id) => {
            const label = usable.find((l) => l.id === id);
            // Toggles: added to all unless all already have it.
            const remove = issues.every((i) => i.label_ids.includes(id));
            update((issue) => {
              if (label?.team_id && label.team_id !== issue.team_id) return null;
              const has = issue.label_ids.includes(id);
              if (remove) return { label_ids: issue.label_ids.filter((l) => l !== id) };
              return has ? null : { label_ids: [...issue.label_ids, id] };
            });
          }}
          triggerClassName={trigger}
        >
          Labels
        </Picker>
        {mayDelete && (
          <button
            type="button"
            className={`${trigger} rounded-md hover:text-danger-ink`}
            onClick={() => {
              const count = issues.length;
              const noun = count === 1 ? "issue" : "issues";
              setConfirmation({
                title: `Delete ${count} ${noun}?`,
                body:
                  count === 1
                    ? "It's deleted for everyone, along with its comments. This can't be undone."
                    : "They're deleted for everyone, along with their comments. This can't be undone.",
                action: `Delete ${count} ${noun}`,
                onConfirm: () => {
                  sync.transact(
                    issues.map(
                      (i) => ({ action: "delete", model: "Issue", id: i.id }) satisfies Operation,
                    ),
                  );
                  onClear();
                },
              });
            }}
          >
            Delete
          </button>
        )}
        <button
          type="button"
          aria-label="Clear the selection"
          title="Clear the selection (Esc)"
          className="ui-button ui-icon-button text-faint hover:text-fg"
          onClick={onClear}
        >
          <X size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </div>
  );
}
