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
      if (kept.length === before) failure.fail(`${site} is not a site, or is already allowed. Give one like github.com.`);
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
            Each task's agents have a browser tab, and open this machine's pages
            (localhost) in it whenever they like. Anywhere else only as listed here: a
            page's text is written by whoever runs the site, and an agent reads it as
            something that may tell it what to do. A site covers its subdomains. An
            agent can ask for one; the request shows in its task's browser. The browser
            is Google Chrome with a profile of its own, apart from yours: what you sign in
            to there stays signed in for every task.
          </>
        }
      >
        <div className="col">
          {sites.length === 0 && <span className="muted">None yet: only this machine's pages.</span>}
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
            <input className="ui-field"
              value={adding}
              placeholder="github.com"
              spellCheck={false}
              onChange={(e) => setAdding(e.target.value)}
            />
            <button className="ui-button" disabled={!adding.trim()}>Allow</button>
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
          Agents sign in with these through the app, which types the password into the
          page's password field itself: an agent never sees it. Passwords are kept in the
          keychain. <code>localhost</code> alone is every port of this machine; a site
          elsewhere is that host only. Use test accounts: an agent signed in can do what
          the account can. You can also save one from the browser, after typing it into a
          page (the key in its toolbar).
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
          <input className="ui-field" value={site} placeholder="localhost" spellCheck={false} style={{ width: 140 }} onChange={(e) => setSite(e.target.value)} />
          <input className="ui-field" value={username} placeholder="username or email" spellCheck={false} onChange={(e) => setUsername(e.target.value)} />
          <input className="ui-field" type="password" value={password} placeholder="password" autoComplete="off" onChange={(e) => setPassword(e.target.value)} />
          <button className="ui-button" disabled={busy || !site.trim() || !username.trim() || !password}>Save</button>
        </form>
        <ErrorMessage>{failure.error}</ErrorMessage>
      </div>
    </Field>
  );
}
