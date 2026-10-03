import {
  type CommandSuggestion,
  isCompleteSuggestion,
  suggestFor,
} from "@commands/suggestions.ts";
import { detectInstalledHarnesses } from "@harness/harnesses.ts";
import type { Thread } from "@session/threads.ts";
import { createEffect, createMemo, createSignal } from "solid-js";

export type SuggestionsState = {
  setDraft: (text: string) => void;
  suggestions: () => readonly CommandSuggestion[];
  cursor: () => number;
  move: (delta: number) => void;
  accept: () => boolean;
  dismiss: () => void;
  completesDraft: () => boolean;
  registerApplier: (apply: (text: string) => void) => void;
};

export function createSuggestionsState(options: {
  threads: () => readonly Thread[];
  pendingDeleteId: () => string | null;
}): SuggestionsState {
  const [draft, setDraft] = createSignal("");
  const [dismissed, setDismissed] = createSignal(false);
  const [cursorRaw, setCursorRaw] = createSignal(0);
  const all = createMemo<readonly CommandSuggestion[]>(() =>
    suggestFor(draft(), {
      installedHarnesses: detectInstalledHarnesses(),
      threads: options.threads(),
      pendingDeleteId: options.pendingDeleteId(),
    }),
  );
  const suggestions = createMemo<readonly CommandSuggestion[]>(() =>
    dismissed() ? [] : all(),
  );
  const cursor = createMemo(() => {
    const length = suggestions().length;
    return length === 0 ? 0 : Math.min(Math.max(cursorRaw(), 0), length - 1);
  });
  createEffect(() => {
    all();
    setCursorRaw(0);
    setDismissed(false);
  });

  let applier: ((text: string) => void) | null = null;

  return {
    setDraft,
    suggestions,
    cursor,
    move(delta) {
      const length = suggestions().length;
      if (length === 0) return;
      setCursorRaw((value) => Math.min(Math.max(value + delta, 0), length - 1));
    },
    accept() {
      const item = suggestions()[cursor()];
      if (!item || !applier) return false;
      applier(item.insertText);
      return true;
    },
    dismiss: () => setDismissed(true),
    completesDraft: () => isCompleteSuggestion(draft(), suggestions()),
    registerApplier(apply) {
      applier = apply;
    },
  };
}
