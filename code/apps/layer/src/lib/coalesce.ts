export interface RefreshSlot {
  current: Promise<void> | null;
  next?: Promise<void> | null;
}

/**
 * Share overlapping polls; explicit refreshes share one successor instead.
 * They asked after something changed, which the in-flight read cannot see.
 */
export async function coalesce(
  slot: RefreshSlot,
  poll: boolean,
  work: () => Promise<void>,
): Promise<void> {
  if (slot.current) {
    if (poll) return slot.current;
    if (!slot.next) {
      slot.next = slot.current
        .catch(() => {})
        .then(() => {
          slot.next = null;
          return coalesce(slot, false, work);
        });
    }
    return slot.next;
  }
  const run = work();
  slot.current = run;
  try {
    await run;
  } finally {
    if (slot.current === run) slot.current = null;
  }
}
