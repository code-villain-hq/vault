/**
 * Each agent plan's limits (§27), as `limits.rs` keeps them, mirrored here.
 * Imported from this file itself: `types.ts` is at its size ceiling.
 */

export interface PlanWindow {
  /** From 0 to 100. */
  used_percent: number;
  /** Epoch milliseconds, when the provider said. */
  resets_at: number | null;
}

export interface PlanLimits {
  /** `claude` or `openai`. */
  provider: string;
  /** The five-hour window. */
  session: PlanWindow | null;
  weekly: PlanWindow | null;
  /** Epoch milliseconds. */
  read_at: number;
}
