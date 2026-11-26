import { useState } from "react";
import { api } from "../lib/api";
import type { SavedSignIn } from "../lib/types-browser";
import { useStore } from "../store";
import { EyeIcon, EyeOffIcon } from "./icons";
import { ErrorMessage, Field, useFailure } from "./ui";

/** Which form is open: one at a time, to add a sign-in or to change one. */
type Open = { kind: "add" } | { kind: "change"; id: string } | null;

/**
 * Saved sign-ins (BRW-18): agents have them filled in, and never see the
 * password. A row opens in place to be changed: before, a new password
 * meant forgetting the sign-in and typing all of it again.
 */
export function BrowserSignIns() {
  const signIns = useStore((s) => s.settings?.browser_sign_ins) ?? [];
  const [open, setOpen] = useState<Open>(null);

  return (
    <Field
      label="Saved sign-ins"
      hint="The app types the password in, so agents never see it. It stays in the keychain. Use test accounts. A site can hold a *, as in *.staging.example.com, to use one account on all of them."
    >
      {signIns.length > 0 && (
        <div className="sign-ins">
          {signIns.map((s) =>
            open?.kind === "change" && open.id === s.id ? (
              <SignInForm key={s.id} saved={s} onDone={() => setOpen(null)} />
            ) : (
              <button
                key={s.id}
                className="ui-row sign-in-row"
                title={`Change the sign-in for ${s.username} on ${s.site}`}
                onClick={() => setOpen({ kind: "change", id: s.id })}
              >
                <code className="sign-in-site">{s.site}</code>
                <span className="sign-in-user">{s.username}</span>
                <span className="sign-in-note">{everyPort(s.site) ? "any port" : ""}</span>
                <span className="sign-in-change">Change</span>
              </button>
            ),
          )}
        </div>
      )}
      {open?.kind === "add" ? (
        <SignInForm onDone={() => setOpen(null)} />
      ) : (
        <button className="ui-button" onClick={() => setOpen({ kind: "add" })}>
          Add a sign-in
        </button>
      )}
    </Field>
  );
}

/** A site saved without a port on this machine covers every port of it (BRW-18). */
function everyPort(site: string) {
  return ["localhost", "127.0.0.1", "[::1]"].includes(site) || /^[^:]+\.localhost$/.test(site);
}

/**
 * The form for a new sign-in, or for changing `saved`. Changing one, the
 * password field starts empty and an empty one keeps the saved password,
 * which never comes back to the window to be shown.
 */
function SignInForm({ saved, onDone }: { saved?: SavedSignIn; onDone: () => void }) {
  const refreshSettings = useStore((s) => s.refreshSettings);
  const failure = useFailure();
  const [site, setSite] = useState(saved?.site ?? "localhost");
  const [username, setUsername] = useState(saved?.username ?? "");
  const [password, setPassword] = useState("");
  const [shown, setShown] = useState(false);
  const [busy, setBusy] = useState(false);

  const filled = !!site.trim() && !!username.trim();
  const ready = saved
    ? filled &&
      (site.trim() !== saved.site || username.trim() !== saved.username || password !== "")
    : filled && password !== "";

  async function run(work: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    failure.clear();
    try {
      await work();
      await refreshSettings();
      onDone();
    } catch (err) {
      failure.fail(err);
    } finally {
      setBusy(false);
    }
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!ready) return;
    void run(() =>
      saved
        ? api.changeBrowserSignIn(saved.id, site.trim(), username.trim(), password || null)
        : api.addBrowserSignIn(site.trim(), username.trim(), password),
    );
  }

  return (
    <form
      className="sign-in-form"
      onSubmit={submit}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          // Closes this form, not the whole Settings dialog.
          e.stopPropagation();
          onDone();
        }
      }}
    >
      <div className="sign-in-fields">
        <input
          className="ui-field"
          value={site}
          placeholder="localhost"
          aria-label="Site"
          spellCheck={false}
          onChange={(e) => setSite(e.target.value)}
        />
        <input
          className="ui-field"
          value={username}
          placeholder="Username or email"
          aria-label="Username or email"
          spellCheck={false}
          autoFocus={!saved}
          onChange={(e) => setUsername(e.target.value)}
        />
        <div className="password-field">
          <input
            className="ui-field"
            type={shown ? "text" : "password"}
            value={password}
            placeholder={saved ? "New password" : "Password"}
            aria-label={saved ? "New password" : "Password"}
            autoComplete="off"
            spellCheck={false}
            // Changing one, the password is what is usually changed.
            autoFocus={!!saved}
            onChange={(e) => setPassword(e.target.value)}
          />
          <button
            type="button"
            className="password-eye"
            aria-pressed={shown}
            aria-label={shown ? "Hide the password" : "Show the password"}
            title={shown ? "Hide the password" : "Show the password"}
            onClick={() => setShown(!shown)}
          >
            {shown ? <EyeOffIcon size={14} /> : <EyeIcon size={14} />}
          </button>
        </div>
      </div>
      <div className="sign-in-foot">
        <button className="ui-button" disabled={busy || !ready}>
          {saved ? "Save" : "Save sign-in"}
        </button>
        <button type="button" className="ui-button sign-in-cancel" onClick={onDone}>
          Cancel
        </button>
        <span className="sign-in-help">
          {saved
            ? "Leave the password empty to keep it."
            : "localhost is every port of this Mac; elsewhere, that host only."}
        </span>
        {saved && (
          <button
            type="button"
            className="ui-button ui-button-danger"
            disabled={busy}
            onClick={() => void run(() => api.forgetBrowserSignIn(saved.id))}
          >
            Forget
          </button>
        )}
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </form>
  );
}
