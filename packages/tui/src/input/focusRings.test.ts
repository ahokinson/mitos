import { expect, test } from "bun:test";
import { createFocusRing } from "@input/focusRings.ts";
import { createRoot } from "solid-js";

function withRing<R>(
  ids: readonly string[],
  run: (ring: ReturnType<typeof createFocusRing>) => R,
): R {
  let result!: R;
  const dispose = createRoot((dispose) => {
    result = run(createFocusRing(() => ids));
    return dispose;
  });
  dispose();
  return result;
}

test("focus() jumps to a known id and ignores an unknown one", () => {
  withRing(["0.0", "0.1", "0.2"], (ring) => {
    ring.focus("0.2");
    expect(ring.current()).toBe("0.2");
    ring.focus("nope");
    expect(ring.current()).toBe("0.2");
  });
});

test("current() is null for an empty ring", () => {
  withRing([], (ring) => expect(ring.current()).toBeNull());
});

test("current() starts at the first id", () => {
  withRing(["0.0", "0.1"], (ring) => expect(ring.current()).toBe("0.0"));
});

test("next() advances through ids in order", () => {
  withRing(["0.0", "0.1", "0.2"], (ring) => {
    ring.next();
    expect(ring.current()).toBe("0.1");
    ring.next();
    expect(ring.current()).toBe("0.2");
  });
});

test("next() wraps around past the last id", () => {
  withRing(["0.0", "0.1"], (ring) => {
    ring.next();
    ring.next();
    expect(ring.current()).toBe("0.0");
  });
});

test("prev() wraps around before the first id", () => {
  withRing(["0.0", "0.1", "0.2"], (ring) => {
    ring.prev();
    expect(ring.current()).toBe("0.2");
  });
});

test("current() stays valid when the id list shrinks out from under the index", () => {
  withRing(["0.0", "0.1", "0.2"], (ring) => {
    ring.next();
    ring.next();
    expect(ring.current()).toBe("0.2");
  });
});
