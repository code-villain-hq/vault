import { useRef, useState } from "react";
import { api } from "../lib/api";
import type { SignInForm } from "../lib/types";
import { useStore } from "../store";
import { KeyIcon } from "./icons";
import { ErrorMessage, Field, Modal, useFailure } from "./ui";

/**
 * "Save sign-in" in the browser panel (BRW-20): what the user typed into the
 * page's sign-in form, saved for agents to use. The password is read from
 * the page and saved by the backend; it never comes to this window.
 */
export function SaveSignIn({ taskId, disabled }: { taskId: string; disabled: boolean }) {
  // Reading the form is a toolbar icon's, with no place of its own, so it
  // fails in the top bar; saving fails in the dialog (NOTE-6).
  const fail = useStore((s) => s.fail);
  const dialog = useFailure();
  const toast = useStore((s) => s.toast);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const [form, setForm] = useState<SignInForm | null>(null);
  const [site, setSite] = useState("");
  const [username, setUsername] = useState("");
  const [busy, setBusy] = useState(false);
  const saving = useRef(false);

  async function start() {
    try {
      const f = await api.browserSignInForm(taskId);
      dialog.clear();
      setForm(f);
      setSite(f.site);
      setUsername(f.username ?? "");
    } catch (e) {
      fail(e);
    }
  }

  async function save() {
    if (saving.current) return;
    saving.current = true;
    setBusy(true);
    dialog.clear();
    try {
      const saved = await api.browserSaveSignIn(taskId, site.trim(), username.trim());
      await refreshSettings();
      toast("success", `Saved the sign-in for ${saved.username} on ${saved.site}: agents can use it now.`);
      setForm(null);
    } catch (e) {
      dialog.fail(e);
    } finally {
      saving.current = false;
      setBusy(false);
    }
  }

  return (
    <>
      <button
        className="ui-button ui-icon-button"
        title="Save the sign-in typed into this page, for agents to use without seeing the password"
        disabled={disabled}
        onClick={() => void start()}
      >
        <KeyIcon />
      </button>
      {form && (
        <Modal
          title="Save sign-in"
          onClose={() => { if (!busy) setForm(null); }}
          footer={
            <>
              <button className="ui-button" disabled={busy} onClick={() => setForm(null)}>Cancel</button>
              <button
                className="ui-button ui-button-primary"
                disabled={busy || !site.trim() || !username.trim()}
                onClick={() => void save()}
              >
                {busy ? "Saving…" : "Save"}
              </button>
            </>
          }
        >
          <Field
            label="Site"
            hint="localhost alone is every port of this machine; a site elsewhere is that host only."
          >
            <input className="ui-field" value={site} onChange={(e) => setSite(e.target.value)} spellCheck={false} />
          </Field>
          <Field
            label="Username"
            hint={form.username ? undefined : "The form has none: a login that asks for it on the step before."}
          >
            <input className="ui-field" value={username} onChange={(e) => setUsername(e.target.value)} spellCheck={false} autoFocus={!form.username} />
          </Field>
          <p className="muted" style={{ margin: 0, lineHeight: 1.5 }}>
            The password is the one typed into the page's password field. It goes to the keychain;
            agents can have it filled in, and never see it.
          </p>
          <ErrorMessage>{dialog.error}</ErrorMessage>
        </Modal>
      )}
    </>
  );
}
