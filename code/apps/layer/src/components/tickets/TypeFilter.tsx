import type { TicketType } from "../../lib/types";
import { IssueTypeIcon, type TypeMap } from "../IssueType";

/// Issue types as toggles. Nothing picked means every type — the same thing
/// as picking them all, and less work to say.
export function TypeFilter({
  issueTypes,
  types,
  kinds,
  onChange,
}: {
  issueTypes: TicketType[];
  types: TypeMap;
  kinds: string[];
  onChange: (kinds: string[]) => void;
}) {
  if (issueTypes.length === 0) return null;
  return (
    <div className="type-filter">
      {issueTypes
        .filter((t) => !t.subtask)
        .map((t) => {
          const on = kinds.includes(t.name);
          return (
            <button
              key={t.id}
              className={`ui-chip type-pick${on ? " active" : ""}`}
              onClick={() =>
                onChange(
                  on ? kinds.filter((n) => n !== t.name) : [...kinds, t.name],
                )
              }
            >
              <IssueTypeIcon types={types} name={t.name} size={14} />
              {t.name}
            </button>
          );
        })}
      {kinds.length > 0 && (
        <button className="ui-chip type-pick clear" onClick={() => onChange([])}>
          Clear
        </button>
      )}
    </div>
  );
}
