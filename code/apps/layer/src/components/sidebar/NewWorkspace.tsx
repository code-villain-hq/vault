import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { ServerSignedIn } from "../../lib/types-profiles";
import { signedInAgain } from "../../lib/workspace-sync";
import { ErrorMessage, Field, Modal, useFailure } from "../ui";

/** Where a new workspace goes when nobody says otherwise (PROF-15). */
const STUDIO_SERVER = "https://layer.codevillain.eu";

type Step =
  | { at: "where" }
  | { at: "browser"; company: string }
  | { at: "choose"; signed: ServerSignedIn };

/**
 * New workspace (§25): on this Mac, as a profile always was, or on a server
 * after signing in there with its company's account (PROF-15). With `again`, the server this window's
 * workspace is on, it signs in again instead (PROF-16).
 */
export function NewWorkspace({
  again, onClose, onOpened,
}: {
  again?: string;
  onClose: () => void;
  onOpened: (name: string) => void;
}) {
  const [onServer, setOnServer] = useState(Boolean(again));
  const [name, setName] = useState("");
  const [address, setAddress] = useState(again ?? STUDIO_SERVER);
  const [step, setStep] = useState<Step>({ at: "where" });
  const [busy, setBusy] = useState(false);
  const failure = useFailure();

  // Closing while the browser is out stops the wait.
  useEffect(() => () => void api.serverSignInCancel().catch(() => {}), []);

  function run<T>(work: Promise<T>, then: (value: T) => void) {
    setBusy(true);
    failure.clear();
    work.then(then).catch(failure.fail).finally(() => setBusy(false));
  }

  function makeHere() {
    if (!name.trim() || busy) return;
    run(api.createProfile(name), (p) => onOpened(p.name));
  }

  /** Signed in: on with this window's workspace, the company's, or a choice. */
  function signedIn(signed: ServerSignedIn) {
    if (again) return run(api.serverResume(), () => { signedInAgain(); onClose(); });
    if (signed.workspaces.length === 1) return open(signed, signed.workspaces[0].id);
    setStep({ at: "choose", signed });
  }

  function open(signed: ServerSignedIn, id: string) {
    const workspace = signed.workspaces.find((w) => w.id === id);
    run(api.serverOpen(id), () => onOpened(workspace?.name ?? signed.host));
  }

  function signIn() {
    if (!address.trim() || busy) return;
    run(api.serverWays(address), (ways) => {
      if (!ways.oidc) return failure.fail("That server signs nobody in. It's another Mac's own.");
      setStep({ at: "browser", company: ways.company ?? "the server's company" });
      setBusy(true);
      api.serverSignInBrowser(address)
        .then(signedIn)
        .catch((e) => { setStep({ at: "where" }); failure.fail(e); })
        .finally(() => setBusy(false));
    });
  }

  function back() {
    void api.serverSignInCancel().catch(() => {});
    failure.clear();
    setStep({ at: "where" });
  }

  const cancel = <button className="ui-button" onClick={onClose}>Cancel</button>;
  const footer =
    step.at === "choose" ? (
      cancel
    ) : step.at === "browser" ? (
      <button className="ui-button" onClick={back}>Cancel</button>
    ) : onServer ? (
      <>
        {cancel}
        <button className="ui-button ui-button-primary" disabled={!address.trim() || busy} onClick={signIn}>
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </>
    ) : (
      <>
        {cancel}
        <button className="ui-button ui-button-primary" disabled={!name.trim() || busy} onClick={makeHere}>
          {busy ? "Opening…" : "Create and open"}
        </button>
      </>
    );

  return (
    <Modal title={again ? "Sign in again" : "New workspace"} onClose={onClose} footer={footer}>
      {step.at === "where" && !again && (
        <Field label="Where">
          <div className="ui-segmented">
            <button className="ui-segment" aria-pressed={!onServer} onClick={() => { setOnServer(false); failure.clear(); }}>
              This Mac
            </button>
            <button className="ui-segment" aria-pressed={onServer} onClick={() => { setOnServer(true); failure.clear(); }}>
              A server
            </button>
          </div>
        </Field>
      )}

      {step.at === "where" && !onServer && (
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
            onKeyDown={(e) => e.key === "Enter" && makeHere()}
          />
        </Field>
      )}

      {step.at === "where" && onServer && (
        <Field
          label="Server"
          hint={again
            ? "Layer stopped being signed in to this server. What changed here waits, and goes once you're back."
            : (
              "Opens your company's sign-in in your browser. Your repositories and notes are shared with the workspace; tasks and agents stay on this Mac."
            )}
        >
          <input className="ui-field"
            value={address}
            autoFocus={!again}
            disabled={Boolean(again)}
            spellCheck={false}
            onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && signIn()}
          />
        </Field>
      )}

      {step.at === "browser" && (
        <p className="hint">
          Finish signing in with {step.company} in your browser. Layer waits for it here, for up to five minutes.
        </p>
      )}

      {step.at === "choose" && <Choose signed={step.signed} busy={busy} onOpen={(id) => open(step.signed, id)} />}

      <ErrorMessage>{failure.error}</ErrorMessage>
    </Modal>
  );
}

/** The person's workspaces on the server, each opened in its own window. */
function Choose({ signed, busy, onOpen }: { signed: ServerSignedIn; busy: boolean; onOpen: (id: string) => void }) {
  if (signed.workspaces.length === 0) {
    return (
      <p className="hint">
        Signed in to {signed.host} as {signed.email}, and in no workspace there yet. Ask {signed.company}'s admin to
        add you.
      </p>
    );
  }
  return (
    <>
      <p className="hint">Signed in to {signed.host} as {signed.email}. Open a workspace in a window of its own.</p>
      <div className="profile-rows">
        {signed.workspaces.map((w) => (
          <div key={w.id} className="profile-row">
            <span className="profile-row-name">{w.name}</span>
            <button className="ui-button" disabled={busy} onClick={() => onOpen(w.id)}>Open</button>
          </div>
        ))}
      </div>
    </>
  );
}
