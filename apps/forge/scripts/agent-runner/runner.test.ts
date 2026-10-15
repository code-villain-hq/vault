import { describe, expect, test } from "bun:test";
import { createRunner, mcpConfig, type RunOutcome, sign, type Wakeup } from "./runner";

const secret = "forge_runner_test";

function wakeup(issue: string, event = "assigned"): Wakeup {
  return { id: crypto.randomUUID(), event, issue, workspace_id: "w", agent_id: "a" };
}

function post(body: string, signature: string | null = sign(secret, body), path = "/wake") {
  const headers = new Headers({ "content-type": "application/json" });
  if (signature) headers.set("x-forge-signature", signature);
  return new Request(`http://localhost${path}`, { method: "POST", headers, body });
}

/** A stand-in for Claude Code: each run waits until the test ends it. */
function fakeRuns() {
  const started: Wakeup[][] = [];
  const finishers: ((outcome: RunOutcome) => void)[] = [];
  return {
    started,
    run: (wakeups: Wakeup[]) => {
      started.push(wakeups);
      return new Promise<RunOutcome>((resolve) => finishers.push(resolve));
    },
    /** Ends the oldest unfinished run, then lets the runner react. */
    finish: async (outcome: RunOutcome) => {
      finishers.shift()?.(outcome);
      await Bun.sleep(0);
    },
  };
}

describe("agent runner", () => {
  test("signs like Forge: sha256= and the hex HMAC-SHA256 of the body", () => {
    // Worked out with `openssl dgst -sha256 -hmac forge_runner_test`.
    const body = '{"id":"1","event":"assigned","issue":"FOR-1","workspace_id":"w","agent_id":"a"}';
    expect(sign(secret, body)).toBe(
      "sha256=bb6718d679f6d4ba40486a6b0971a446c749f0f3ba9bf6130637c6d04738d4c6",
    );
  });

  test("a signed wake-up is answered at once and starts a run", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    const response = await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    expect(response.status).toBe(202);
    expect(runs.started.map((w) => w.map((x) => x.issue))).toEqual([["FOR-1"]]);
    await runs.finish("idle");
    await runner.idle();
  });

  test("a missing or wrong signature is refused and starts nothing", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    const body = JSON.stringify(wakeup("FOR-1"));
    expect((await runner.fetch(post(body, null))).status).toBe(401);
    expect((await runner.fetch(post(body, sign("another secret", body)))).status).toBe(401);
    expect((await runner.fetch(post(`${body} `, sign(secret, body)))).status).toBe(401);
    expect(runs.started).toEqual([]);
  });

  test("only POST /wake wakes it", async () => {
    const runner = createRunner({ secret, run: fakeRuns().run });
    expect((await runner.fetch(post("{}", sign(secret, "{}"), "/other"))).status).toBe(404);
    expect((await runner.fetch(new Request("http://localhost/wake"))).status).toBe(405);
    const status = await runner.fetch(new Request("http://localhost/"));
    expect(await status.json()).toEqual({ running: false });
  });

  test("wake-ups during a run don't start a second one; one more run follows", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    await runner.fetch(post(JSON.stringify(wakeup("FOR-2", "comment"))));
    await runner.fetch(post(JSON.stringify(wakeup("FOR-3", "mention"))));
    expect(runs.started).toHaveLength(1);
    expect(await (await runner.fetch(new Request("http://localhost/"))).json()).toEqual({
      running: true,
    });

    await runs.finish("idle");
    expect(runs.started.map((w) => w.map((x) => x.issue))).toEqual([["FOR-1"], ["FOR-2", "FOR-3"]]);
    await runs.finish("idle");
    await runner.idle();
    expect(runs.started).toHaveLength(2);
  });

  test("after a run that did something, it checks for more work until there's none", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    await runs.finish("worked");
    await runs.finish("worked");
    await runs.finish("idle");
    await runner.idle();
    expect(runs.started.map((w) => w.length)).toEqual([1, 0, 0]);
  });

  test("a failed run isn't retried until the next wake-up", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    await runs.finish("failed");
    await runner.idle();
    expect(runs.started).toHaveLength(1);

    await runner.fetch(post(JSON.stringify(wakeup("FOR-1", "comment"))));
    expect(runs.started).toHaveLength(2);
    await runs.finish("idle");
    await runner.idle();
  });

  test("it stops after a few runs in a row", async () => {
    let count = 0;
    const runner = createRunner({
      secret,
      maxRunsInARow: 3,
      run: async () => {
        count++;
        return "worked";
      },
    });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    await runner.idle();
    expect(count).toBe(3);
  });

  test("a run that throws counts as failed and frees the runner", async () => {
    let count = 0;
    const runner = createRunner({
      secret,
      run: async () => {
        count++;
        throw new Error("claude not found");
      },
    });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    await runner.idle();
    await runner.fetch(post(JSON.stringify(wakeup("FOR-2"))));
    await runner.idle();
    expect(count).toBe(2);
  });

  test("the schedule's check starts a run without a wake-up", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    runner.checkIn();
    expect(runs.started).toEqual([[]]);
    await runs.finish("idle");
    await runner.idle();
  });

  test("the schedule's check is skipped during a run", async () => {
    const runs = fakeRuns();
    const runner = createRunner({ secret, run: runs.run });
    await runner.fetch(post(JSON.stringify(wakeup("FOR-1"))));
    runner.checkIn();
    expect(runs.started).toHaveLength(1);
    await runs.finish("idle");
    await runner.idle();
    expect(runs.started).toHaveLength(1);
  });

  test("runs get only Forge as an MCP server, signed in with the agent's key", () => {
    expect(mcpConfig("http://localhost:7380", "forge_abc")).toEqual({
      mcpServers: {
        forge: {
          type: "http",
          url: "http://localhost:7380/mcp",
          headers: { Authorization: "Bearer forge_abc" },
        },
      },
    });
  });
});
