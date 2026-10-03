import type { SuggestionsState } from "@commands/suggestionStates.ts";
import type { FocusRing } from "@input/focusRings.ts";
import {
  KeyAction,
  matchesAction,
  parseKeySpec,
  type ResolvedKeyBindings,
} from "@input/keybindings.ts";
import { isConversationWidget, type LayoutConfig } from "@layout/trees.ts";
import { useKeyboard } from "@opentui/solid";

export function useAppShortcuts(options: {
  suggestions: SuggestionsState;
  keyBindings: () => ResolvedKeyBindings;
  layout: () => LayoutConfig;
  focusRing: FocusRing;
  startNewThread: () => void;
  refresh: () => void;
}): void {
  const { suggestions, focusRing } = options;

  useKeyboard((event) => {
    if (suggestions.suggestions().length > 0) {
      if (event.name === "down") {
        event.preventDefault();
        suggestions.move(1);
        return;
      }
      if (event.name === "up") {
        event.preventDefault();
        suggestions.move(-1);
        return;
      }
      const submitsDraft =
        event.name === "return" && suggestions.completesDraft();
      if (
        event.name === "tab" ||
        (event.name === "return" && !event.shift && !submitsDraft)
      ) {
        if (suggestions.accept()) {
          event.preventDefault();
          return;
        }
      }
      if (event.name === "escape") {
        event.preventDefault();
        suggestions.dismiss();
        return;
      }
    }

    const bindings = options.keyBindings();
    const handle = (action: KeyAction, run: () => void): boolean => {
      if (!matchesAction(event, bindings[action])) return false;
      event.preventDefault();
      run();
      return true;
    };
    for (const [id, widget] of Object.entries(options.layout().widgets)) {
      if (
        isConversationWidget(widget) &&
        widget.focusKey &&
        matchesAction(event, [parseKeySpec(widget.focusKey)])
      ) {
        event.preventDefault();
        focusRing.focus(id);
        return;
      }
    }
    handle(KeyAction.FocusPrev, focusRing.prev) ||
      handle(KeyAction.FocusNext, focusRing.next) ||
      handle(KeyAction.NewSession, options.startNewThread) ||
      handle(KeyAction.Refresh, options.refresh) ||
      handle(KeyAction.Quit, () => process.exit(0));
  });
}
