import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../lib/api";
import type { ProfileView } from "../../lib/types-profiles";
import { useSession } from "../../lib/workspace-sync";
import { useStore } from "../../store";
import { Confirm, ErrorMessage, Floating, Modal, useFailure } from "../ui";
import { ChevronIcon } from "../icons";
import { NewWorkspace } from "./NewWorkspace";

/**
 * The window's workspace, at the foot of the sidebar (§25), and the way to
 * the others, grouped by where they live: this Mac, then each server
 * (PROF-15). Each opens in a copy of the app of its own (PROF-2), or brings
 * that copy forward if it is open already (PROF-3).
 *
 * The list is read when the menu opens and when `profiles:changed` says so:
 * another window may have made, renamed or deleted one since.
 */
export function ProfileMenu() {
  // The list loads in the background, and opening one is a menu item that
  // closes its menu: neither has a place of its own, so both fail in the
  // top bar (WIN-2).
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const [profiles, setProfiles] = useState<ProfileView[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [making, setMaking] = useState(false);
  const [managing, setManaging] = useState(false);
  const [again, setAgain] = useState(false);
  const { signedOut } = useSession();
  const button = useRef<HTMLButtonElement>(null);

  const load = useCallback(() => {
    api.listProfiles().then(setProfiles).catch(fail);
  }, [fail]);

  useEffect(() => {
    load();
    const p = listen("profiles:changed", load);
    return () => void p.then((un) => un());
  }, [load]);

  const me = profiles.find((p) => p.current);
  // This Mac's first, then each server's, in the order first seen.
  const places = [null, ...new Set(profiles.flatMap((p) => (p.server ? [p.server] : [])))];

  function members() {
    setMenu(null);
    api.openMembers().catch(fail);
  }

  function toggle() {
    if (menu) return setMenu(null);
    load();
    const r = button.current?.getBoundingClientRect();
    if (r) setMenu({ x: r.left, y: r.top - 4 });
  }

  function open(p: ProfileView) {
    setMenu(null);
    if (p.current) return;
    api.openProfile(p.id)
      .then(() => { if (!p.open) toast("info", `Opening ${p.name} in a window of its own…`); })
      .catch(fail);
  }

  return (
    <div className="profile-foot">
      <button
        ref={button}
        className={`profile-button${menu ? " active" : ""}`}
        onClick={toggle}
        title="Workspaces: each is a whole Layer of its own, in a window of its own"
      >
        <Initial profile={me} />
        <span className="profile-name">{me?.name ?? "Workspace"}</span>
        {signedOut && <span className="profile-state">signed out</span>}
        <span className="profile-chev" aria-hidden><ChevronIcon /></span>
      </button>

      {menu && (
        <Floating
          x={menu.x}
          y={menu.y}
          above
          onClose={() => setMenu(null)}
          ignore={button}
          className="ui-menu ctx-menu profile-menu"
          measureKey={profiles.length}
        >
          {places.map((place) => (
            <div key={place ?? "this-mac"} className="profile-menu-group">
              <div className="profile-menu-head">{place ?? "On this Mac"}</div>
              {profiles.filter((p) => p.server === place).map((p) => (
                <button key={p.id} className="ctx-item ui-option profile-item" onClick={() => open(p)}>
                  <Initial profile={p} />
                  <span className="profile-item-name">{p.name}</span>
                  <span className="profile-state">{p.current ? "this window" : p.open ? "open" : ""}</span>
                </button>
              ))}
            </div>
          ))}
          {me?.server && signedOut && (
            <button className="ctx-item ui-option separated" onClick={() => { setMenu(null); setAgain(true); }}>
              Sign in again…
            </button>
          )}
          {me?.server && !signedOut && (
            <button className="ctx-item ui-option separated" onClick={members} title="Opens the workspace's members on its server, in your browser">
              Members…
            </button>
          )}
          <button className={`ctx-item ui-option${me?.server ? "" : " separated"}`} onClick={() => { setMenu(null); setMaking(true); }}>
            New workspace…
          </button>
          <button className="ctx-item ui-option" onClick={() => { setMenu(null); setManaging(true); }}>
            Manage workspaces…
          </button>
        </Floating>
      )}

      {(making || again) && (
        <NewWorkspace
          again={again ? (me?.server ?? undefined) : undefined}
          onClose={() => { setMaking(false); setAgain(false); }}
          onOpened={(name) => {
            setMaking(false);
            load();
            toast("success", `${name} is opening in a window of its own.`);
          }}
        />
      )}
      {managing && <ManageProfiles profiles={profiles} reload={load} onClose={() => setManaging(false)} />}
    </div>
  );
}

/** A profile's initial on its colour, as its Dock icon has it (PROF-6). */
function Initial({ profile }: { profile?: ProfileView }) {
  const letter = profile?.name.trim().charAt(0).toUpperCase() ?? "";
  return (
    <span className="profile-initial" style={{ background: `var(--profile-${profile?.colour ?? 0})` }} aria-hidden>
      {letter}
    </span>
  );
}

/** How many colours a profile can have (PROF-10): `--profile-0` to `-7`. */
const COLOURS = 8;

/** The colours to choose from for `profile`, under its initial (PROF-10). */
function ColourPicker({
  profile, at, onPick, onClose,
}: {
  profile: ProfileView;
  at: { x: number; y: number };
  onPick: (colour: number) => void;
  onClose: () => void;
}) {
  return (
    <Floating x={at.x} y={at.y} onClose={onClose} className="ui-menu ctx-menu profile-colours">
      {Array.from({ length: COLOURS }, (_, i) => (
        <button
          key={i}
          className={`profile-swatch${i === profile.colour ? " chosen" : ""}`}
          style={{ background: `var(--profile-${i})` }}
          aria-label={`Colour ${i + 1}`}
          onClick={() => { onClose(); onPick(i); }}
        />
      ))}
    </Floating>
  );
}

function ManageProfiles({
  profiles, reload, onClose,
}: {
  profiles: ProfileView[];
  reload: () => void;
  onClose: () => void;
}) {
  const toast = useStore((s) => s.toast);
  // Renaming and recolouring fail above the dialog's button, deleting in
  // its own dialog (NOTE-6).
  const failure = useFailure();
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<ProfileView | null>(null);
  const [picking, setPicking] = useState<{ profile: ProfileView; x: number; y: number } | null>(null);

  function recolour(p: ProfileView, colour: number) {
    if (colour === p.colour) return;
    failure.clear();
    api.setProfileColour(p.id, colour).then(reload).catch(failure.fail);
  }

  function rename() {
    if (!renaming) return;
    const was = profiles.find((p) => p.id === renaming.id)?.name;
    // Left empty or as it was: nothing to send, the field just closes.
    if (!renaming.name.trim() || renaming.name === was) { setRenaming(null); return; }
    failure.clear();
    api.renameProfile(renaming.id, renaming.name)
      .then(() => { setRenaming(null); reload(); })
      .catch(failure.fail);
  }

  /** Why a profile cannot be deleted, if it cannot (PROF-9). */
  const kept = (p: ProfileView) =>
    p.id === "default"
      ? "The workspace Layer had before workspaces stays."
      : p.current
        ? "This window's workspace. Delete it from another workspace's window."
        : p.open
          ? "Open in another window. Quit it first."
          : null;

  return (
    <Modal title="Workspaces" onClose={onClose} footer={<button className="ui-button" onClick={onClose}>Done</button>}>
      <div className="profile-rows">
        {profiles.map((p) => (
          <div key={p.id} className="profile-row">
            <button
              className="profile-recolour"
              title="Choose its colour"
              onClick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                setPicking({ profile: p, x: r.left, y: r.bottom + 6 });
              }}
            >
              <Initial profile={p} />
            </button>
            {renaming?.id === p.id ? (
              <input
                className="ui-field profile-rename"
                value={renaming.name}
                maxLength={40}
                autoFocus
                // Typing replaces the name, as renaming in Finder does.
                onFocus={(e) => e.target.select()}
                onChange={(e) => setRenaming({ id: p.id, name: e.target.value })}
                // Saves when you leave it or press Enter, with no Save button
                // (studio/design/behaviour.md); Esc keeps the old name.
                onBlur={(e) => { if (!e.currentTarget.dataset.cancelled) rename(); }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") e.currentTarget.blur();
                  if (e.key === "Escape") {
                    e.stopPropagation();
                    // The field goes; a blur on its way out must not rename.
                    e.currentTarget.dataset.cancelled = "1";
                    setRenaming(null);
                  }
                }}
              />
            ) : (
              <span className="profile-row-name">
                {p.name}
                <span className="profile-state">{p.current ? "this window" : p.open ? "open" : ""}</span>
              </span>
            )}
            {renaming?.id === p.id ? null : (
              <>
                <button className="ui-button" onClick={() => setRenaming({ id: p.id, name: p.name })}>Rename</button>
                <button
                  className="ui-button ui-button-danger"
                  disabled={kept(p) !== null}
                  title={kept(p) ?? undefined}
                  onClick={() => setDeleting(p)}
                >
                  Delete
                </button>
              </>
            )}
          </div>
        ))}
      </div>

      <ErrorMessage>{failure.error}</ErrorMessage>

      {picking && (
        <ColourPicker
          profile={picking.profile}
          at={picking}
          onPick={(colour) => recolour(picking.profile, colour)}
          onClose={() => setPicking(null)}
        />
      )}

      {deleting && (
        <Confirm
          title={`Delete ${deleting.name}`}
          confirmLabel="Delete workspace"
          busyLabel="Deleting…"
          body={
            <>
              Its repositories, tasks, settings, connections and tokens, its notes, its messages and its
              run log are deleted.
              <div className="confirm-detail">
                Its task folders stay where they are, since they can hold work not yet committed:
                <div><b>{deleting.worktree_root}</b></div>
              </div>
              <div style={{ marginTop: 8 }}>This can't be undone.</div>
            </>
          }
          onCancel={() => setDeleting(null)}
          onConfirm={() =>
            api.deleteProfile(deleting.id)
              .then(() => {
                toast("success", `Deleted ${deleting.name}.`);
                setDeleting(null);
                reload();
              })
          }
        />
      )}
    </Modal>
  );
}
