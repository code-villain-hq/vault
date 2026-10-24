// The window's way between the backend's setup and the workspace
// (src/lib/workspace-sync.ts), against the sync kit's FakeServer. The
// backend is a stand-in that holds an outbox and the last snapshot, as
// `read_setup_writes` / `ack_setup_writes` and `apply_setup` do. Bun's module mocks reach every
// test file, so each keeps the rest of its module. Outside src/ so tsc, which has no
// bun types, does not read it.
import { beforeEach, describe, expect, mock, test } from "bun:test";
import * as kit from "@code-villain/sync-client";
import type { Operation } from "@layer/types";
import { FakeServer } from "../../../kit/sync/client/src/fake-server";
import * as real from "../src/lib/api";

let server: FakeServer;
let backend: { outbox: Operation[]; snapshot: Record<string, { id: string }[]> | null };
let written: () => void = () => {};
let kept: string[] = [];
const identities = new WeakMap<Operation, string>();

mock.module("@code-villain/sync-client", () => ({
  ...kit,
  IndexedDbStore: kit.MemoryStore,
  HttpTransport: class {
    constructor() {
      // biome-ignore lint/correctness/noConstructorReturn: the fake's transport stands in
      return server.transport();
    }
  },
}));
mock.module("@tauri-apps/api/event", () => ({
  listen: async (_event: string, handler: () => void) => {
    written = handler;
    return () => {};
  },
}));
mock.module("../src/lib/api", () => ({
  ...real,
  api: {
    ...real.api,
    workspacePlace: async () => ({ origin: "http://127.0.0.1:7685", token: "t", user_id: "u", workspace_id: "w" }),
    readSetupWrites: async () => backend.outbox.map((operation) => {
      const id = identities.get(operation) ?? kit.uuidv7();
      identities.set(operation, id);
      return { id, operation };
    }),
    ackSetupWrites: async (ids: string[]) => { backend.outbox = backend.outbox.filter((op) => !ids.includes(identities.get(op) ?? "")); },
    addMessage: async (_kind: string, _level: string, title: string) => void kept.push(title),
    applySetup: async (snapshot: Record<string, { id: string }[]>) => {
      if (backend.outbox.length) return { applied: false, setup: false };
      const setup = JSON.stringify(snapshot) !== JSON.stringify(backend.snapshot);
      backend.snapshot = snapshot;
      return { applied: true, setup };
    },
  },
}));
const { refusals, startWorkspaceSync } = await import("../src/lib/workspace-sync");

// The fake server is Forge's: a create needs a title.
const note = (id: string, text: string): Operation =>
  ({ action: "create", model: "Note", id, data: { title: "note", repo: "github.com/acme/api", text } }) as Operation;

const settled = () => new Promise((resolve) => setTimeout(resolve, 20));

beforeEach(() => {
  server = new FakeServer();
  backend = { outbox: [], snapshot: null };
  kept = [];
});

describe("the shared setup between this Mac and its workspace", () => {
  test("what the backend queued reaches the workspace, and comes back as the snapshot", async () => {
    backend.outbox.push(note("n1", "Tests need bun run db first."));
    const stop = await startWorkspaceSync({ onSetup: () => {}, onRefused: () => {} });
    await settled();

    expect(server.records.get("Note")?.get("n1")?.text).toBe("Tests need bun run db first.");
    expect(backend.snapshot?.notes.map((n) => n.id)).toEqual(["n1"]);
    stop();
  });

  test("a change from elsewhere comes in and the setup is read again", async () => {
    server.apply({ id: kit.uuidv7(), operations: [note("n1", "First.")] });
    let reads = 0;
    const stop = await startWorkspaceSync({ onSetup: () => reads++, onRefused: () => {} });
    await settled();
    const first = reads;

    server.change("Note", "n1", { text: "Second." });
    await settled();
    expect(reads).toBe(first + 1);
    expect((backend.snapshot?.notes[0] as unknown as { text: string }).text).toBe("Second.");
    stop();
  });

  test("a write the backend raises later is taken when it says so", async () => {
    const stop = await startWorkspaceSync({ onSetup: () => {}, onRefused: () => {} });
    await settled();

    backend.outbox.push(note("n2", "From an MCP note."));
    written();
    await settled();
    expect(server.records.get("Note")?.has("n2")).toBe(true);
    expect(backend.outbox).toEqual([]);
    stop();
  });

  test("a change the workspace refuses is said by the sync status, and kept", async () => {
    const refused: string[] = [];
    const said = (e: Event) => refused.push((e as CustomEvent<string>).detail);
    refusals.addEventListener("refused", said);
    backend.outbox.push({ action: "create", model: "Note", id: "n3", data: { text: "No title." } } as Operation);
    const stop = await startWorkspaceSync({ onSetup: () => {}, onRefused: (m) => refused.push(`here: ${m}`) });
    await settled();
    refusals.removeEventListener("refused", said);

    expect(refused).toEqual(["title can't be empty"]);
    expect(kept).toEqual(["The workspace did not take a change: title can't be empty"]);
    expect(server.records.get("Note")?.has("n3") ?? false).toBe(false);
    stop();
  });
});
