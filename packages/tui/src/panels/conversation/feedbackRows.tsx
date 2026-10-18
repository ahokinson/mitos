import { type FeedbackLine, FeedbackTone } from "@commands/feedbackLines.ts";
import { useTheme } from "@theme/providers.tsx";
import { For } from "solid-js";

export function FeedbackLineRow(props: { feedback: FeedbackLine }) {
  const theme = useTheme();
  const color = () =>
    props.feedback.tone === FeedbackTone.Error ? theme.err : theme.textDim;
  return (
    <box width="100%" flexDirection="column" marginTop={1} paddingX={1}>
      <For each={props.feedback.lines}>
        {(line) => (
          <text flexShrink={0} width="100%" wrapMode="word" fg={color()}>
            {line}
          </text>
        )}
      </For>
    </box>
  );
}
