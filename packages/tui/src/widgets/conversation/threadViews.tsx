import { For, Show } from "solid-js";

import type { PaneStyle } from "@layout/trees.ts";
import { accumulateAssistantDeltas, isDisplayEvent } from "@session/events.ts";
import { FeedItemKind, type FeedItem, mergeFeed } from "@commands/feedbackLines.ts";
import { ThreadMode } from "@session/threads.ts";
import { useAppState } from "@app/states.tsx";
import { Chrome } from "@theme/themes.ts";
import { useTheme } from "@theme/providers.tsx";
import { CommandSuggestions } from "@widgets/conversation/suggestionLists.tsx";
import { Composer } from "@widgets/conversation/composers.tsx";
import { EventRow } from "@widgets/conversation/eventRows.tsx";
import { FeedbackLineRow } from "@widgets/conversation/feedbackRows.tsx";
import { WorkingIndicator } from "@widgets/conversation/workingIndicators.tsx";

function EmptyConversation() {
  const theme = useTheme();
  return (
    <box flexGrow={1} flexShrink={1} minHeight={0} justifyContent="center" alignItems="center" paddingX={1}>
      <text fg={theme.textMuted}>Start a conversation.</text>
    </box>
  );
}

export function ThreadViewWidget(props: { paneStyle?: PaneStyle; focusId: string }) {
  const { selected, events, feedback, onSend, loadHistory, onDraftChange, registerSuggestionApplier, suggestions, suggestionCursor, working, pendingRequests, focusRing } =
    useAppState();
  const theme = useTheme();
  const realEvents = () => accumulateAssistantDeltas(events.events().filter(isDisplayEvent));
  const feed = (): FeedItem[] => mergeFeed(realEvents(), feedback.lines(), selected()?.id ?? null);
  const isFocused = () => focusRing.current() === props.focusId;

  return (
    <box
      {...props.paneStyle}
      flexDirection="column"
      border={theme.chrome === Chrome.Framed}
      borderColor={theme.textDim}
      focusable
      focused={isFocused()}
      focusedBorderColor={theme.accent}
    >
      <Show when={selected() !== null || feed().length > 0} fallback={<EmptyConversation />}>
        <scrollbox width="100%" flexGrow={1} flexShrink={1} minHeight={0} stickyScroll stickyStart="bottom" contentOptions={{ width: "100%" }}>
          <For each={feed()}>{(item) => (item.kind === FeedItemKind.Event ? <EventRow event={item.event} /> : <FeedbackLineRow feedback={item.feedback} />)}</For>
          <Show when={working()}>
            <WorkingIndicator awaitingAnswer={pendingRequests().length > 0} />
          </Show>
        </scrollbox>
      </Show>
      <Show when={suggestions().length > 0}>
        <CommandSuggestions items={suggestions()} cursor={suggestionCursor()} />
      </Show>
      <Composer
        focused={isFocused()}
        hasSession={selected() !== null}
        working={working()}
        mode={selected()?.mode ?? ThreadMode.Build}
        loadHistory={loadHistory}
        onSubmit={onSend}
        onDraftChange={onDraftChange}
        registerSuggestionApplier={registerSuggestionApplier}
      />
    </box>
  );
}
