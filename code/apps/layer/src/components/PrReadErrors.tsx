import { openUrl } from "@tauri-apps/plugin-opener";
import { useStore } from "../store";
import { TOKEN_NEEDS_SSO, TOKEN_REJECTED } from "../lib/derive";
import type { CheckoutPr } from "../lib/types";

/**
 * The repos whose pull requests could not be read. A refused token is said
 * once, with the way out, since it is every repo's failure at once: shown per
 * repo it was the same raw "Bad credentials" JSON on each line (PR-12). A
 * token not authorized for an organization's single sign-on was the same,
 * with GitHub's "SAML enforcement" JSON.
 */
export function PrReadErrors({ rows }: { rows: CheckoutPr[] }) {
  const toggleSettings = useStore((s) => s.toggleSettings);
  const refreshPrs = useStore((s) => s.refreshPrs);
  const webUrl = useStore((s) => s.settings?.github?.web_url);
  const fail = useStore((s) => s.fail);
  const failed = rows.filter((r) => r.error);
  if (failed.length === 0) return null;
  const rejected = failed.some((r) => r.error === TOKEN_REJECTED);
  const sso = failed.filter((r) => r.error === TOKEN_NEEDS_SSO);
  const others = failed.filter((r) => r.error !== TOKEN_REJECTED && r.error !== TOKEN_NEEDS_SSO);

  return (
    <>
      {rejected && (
        <div className="card">
          <h3>{TOKEN_REJECTED}</h3>
          <p className="muted">It may have expired or been revoked. Save a new one to read pull requests again.</p>
          <button className="ui-button" onClick={() => toggleSettings(true, "github")}>Reconnect GitHub</button>
        </div>
      )}
      {sso.length > 0 && !rejected && (
        <div className="card">
          <h3>{TOKEN_NEEDS_SSO}</h3>
          <p className="muted">
            The organization that owns {sso.map((r) => r.repo).join(", ")} requires single sign-on. In
            GitHub's token settings, choose Configure SSO beside the token Layer uses and authorize it
            for the organization. The same token then works; nothing needs saving again.
          </p>
          <div className="row">
            {webUrl && (
              <button
                className="ui-button"
                onClick={() => void openUrl(`${webUrl}/settings/tokens`).catch(() => fail("Couldn't open the browser. Try again."))}
              >
                Open token settings
              </button>
            )}
            <button className="ui-button" onClick={() => void refreshPrs()}>Check again</button>
          </div>
        </div>
      )}
      {others.length > 0 && (
        <div className="card">
          <h3>Repos that could not be read</h3>
          {others.map((r) => (
            <div key={r.checkout_id} className="check">
              <span className="dot" style={{ background: "var(--status-danger-ink)" }} />
              <span className="name">{r.repo}</span>
              <span className="muted">{r.error}</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
