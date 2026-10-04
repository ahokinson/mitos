import { expect, test } from "bun:test";
import { createSuggestionsState } from "@commands/suggestionStates.ts";
import { createRoot } from "solid-js";

function withState<R>(
  run: (state: ReturnType<typeof createSuggestionsState>) => Promise<R> | R,
): Promise<R> {
  return createRoot(async (dispose) => {
    const state = createSuggestionsState({
      threads: () => [],
      pendingDeleteId: () => null,
    });
    await Promise.resolve();
    try {
      return await run(state);
    } finally {
      dispose();
    }
  });
}

test("plain text offers no suggestions", () =>
  withState((state) => {
    state.setDraft("hello");
    expect(state.suggestions()).toEqual([]);
    expect(state.cursor()).toBe(0);
    expect(state.accept()).toBe(false);
    state.move(1);
    expect(state.cursor()).toBe(0);
  }));

test("a command prefix suggests matching commands", () =>
  withState((state) => {
    state.setDraft("/ar");
    expect(state.suggestions().map((item) => item.label)).toEqual(["/archive"]);
    expect(state.completesDraft()).toBe(false);
  }));

test("the cursor moves within bounds", () =>
  withState((state) => {
    state.setDraft("/");
    const count = state.suggestions().length;
    expect(count).toBeGreaterThan(2);
    state.move(-1);
    expect(state.cursor()).toBe(0);
    state.move(2);
    expect(state.cursor()).toBe(2);
    state.move(count + 5);
    expect(state.cursor()).toBe(count - 1);
  }));

test("accepting applies the highlighted suggestion", () =>
  withState((state) => {
    const applied: string[] = [];
    state.setDraft("/ar");
    expect(state.accept()).toBe(false);
    state.registerApplier((text) => applied.push(text));
    expect(state.accept()).toBe(true);
    expect(applied).toEqual(["/archive "]);
  }));

test("dismissing hides suggestions until the draft changes", () =>
  withState(async (state) => {
    state.setDraft("/ar");
    state.dismiss();
    expect(state.suggestions()).toEqual([]);
    state.setDraft("/arc");
    await Promise.resolve();
    expect(state.suggestions()).toHaveLength(1);
  }));

test("a fully spelled command completes the draft", () =>
  withState((state) => {
    state.setDraft("/archive");
    expect(state.completesDraft()).toBe(true);
  }));

test("changing the draft resets the cursor", () =>
  withState(async (state) => {
    state.setDraft("/");
    state.move(3);
    expect(state.cursor()).toBe(3);
    state.setDraft("/h");
    await Promise.resolve();
    expect(state.cursor()).toBe(0);
  }));
