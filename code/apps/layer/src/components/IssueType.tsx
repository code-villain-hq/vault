import type { TicketType } from "../lib/types";

/**
 * Issue-type badges, driven entirely by what the Jira site reports.
 *
 * Nothing about types is hardcoded: names, icons and hierarchy all come from
 * `/rest/api/3/issuetype`, so a project with custom types renders the same way
 * it does in Jira. The only assumption is `hierarchyLevel`, which is
 * structural in Jira itself — 1 and above is epic-level, 0 is a standard
 * issue, -1 is a sub-task.
 */

export type TypeMap = Record<string, TicketType>;

export function typeMap(types: TicketType[]): TypeMap {
  return Object.fromEntries(types.map((t) => [t.name.trim().toLowerCase(), t]));
}

export function lookupType(types: TypeMap, name: string): TicketType | undefined {
  return types[name.trim().toLowerCase()];
}

export function isEpicType(types: TypeMap, name: string): boolean {
  return (lookupType(types, name)?.hierarchy_level ?? 0) >= 1;
}

/**
 * Hierarchy is the one thing that means the same in every Jira, so it drives
 * the structural accent. Type identity is carried by the icon.
 */
export function hierarchyClass(types: TypeMap, name: string): string {
  const level = lookupType(types, name)?.hierarchy_level ?? 0;
  if (level >= 1) return "lvl-epic";
  if (level <= -1) return "lvl-sub";
  return "";
}

/** Used only when Jira gave us no icon — never in place of one it did. */
function Fallback({ level, size }: { level: number; size: number }) {
  const color = level >= 1 ? "var(--fg)" : level <= -1 ? "var(--faint)" : "var(--muted)";
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" style={{ flex: "none", display: "block" }}>
      <rect width="16" height="16" rx="3.5" fill={color} opacity="0.85" />
      {level >= 1 ? (
        <path d="M9.4 2.6 4.8 9.1h2.6l-.6 4.5 4.6-6.7H8.8z" fill="var(--panel)" />
      ) : level <= -1 ? (
        <path
          d="M4.6 4v4.2a1.6 1.6 0 0 0 1.6 1.6h5"
          fill="none"
          stroke="var(--panel)"
          strokeWidth="1.75"
          strokeLinecap="round"
        />
      ) : (
        <circle cx="8" cy="8" r="2.6" fill="var(--panel)" />
      )}
    </svg>
  );
}

export function IssueTypeIcon({
  types, name, size = 16,
}: {
  types: TypeMap;
  name: string;
  size?: number;
}) {
  const type = lookupType(types, name);

  if (type?.icon) {
    return (
      <img
        src={type.icon}
        width={size}
        height={size}
        alt={type.name}
        title={type.name}
        style={{ flex: "none", display: "block", borderRadius: "var(--radius-sm)" }}
      />
    );
  }
  return <Fallback level={type?.hierarchy_level ?? 0} size={size} />;
}
