import type { ScrollBoxRenderable } from "@opentui/core";
import { For, createEffect } from "solid-js";

import type { CommandSuggestion } from "@commands/suggestions.ts";
import { BOLD, Chrome } from "@theme/themes.ts";
import { useTheme } from "@theme/providers.tsx";

// Bounds how many rows render at once; the active row is scrolled into view.
const MAX_SUGGESTION_ROWS = 6;
const MAX_HINT_LENGTH = 48;

// A plain `<box>` never clips, so every row is held to one line.
function truncateToOneLine(text: string): string {
  const flattened = text.replace(/\s+/g, " ").trim();
  return flattened.length > MAX_HINT_LENGTH ? `${flattened.slice(0, MAX_HINT_LENGTH - 1).trimEnd()}…` : flattened;
}

/** Renders the suggestions above the composer; key handling is in
 * `app/shortcuts.ts`. */
export function CommandSuggestions(props: { items: readonly CommandSuggestion[]; cursor: number }) {
  const theme = useTheme();
  let scrollbox: ScrollBoxRenderable | undefined;

  createEffect(() => {
    scrollbox?.scrollChildIntoView(`suggestion-${props.cursor}`);
  });

  return (
    <box flexShrink={0} flexDirection="column" paddingX={1} marginTop={1} border={theme.chrome === Chrome.Framed} borderColor={theme.textDim}>
      <scrollbox
        ref={(element) => {
          scrollbox = element;
        }}
        height={MAX_SUGGESTION_ROWS}
        width="100%"
        contentOptions={{ width: "100%" }}
      >
        <For each={props.items}>
          {(item, index) => {
            const active = () => index() === props.cursor;
            return (
              <box id={`suggestion-${index()}`} width="100%" flexDirection="row" gap={1} backgroundColor={active() ? theme.backgroundSelection : undefined}>
                <text flexShrink={0} wrapMode="none" fg={active() ? theme.accent : theme.text} attributes={active() ? BOLD : 0}>
                  {truncateToOneLine(item.label)}
                </text>
                <text flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} wrapMode="none" fg={theme.textDim}>
                  {truncateToOneLine(item.hint)}
                </text>
              </box>
            );
          }}
        </For>
      </scrollbox>
    </box>
  );
}
