import { expect, test } from "bun:test";
import { poll } from "./poll";

test("slow refreshes share a request and stopping cancels it", async () => {
  let calls = 0;
  let signal: AbortSignal | undefined;
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  const polling = poll(
    async (current) => {
      calls++;
      signal = current;
      await pending;
    },
    () => true,
    5,
  );
  const first = polling.refresh();
  await Promise.resolve();
  expect(polling.refresh()).toBe(first);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(calls).toBe(1);
  polling.stop();
  expect(signal?.aborted).toBe(true);
  release();
  await first;
  await new Promise((resolve) => setTimeout(resolve, 15));
  expect(calls).toBe(1);
});

test("refresh resumes after completion and skips hidden pages", async () => {
  let calls = 0;
  let visible = false;
  const polling = poll(
    async () => {
      calls++;
    },
    () => visible,
    5,
  );
  await polling.refresh();
  try {
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls).toBe(1);
    visible = true;
    const until = Date.now() + 1000;
    while (calls < 2 && Date.now() < until) await new Promise((resolve) => setTimeout(resolve, 2));
    expect(calls).toBeGreaterThan(1);
  } finally {
    polling.stop();
  }
});
