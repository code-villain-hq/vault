/**
 * Plan limits (§27) for the mock harness: Claude's read a moment ago with its
 * week past the warning, OpenAI's read earlier with its five hours nearly
 * spent, so the corner shows every tone. The empty scenario has none, as a
 * Mac where no provider has said anything yet.
 */
import type { PlanLimits } from "../lib/types-limits";

const MIN = 60_000;

function seen(now: number): PlanLimits[] {
  return [
    {
      provider: "claude",
      session: { used_percent: 42, resets_at: now + 133 * MIN },
      weekly: { used_percent: 83, resets_at: now + 3 * 24 * 60 * MIN },
      read_at: now - 2 * MIN,
    },
    {
      provider: "openai",
      session: { used_percent: 96.4, resets_at: now + 41 * MIN },
      weekly: { used_percent: 30, resets_at: now + 5 * 24 * 60 * MIN },
      read_at: now - 6 * MIN,
    },
  ];
}

export function limitAnswers(seeded: boolean) {
  let limits = seeded ? seen(Date.now()) : [];
  return {
    plan_limits: () => structuredClone(limits),
    // Asking OpenAI again: its row reads just now.
    refresh_plan_limits: () => {
      limits = limits.map((l) => (l.provider === "openai" ? { ...l, read_at: Date.now() } : l));
      return structuredClone(limits);
    },
  };
}
