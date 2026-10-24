/** One request at a time; refreshes during a request share its result. */
export function poll(
  load: (signal: AbortSignal) => Promise<void>,
  visible: () => boolean,
  every = 15_000,
) {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: Promise<void> | undefined;
  const schedule = () => {
    if (controller.signal.aborted) return;
    timer = setTimeout(() => {
      if (visible()) void refresh();
      else schedule();
    }, every);
  };
  const refresh = (): Promise<void> => {
    if (controller.signal.aborted) return Promise.resolve();
    if (pending) return pending;
    clearTimeout(timer);
    pending = Promise.resolve()
      .then(() => load(controller.signal))
      .finally(() => {
        pending = undefined;
        schedule();
      });
    return pending;
  };
  return {
    refresh,
    stop() {
      controller.abort();
      clearTimeout(timer);
    },
  };
}
