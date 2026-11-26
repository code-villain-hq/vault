import { useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import { BrowserSignIns } from "./BrowserSignIns";
import { CloseIcon } from "./icons";
import { ErrorMessage, Field, useFailure } from "./ui";

/**
 * Settings → Browser: the sites agents may use besides this machine's
 * (BRW-3), and the saved sign-ins (BRW-18).
 *
 * The sites are chips, not a row each: fourteen hosts as fourteen tall rows
 * pushed the field to add one, and the explanation, below the dialog's
 * fold. The field to add one comes first, as the action the list needs.
 */
export function BrowserSettings() {
  const sites = useStore((s) => s.settings?.browser_sites) ?? [];
  const refreshSettings = useStore((s) => s.refreshSettings);
  // A site that could not be added or removed says so under the list (NOTE-6).
  const failure = useFailure();
  const [adding, setAdding] = useState("");

  async function remove(site: string) {
    failure.clear();
    try {
      await api.setBrowserSites(sites.filter((x) => x !== site));
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
        label="Sites agents may open"
        hint="A page's text can tell an agent what to do, so agents open only these. A site covers its subdomains, and * stands for any part of a name, as in pr-*.example.com."
      >
        <form className="browser-add" onSubmit={(e) => void add(e)}>
          <input
            className="ui-field"
            value={adding}
            placeholder="Allow a site, like github.com"
            aria-label="Allow a site"
            spellCheck={false}
            onChange={(e) => setAdding(e.target.value)}
          />
          <button className="ui-button" disabled={!adding.trim()}>
            Allow
          </button>
        </form>
        <ul className="site-chips">
          {/* What needs no allowing, said where the list is rather than in
              a paragraph under it. */}
          <li className="ui-chip site-chip site-chip-mac" title="localhost, 127.0.0.1 and [::1]">
            This Mac, always
          </li>
          {sites.map((s) => (
            <li key={s} className="ui-chip site-chip">
              <code>{s}</code>
              <button
                className="site-chip-remove"
                aria-label={`Stop agents opening ${s}`}
                title={`Stop agents opening ${s}`}
                onClick={() => void remove(s)}
              >
                <CloseIcon size={12} />
              </button>
            </li>
          ))}
        </ul>
        <ErrorMessage>{failure.error}</ErrorMessage>
      </Field>
      <BrowserSignIns />
    </>
  );
}
