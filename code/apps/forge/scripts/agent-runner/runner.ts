// The runner's logic, apart from how a run is started: checking Forge's
// signature, answering at once, keeping to one run at a time, and the
// schedule's check for work that wake-ups missed.

import { createHmac, timingSafeEqual } from "node:crypto";

/** A wake-up as Forge sends it (specs "Connecting agents", "Waking agents"). */
export type Wakeup = {
  id: string;
  event: string;
  issue: string;
  workspace_id: string;
  agent_id: string;
};

/**
 * How a run ended. `worked`: it did something, so there may be more to do.
 * `idle`: it found nothing to pick up. `failed`: it crashed or timed out.
 */
export type RunOutcome = "worked" | "idle" | "failed";

export type RunnerOptions = {
  secret: string;
  /** Starts one run, given the wake-ups since the last one started. */
  run: (wakeups: Wakeup[]) => Promise<RunOutcome>;
  log?: (message: string) => void;
  /** Runs in a row before waiting for the next wake-up. */
  maxRunsInARow?: number;
};

export const SIGNATURE_HEADER = "x-forge-signature";

/** `sha256=` and the hex HMAC-SHA256 of the body, keyed with the secret. */
export function sign(secret: string, body: string): string {
  return `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;
}

export function verify(secret: string, body: string, header: string | null): boolean {
  if (!header) return false;
  const expected = Buffer.from(sign(secret, body));
  const given = Buffer.from(header);
  return expected.length === given.length && timingSafeEqual(expected, given);
}

export function createRunner(options: RunnerOptions) {
  const log = options.log ?? (() => {});
  const maxRunsInARow = options.maxRunsInARow ?? 5;
  let pending: Wakeup[] = [];
  let current: Promise<void> | null = null;

  // Runs until nothing is left: again if wake-ups came in meanwhile, or if
  // the run did something (there may be more work behind it).
  async function loop() {
    let runs = 0;
    let outcome: RunOutcome = "idle";
    do {
      const wakeups = pending;
      pending = [];
      runs++;
      log(`run ${runs} starting (${describe(wakeups)})`);
      try {
        outcome = await options.run(wakeups);
      } catch (error) {
        log(`run failed: ${error instanceof Error ? error.message : String(error)}`);
        outcome = "failed";
      }
      log(`run ${runs} ended: ${outcome}`);
    } while ((pending.length > 0 || outcome === "worked") && runs < maxRunsInARow);
    if (pending.length > 0 || outcome === "worked") {
      log(`stopping after ${runs} runs in a row; the next wake-up or the schedule carries on`);
      pending = [];
    }
  }

  function wake(wakeup: Wakeup) {
    pending.push(wakeup);
    if (current) {
      log(`${describe([wakeup])} arrived during a run; checking again when it ends`);
      return;
    }
    start();
  }

  function start() {
    current = loop().finally(() => {
      current = null;
    });
  }

  async function fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/" && request.method === "GET") {
      return Response.json({ running: current !== null });
    }
    if (url.pathname !== "/wake") return new Response("not found", { status: 404 });
    if (request.method !== "POST") return new Response("POST only", { status: 405 });

    const body = await request.text();
    if (!verify(options.secret, body, request.headers.get(SIGNATURE_HEADER))) {
      log("refused a wake-up with a missing or wrong signature");
      return new Response("bad signature", { status: 401 });
    }
    let wakeup: Wakeup;
    try {
      wakeup = JSON.parse(body);
    } catch {
      return new Response("not JSON", { status: 400 });
    }
    wake(wakeup);
    return new Response(null, { status: 202 });
  }

  /**
   * The schedule's check, without a wake-up: a run picks up whatever Forge's
   * wake-ups missed (e.g. while the Mac slept). Skipped during a run, which
   * checks for more work when it ends anyway.
   */
  function checkIn() {
    if (!current) start();
  }

  return {
    fetch,
    checkIn,
    /** Resolves once no run is going. */
    idle: async () => {
      while (current) await current;
    },
  };
}

/**
 * Claude Code's MCP settings for the runner's runs: only Forge, with the
 * agent's own key, so they act as that agent and never as the chat session
 * in the same checkout (FOR-43).
 */
export function mcpConfig(forgeUrl: string, key: string) {
  return {
    mcpServers: {
      forge: {
        type: "http",
        url: new URL("/mcp", forgeUrl).toString(),
        headers: { Authorization: `Bearer ${key}` },
      },
    },
  };
}

export function describe(wakeups: Wakeup[]): string {
  if (wakeups.length === 0) return "no wake-up";
  return wakeups.map((w) => `${w.event} on ${w.issue}`).join(", ");
}
