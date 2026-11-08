import { useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import { CloseIcon } from "./icons";
import { ErrorMessage, Field, useFailure } from "./ui";

/** Settings → Browser: the sites agents may use besides this machine's (BRW-3). */
export function BrowserSettings() {
  const sites = useStore((s) => s.settings?.browser_sites) ?? [];
  const refreshSettings = useStore((s) => s.refreshSettings);
  // A site that could not be added or removed says so under the list (NOTE-6).
  const failure = useFailure();
  const [adding, setAdding] = useState("");

  async function save(next: string[]) {
    failure.clear();
    try {
      await api.setBrowserSites(next);
      await refreshSettings();
    } catch (e) {
      failure.fail(e);
    }
  }

  async function add(e: React.FormEvent) {
    e.preventDefault();
    const site = adding.trim();
    if (!site) return;
    const before = sites.length;
    failure.clear();
    try {
      const kept = await api.setBrowserSites([...sites, site]);
      await refreshSettings();
      if (kept.length === before)
        failure.fail(`${site} is not a site, or is already allowed. Give one like github.com.`);
      else setAdding("");
    } catch (err) {
      failure.fail(err);
    }
  }

  return (
    <>
      <Field
        label="Sites agents may use"
        hint={
          <>
            Agents open this Mac's pages whenever they like, and other sites only as listed here or
            asked for in a task's browser: a page's text can tell an agent what to do. A site covers
            its subdomains.
          </>
        }
      >
        <div className="col">
          {sites.length === 0 && (
            <span className="muted">None yet: only this machine's pages.</span>
          )}
          {sites.map((s) => (
            <div key={s} className="row site-row">
              <code>{s}</code>
              <div className="spacer" />
              <button
                className="ui-button ui-icon-button"
                title={`Stop agents using ${s}`}
                onClick={() => void save(sites.filter((x) => x !== s))}
              >
                <CloseIcon />
              </button>
            </div>
          ))}
          <form className="row" onSubmit={(e) => void add(e)}>
            <input
              className="ui-field"
              value={adding}
              placeholder="github.com"
              spellCheck={false}
              onChange={(e) => setAdding(e.target.value)}
            />
            <button className="ui-button" disabled={!adding.trim()}>
              Allow
            </button>
          </form>
          <ErrorMessage>{failure.error}</ErrorMessage>
        </div>
      </Field>
      <SignIns />
    </>
  );
}

/** Saved sign-ins (BRW-18): agents have them filled in, and never see the password. */
function SignIns() {
  const signIns = useStore((s) => s.settings?.browser_sign_ins) ?? [];
  const refreshSettings = useStore((s) => s.refreshSettings);
  const failure = useFailure();
  const [site, setSite] = useState("localhost");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  async function add(e: React.FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    failure.clear();
    try {
      await api.addBrowserSignIn(site.trim(), username.trim(), password);
      await refreshSettings();
      setUsername("");
      setPassword("");
    } catch (err) {
      failure.fail(err);
    } finally {
      setBusy(false);
    }
  }

  async function forget(id: string) {
    failure.clear();
    try {
      await api.forgetBrowserSignIn(id);
      await refreshSettings();
    } catch (e) {
      failure.fail(e);
    }
  }

  return (
    <Field
      label="Saved sign-ins"
      hint={
        <>
          The app types the password in, so an agent never sees it, and keeps it in the keychain.{" "}
          <code>localhost</code> is every port of this Mac; elsewhere, that host only. Use test
          accounts.
        </>
      }
    >
      <div className="col">
        {signIns.length === 0 && <span className="muted">None saved.</span>}
        {signIns.map((s) => (
          <div key={s.id} className="row site-row">
            <code>{s.site}</code>
            <span>{s.username}</span>
            <div className="spacer" />
            <button
              className="ui-button ui-icon-button"
              title={`Forget the sign-in for ${s.username} on ${s.site}`}
              onClick={() => void forget(s.id)}
            >
              <CloseIcon />
            </button>
          </div>
        ))}
        <form className="row" onSubmit={(e) => void add(e)}>
          <input
            className="ui-field"
            value={site}
            placeholder="localhost"
            spellCheck={false}
            style={{ width: 140 }}
            onChange={(e) => setSite(e.target.value)}
          />
          <input
            className="ui-field"
            value={username}
            placeholder="username or email"
            spellCheck={false}
            onChange={(e) => setUsername(e.target.value)}
          />
          <input
            className="ui-field"
            type="password"
            value={password}
            placeholder="password"
            autoComplete="off"
            onChange={(e) => setPassword(e.target.value)}
          />
          <button
            className="ui-button"
            disabled={busy || !site.trim() || !username.trim() || !password}
          >
            Save
          </button>
        </form>
        <ErrorMessage>{failure.error}</ErrorMessage>
      </div>
    </Field>
  );
}
