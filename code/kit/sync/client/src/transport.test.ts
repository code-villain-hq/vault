import { afterEach, beforeEach, expect, test } from "bun:test";
import { HttpTransport, RejectedError } from "./transport";

// A WebSocket that does what a test tells it to.
class FakeSocket {
  static last: FakeSocket;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor() {
    FakeSocket.last = this;
  }
  close() {
    this.closed = true;
  }
  say(message: object) {
    this.onmessage?.({ data: JSON.stringify(message) });
  }
}

const real = globalThis.WebSocket;
beforeEach(() => {
  globalThis.WebSocket = FakeSocket as unknown as typeof WebSocket;
});
afterEach(() => {
  globalThis.WebSocket = real;
});

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function connect({ open = true } = {}) {
  const transport = new HttpTransport({
    workspaceId: "w",
    baseUrl: "http://localhost",
    openFor: 30,
    silentFor: 30,
  });
  const heard = { closed: 0, messages: 0 };
  const stop = transport.connect(0, "e", {
    open: () => {},
    message: () => heard.messages++,
    close: () => heard.closed++,
  });
  const socket = FakeSocket.last;
  if (open) socket.onopen?.();
  return { socket, heard, stop };
}

test("a feed that said it's alive and then says nothing counts as gone", async () => {
  const { socket, heard } = connect();
  socket.say({ type: "alive" });
  await wait(15);
  socket.say({ type: "delta", entries: [], sync_id: 1 });
  await wait(15);
  expect(heard.closed).toBe(0);
  await wait(60);
  expect(heard.closed).toBe(1);
  expect(socket.closed).toBe(true);
});

test("a feed that doesn't open in time counts as gone", async () => {
  const { socket, heard } = connect({ open: false });
  await wait(60);
  expect(heard.closed).toBe(1);
  expect(socket.closed).toBe(true);
});

test("a server that never says it's alive is waited for, as before", async () => {
  const { socket, heard } = connect();
  socket.say({ type: "delta", entries: [], sync_id: 1 });
  await wait(80);
  expect(heard.closed).toBe(0);
  expect(socket.closed).toBe(false);
});

test("closing the feed stops waiting for it", async () => {
  const { socket, heard, stop } = connect();
  socket.say({ type: "alive" });
  stop();
  await wait(80);
  expect(heard.closed).toBe(0);
  expect(socket.closed).toBe(true);
});

function answering(status: number, body: object) {
  const realFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(JSON.stringify(body), { status })) as unknown as typeof fetch;
  return () => {
    globalThis.fetch = realFetch;
  };
}

async function sendOnce() {
  const transport = new HttpTransport({ workspaceId: "w", baseUrl: "http://localhost" });
  return transport.send({ id: "t", operations: [] }).then(
    () => "sent",
    (error: Error) => (error instanceof RejectedError ? `rejected ${error.code}` : "held"),
  );
}

test("someone outside the workspace keeps their changes to send later", async () => {
  const restore = answering(404, { code: "not_found", message: "workspace not found" });
  try {
    expect(await sendOnce()).toBe("held");
  } finally {
    restore();
  }
});

test("a change the server refuses is still refused", async () => {
  for (const [status, body] of [
    [403, { code: "forbidden", message: "only workspace admins can do that" }],
    [404, { code: "not_found", message: "Task not found" }],
    [422, { code: "invalid", message: "title can't be empty" }],
  ] as const) {
    const restore = answering(status, body);
    try {
      expect(await sendOnce()).toBe(`rejected ${body.code}`);
    } finally {
      restore();
    }
  }
});
