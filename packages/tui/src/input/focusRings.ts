import { createMemo, createSignal } from "solid-js";

export type FocusRing = {
  current: () => string | null;
  focus: (id: string) => void;
  next: () => void;
  prev: () => void;
};

/** Wraps around at both ends, so Tab cycling loops. */
export function createFocusRing(ids: () => readonly string[]): FocusRing {
  const [index, setIndex] = createSignal(0);
  const current = createMemo<string | null>(() => {
    const list = ids();
    if (list.length === 0) return null;
    const wrapped = ((index() % list.length) + list.length) % list.length;
    return list[wrapped] ?? null;
  });

  return {
    current,
    focus: (id) => {
      const found = ids().indexOf(id);
      if (found >= 0) setIndex(found);
    },
    next: () => setIndex((value) => value + 1),
    prev: () => setIndex((value) => value - 1),
  };
}
