import type { Operation } from "@layer/types";

export type SetupWrite = { id: string; operation: Operation };

/** The backend keeps its queue until the client has durably accepted it. */
export async function transferSetupWrites(
  read: () => Promise<SetupWrite[]>,
  ack: (ids: string[]) => Promise<void>,
  client: { transact: (ops: Operation[], id: string) => string; saved: () => Promise<void> },
): Promise<number> {
  const writes = await read();
  if (!writes.length) return 0;
  for (const write of writes) client.transact([write.operation], write.id);
  await client.saved();
  await ack(writes.map((write) => write.id));
  return writes.length;
}
