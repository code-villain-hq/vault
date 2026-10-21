import { Fragment, type ReactNode } from "react";
import type { ConnectionKind, WorkspaceView } from "../lib/types";
import { useStore } from "../store";

/** The workspace these settings are kept in (SET-7), once there is one. */
export function useWorkspace(): WorkspaceView | null {
  return useStore((s) => s.settings?.workspace) ?? null;
}

/** What the workspace's connection of `kind` says, where it has one (SET-8). */
export function useHeld(kind: ConnectionKind): Record<string, unknown> | null {
  return useStore((s) => s.settings?.workspace?.connections[kind]) ?? null;
}

/** A held setting as text, or none. */
export function heldText(held: Record<string, unknown> | null, key: string): string | null {
  const value = held?.[key];
  return typeof value === "string" ? value : null;
}

/**
 * A connection's settings that are the same for everyone in the workspace
 * (SET-7): an admin's fields, a member's text. A member whose workspace has
 * no connection of this kind keeps their own on this Mac (SET-10), so they
 * get the fields, and a line saying so.
 */
export function Theirs({ kind, what, rows, children }: {
  kind: ConnectionKind;
  /** The service, as in "Code Villain has no Jira connection". */
  what: string;
  /** Label, value, and whether it is code (a query). */
  rows: [string, string | null | undefined, boolean?][];
  children: ReactNode;
}) {
  const workspace = useWorkspace();
  const held = useHeld(kind);
  if (!workspace) return <>{children}</>;
  if (!workspace.admin && !held) {
    return (
      <section className="settings-block">
        <p className="settings-alone">
          {workspace.name} has no {what} connection, so yours stays on this Mac. An admin connects one for everyone.
        </p>
        {children}
      </section>
    );
  }
  return (
    <section className="settings-block">
      <h3 className="settings-block-title">
        {workspace.name}'s <span className="muted">· {workspace.admin ? "the same for everyone in it" : "admins change these"}</span>
      </h3>
      {workspace.admin ? children : (
        <dl className="settings-shared">
          {rows.map(([label, value, code]) => (
            <Fragment key={label}>
              <dt>{label}</dt>
              <dd className={!value ? "muted" : code ? "mono" : undefined} title={value ?? undefined}>{value || "none"}</dd>
            </Fragment>
          ))}
        </dl>
      )}
    </section>
  );
}

/** What stays on this Mac: the email and the tokens (SET-7). */
export function Yours({ children }: { children: ReactNode }) {
  const workspace = useWorkspace();
  if (!workspace) return <>{children}</>;
  return (
    <section className="settings-block">
      <h3 className="settings-block-title">Yours</h3>
      {children}
    </section>
  );
}
