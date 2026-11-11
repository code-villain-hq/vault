import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { limitsTitle, limitWindows, providerName, type PlanLimits } from "../../lib/limits";
import { useNow, useStore } from "../../store";

/**
 * How much of each agent plan is used and when each window starts over,
 * always in the sidebar's corner (§27). Nothing until a provider has said.
 * Clicking asks OpenAI again; Claude's come as its agents reply (LIMIT-2).
 */
export function PlanLimitsStrip() {
  const fail = useStore((s) => s.fail);
  // Minutes are the finest a countdown shows.
  const now = useNow(30_000);
  const [limits, setLimits] = useState<PlanLimits[]>([]);
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    let current = true;
    api
      .planLimits()
      .then((l) => {
        if (current) setLimits(l);
      })
      .catch(fail);
    const p = listen<PlanLimits[]>("limits:changed", (e) => setLimits(e.payload));
    return () => {
      current = false;
      void p.then((un) => un());
    };
  }, [fail]);

  if (limits.length === 0) return null;

  const refresh = () => {
    setAsking(true);
    api
      .refreshPlanLimits()
      .then(setLimits)
      .catch(fail)
      .finally(() => setAsking(false));
  };

  return (
    <div className="plan-limits" aria-label="Agent plan limits">
      {limits.map((l) => (
        <button
          key={l.provider}
          type="button"
          className="plan-limit"
          title={limitsTitle(l, now)}
          disabled={asking}
          onClick={refresh}
        >
          <span className="plan-name">{providerName(l.provider)}</span>
          {limitWindows(l, now).map((w) => (
            <span key={w.label} className={`plan-window ${w.tone}`}>
              <span className="plan-label">{w.label}</span> {w.used}
              {w.left && <span className="plan-left"> {w.left}</span>}
            </span>
          ))}
        </button>
      ))}
    </div>
  );
}
