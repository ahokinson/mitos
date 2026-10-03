import { useTimeline } from "@opentui/solid";
import { createSignal } from "solid-js";

import { useTheme } from "@theme/providers.tsx";

const spinnerFrames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

export function WorkingIndicator(props: { awaitingAnswer: boolean }) {
  const theme = useTheme();
  const [frame, setFrame] = createSignal(0);
  const state = { value: 0 };
  const timeline = useTimeline({ duration: 800, loop: true });
  timeline.add(state, {
    value: 1,
    duration: 800,
    loop: true,
    onUpdate: (animation) => setFrame(Math.floor(animation.progress * spinnerFrames.length) % spinnerFrames.length),
  });

  return (
    <box width="100%" flexDirection="row" marginTop={1} paddingX={1} gap={1}>
      <text width={1} flexShrink={0} fg={theme.accent}>
        {spinnerFrames[frame()]}
      </text>
      <text fg={props.awaitingAnswer ? theme.warning : theme.textMuted}>
        {props.awaitingAnswer ? "waiting for your answer" : "working"}
      </text>
    </box>
  );
}
