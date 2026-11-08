import { Fragment, useState, type ReactNode } from "react";
import type { ConnectionKind, WorkspaceView } from "../lib/types";
import { useStore } from "../store";
import { BusyLabel } from "./ui";

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

/** Label, value, and whether it is code (a query). */
type Row = [string, string | null | undefined, boolean?];

/** Settings as text: label, then value, "none" where there is none. */
function Rows({ rows }: { rows: Row[] }) {
  return (
    <dl className="settings-shared">
      {rows.map(([label, value, code]) => (
        <Fragment key={label}>
          <dt>{label}</dt>
          <dd className={!value ? "muted" : code ? "mono" : undefined} title={value ?? undefined}>
            {value || "none"}
          </dd>
        </Fragment>
      ))}
    </dl>
  );
}

/**
 * A connection's settings that are the same for everyone in the workspace
 * (SET-7): an admin's fields, a member's text. A member whose workspace has
 * no connection of this kind keeps their own on this Mac (SET-10), so they
 * get the fields, and a line saying so. Folded (SET-14), everyone gets the
 * text, and an admin `keep`s what is chosen day to day.
 */
export function Theirs({
  kind,
  what,
  rows,
  folded,
  keep,
  children,
}: {
  kind: ConnectionKind;
  /** The service, as in "Code Villain has no Jira connection". */
  what: string;
  rows: Row[];
  /** Connected, and not being changed: the settings as text. */
  folded?: boolean;
  /** An admin's controls that stay out when folded, saved as they change. */
  keep?: ReactNode;
  children: ReactNode;
}) {
  const workspace = useWorkspace();
  const held = useHeld(kind);
  const fields = folded ? (
    <>
      <Rows rows={rows} />
      {keep}
    </>
  ) : (
    children
  );
  if (!workspace) return <>{fields}</>;
  if (!workspace.admin && !held) {
    return (
      <section className="settings-block">
        <p className="settings-alone">
          {workspace.name} has no {what} connection, so yours stays on this Mac. An admin connects
          one for everyone.
        </p>
        {fields}
      </section>
    );
  }
  return (
    <section className="settings-block">
      <h3 className="settings-block-title">
        {workspace.name}'s{" "}
        <span className="muted">
          · {workspace.admin ? "the same for everyone in it" : "admins change these"}
        </span>
      </h3>
      {workspace.admin ? fields : <Rows rows={rows} />}
    </section>
  );
}

/** What stays on this Mac: the email and the tokens (SET-7); folded, as text (SET-14). */
export function Yours({
  rows,
  folded,
  children,
}: {
  rows?: Row[];
  folded?: boolean;
  children: ReactNode;
}) {
  const workspace = useWorkspace();
  const fields = folded && rows ? <Rows rows={rows} /> : children;
  if (!workspace) return <>{fields}</>;
  return (
    <section className="settings-block">
      <h3 className="settings-block-title">Yours</h3>
      {fields}
    </section>
  );
}

/** A token, folded: never shown, only where it is (SET-14). */
export const IN_KEYCHAIN = "in your keychain";

/**
 * Whether a connected page shows its form (SET-14). Connected, it folds;
 * Change… opens it, and connecting or Cancel folds it again.
 */
export function useFold(connected: boolean) {
  const [changing, setChanging] = useState(false);
  return {
    folded: connected && !changing,
    change: () => setChanging(true),
    fold: () => setChanging(false),
  };
}

/** Change…, and what it opens. */
export function ChangeRow({
  label = "Change…",
  note,
  onClick,
}: {
  label?: string;
  note: string;
  onClick: () => void;
}) {
  return (
    <div className="row settings-change">
      <button className="ui-button" onClick={onClick}>
        {label}
      </button>
      <span className="muted">{note}</span>
    </div>
  );
}

/**
 * Connect, or, with the form open on a connected page, Reconnect and Cancel
 * (SET-12, SET-14). One button named by what it does.
 */
export function ConnectRow({
  connected,
  busy,
  disabled,
  onConnect,
  onCancel,
}: {
  connected: boolean;
  busy: boolean;
  /** Something it needs is missing. */
  disabled: boolean;
  onConnect: () => void;
  /** Puts the form back as stored, and folds it. */
  onCancel: () => void;
}) {
  return (
    <div className="row">
      <button
        className="ui-button ui-button-primary"
        disabled={busy || disabled}
        onClick={onConnect}
      >
        <BusyLabel busy={busy} idle={connected ? "Reconnect" : "Connect"} working="Connecting…" />
      </button>
      {connected && (
        <button className="ui-button" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      )}
    </div>
  );
}

/** Whether the workspace's part is an admin's to change, not this person's (SET-7). */
export function useTheirsLocked(kind: ConnectionKind): boolean {
  const workspace = useWorkspace();
  const held = useHeld(kind);
  return !!workspace && !workspace.admin && held !== null;
}
