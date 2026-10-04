import { latestUsage, pendingRequests } from "@database/views.ts";
import type { Thread } from "@session/threads.ts";
import { createMemo, createSignal } from "solid-js";

/** Usage and pending requests of the selected thread; `bump` re-reads both. */
export function createSelectionUsageState(selected: () => Thread | null) {
  const [tick, setTick] = createSignal(0);

  const usage = createMemo(() => {
    tick();
    const thread = selected();
    return thread ? latestUsage(thread.id, thread.active_harness) : null;
  });

  const requests = createMemo(
    () => {
      tick();
      const thread = selected();
      return thread ? pendingRequests(thread.id) : [];
    },
    [],
    {
      equals: (a, b) =>
        a.length === b.length &&
        a.every((request, index) => request.id === b[index]?.id),
    },
  );

  return { usage, requests, bump: () => setTick((value) => value + 1) };
}
