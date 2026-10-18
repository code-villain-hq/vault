import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errMessage } from "../../lib/api";
import type { ProfileView } from "../../lib/types-profiles";
import { useStore } from "../../store";
import { Confirm, Field, Floating, Modal } from "../ui";
import { ChevronIcon } from "../icons";

/**
 * The window's profile, at the foot of the sidebar (§25), and the way to the
 * others: each opens in a copy of the app of its own (PROF-2), or brings that
 * copy forward if it is open already (PROF-3).
 *
 * The list is read when the menu opens and when `profiles:changed` says so:
 * another window may have made, renamed or deleted one since.
 */
export function ProfileMenu() {
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const [profiles, setProfiles] = useState<ProfileView[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [making, setMaking] = useState(false);
  const [managing, setManaging] = useState(false);
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
        title="Profiles: each is a whole Layer of its own, in a window of its own"
      >
        <Initial profile={me} />
        <span className="profile-name">{me?.name ?? "Profile"}</span>
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
          <div className="profile-menu-head">Profiles</div>
          {profiles.map((p) => (
            <button key={p.id} className="ctx-item ui-option profile-item" onClick={() => open(p)}>
              <Initial profile={p} />
              <span className="profile-item-name">{p.name}</span>
              <span className="profile-state">{p.current ? "this window" : p.open ? "open" : ""}</span>
            </button>
          ))}
          <button className="ctx-item ui-option separated" onClick={() => { setMenu(null); setMaking(true); }}>
            New profile…
          </button>
          <button className="ctx-item ui-option" onClick={() => { setMenu(null); setManaging(true); }}>
            Manage profiles…
          </button>
        </Floating>
      )}

      {making && (
        <NewProfile
          onClose={() => setMaking(false)}
          onMade={(p) => {
            setMaking(false);
            load();
            toast("success", `${p.name} is opening in a window of its own.`);
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

function NewProfile({ onClose, onMade }: { onClose: () => void; onMade: (p: ProfileView) => void }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function make() {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    api.createProfile(name)
      .then(onMade)
      .catch((e) => setError(errMessage(e)))
      .finally(() => setBusy(false));
  }

  return (
    <Modal
      title="New profile"
      onClose={onClose}
      footer={
        <>
          <button className="ui-button" onClick={onClose}>Cancel</button>
          <button className="ui-button ui-button-primary" disabled={!name.trim() || busy} onClick={make}>
            {busy ? "Opening…" : "Create and open"}
          </button>
        </>
      }
    >
      <Field
        label="Name"
        hint="A Layer of its own, empty: its own repositories, tasks, connections and tokens. It opens in a new window, beside this one."
      >
        <input className="ui-field"
          value={name}
          maxLength={40}
          autoFocus
          placeholder="My project"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && make()}
        />
      </Field>
      {error && <div className="profile-error">{error}</div>}
    </Modal>
  );
}

function ManageProfiles({
  profiles, reload, onClose,
}: {
  profiles: ProfileView[];
  reload: () => void;
  onClose: () => void;
}) {
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<ProfileView | null>(null);
  const [picking, setPicking] = useState<{ profile: ProfileView; x: number; y: number } | null>(null);

  function recolour(p: ProfileView, colour: number) {
    if (colour === p.colour) return;
    api.setProfileColour(p.id, colour).then(reload).catch(fail);
  }

  function rename() {
    if (!renaming) return;
    api.renameProfile(renaming.id, renaming.name)
      .then(() => { setRenaming(null); reload(); })
      .catch(fail);
  }

  /** Why a profile cannot be deleted, if it cannot (PROF-9). */
  const kept = (p: ProfileView) =>
    p.id === "default"
      ? "The profile Layer had before profiles stays."
      : p.current
        ? "This window's profile. Delete it from another profile's window."
        : p.open
          ? "Open in another window. Quit it first."
          : null;

  return (
    <Modal title="Profiles" onClose={onClose} footer={<button className="ui-button" onClick={onClose}>Done</button>}>
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
                onKeyDown={(e) => {
                  if (e.key === "Enter") rename();
                  if (e.key === "Escape") { e.stopPropagation(); setRenaming(null); }
                }}
              />
            ) : (
              <span className="profile-row-name">
                {p.name}
                <span className="profile-state">{p.current ? "this window" : p.open ? "open" : ""}</span>
              </span>
            )}
            {renaming?.id === p.id ? (
              <>
                <button className="ui-button ui-button-primary" disabled={!renaming.name.trim()} onClick={rename}>Save</button>
                <button className="ui-button" onClick={() => setRenaming(null)}>Cancel</button>
              </>
            ) : (
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
          confirmLabel="Delete profile"
          busyLabel="Deleting…"
          body={
            <>
              Its repositories list, tasks, settings, connections and tokens, its messages and its
              run log are deleted. This cannot be undone.
              <div className="confirm-detail">
                Its task folders stay where they are, since they can hold work not yet committed:
                <div><b>{deleting.worktree_root}</b></div>
              </div>
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
              .catch((e) => { setDeleting(null); fail(e); })
          }
        />
      )}
    </Modal>
  );
}
