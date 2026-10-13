import { type MiningReport, mineAll } from "@database/views.ts";
import { createMemo, createSignal } from "solid-js";

/** Every harness's merged mining payload, shared by every chart panel;
 * `bump` re-reads it, which the history watchers do on each write. */
export function createMiningState() {
  const [tick, setTick] = createSignal(0);

  const mining = createMemo<MiningReport | null>(() => {
    tick();
    return mineAll();
  });

  return { mining, bump: () => setTick((value) => value + 1) };
}
