import { openUrl } from "@tauri-apps/plugin-opener";
import { TOKEN_NEEDS_SSO, TOKEN_REJECTED } from "../lib/derive";
import type { CheckoutPr } from "../lib/types";
import { useStore } from "../store";

/** A token warning for one source must not hide another source's warning. */
export function PrReadErrors({ rows }: { rows: CheckoutPr[] }) {
  const toggleSettings = useStore((s) => s.toggleSettings);
  const sources = useStore((s) => s.settings?.github_sources);
  const fail = useStore((s) => s.fail);
  const auth = new Map<string, CheckoutPr[]>();
  const others: CheckoutPr[] = [];
  for (const row of rows) {
    if (row.error === TOKEN_REJECTED || row.error === TOKEN_NEEDS_SSO) {
      const source = row.source ?? "";
      auth.set(source, [...(auth.get(source) ?? []), row]);
    } else if (row.error) {
      others.push(row);
    }
  }
  return (
    <>
      {[...auth].map(([source, failed]) => {
        const rejected = failed.some((r) => r.error === TOKEN_REJECTED);
        const webUrl = sources?.find((g) => g.key === source)?.web_url;
        return (
          <div key={source} className="card">
            <h3>
              {source ? `${source}: ` : ""}
              {rejected ? TOKEN_REJECTED : TOKEN_NEEDS_SSO}
            </h3>
            <p className="muted">
              {rejected
                ? "Save a new token to read pull requests from this source again."
                : `Authorize the saved token for the organization that owns ${failed.map((r) => r.repo).join(", ")}. Layer checks again on its own.`}
            </p>
            {rejected ? (
              <button className="ui-button" onClick={() => toggleSettings(true, "github")}>
                Reconnect {source || "GitHub"}
              </button>
            ) : (
              webUrl && (
                <button
                  className="ui-button"
                  onClick={() => void openUrl(`${webUrl}/settings/tokens`).catch(fail)}
                >
                  Open token settings
                </button>
              )
            )}
          </div>
        );
      })}
      {others.length > 0 && (
        <div className="card">
          <h3>Repos that could not be read</h3>
          {others.map((r) => (
            <div key={r.checkout_id} className="check">
              <span className="dot" style={{ background: "var(--status-danger-ink)" }} />
              <span className="name">
                {r.repo}
                {r.source ? ` · ${r.source}` : ""}
              </span>
              <span className="muted">{r.error}</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
