import { useStore } from "../store";
import { TOKEN_REJECTED } from "../lib/derive";
import type { CheckoutPr } from "../lib/types";

/**
 * The repos whose pull requests could not be read. A rejected token is said
 * once, with the way out, since it is every repo's failure at once: shown per
 * repo it was the same raw "Bad credentials" JSON on each line (PR-12).
 */
export function PrReadErrors({ rows }: { rows: CheckoutPr[] }) {
  const toggleSettings = useStore((s) => s.toggleSettings);
  const failed = rows.filter((r) => r.error);
  if (failed.length === 0) return null;
  const rejected = failed.some((r) => r.error === TOKEN_REJECTED);
  const others = failed.filter((r) => r.error !== TOKEN_REJECTED);

  return (
    <>
      {rejected && (
        <div className="card">
          <h3>{TOKEN_REJECTED}</h3>
          <p className="muted">It may have expired or been revoked. Save a new one to read pull requests again.</p>
          <button className="ui-button" onClick={() => toggleSettings(true, "github")}>Reconnect GitHub</button>
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
