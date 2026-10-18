import type { PaneStyle } from "@layout/configs.ts";
import { useTheme } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import { Show } from "solid-js";

export type ChartPanelProps = {
  paneStyle?: PaneStyle;
  /** The pane's measured width, so the chart sizes its columns to fit;
   * `undefined` leaves the default widths. */
  paneWidth?: number;
  metric?: string;
  top?: number;
};

/** A titled, framed column of chart rows; an empty series shows one muted
 * line instead of an empty pane. */
export function ChartFrame(props: {
  paneStyle?: PaneStyle;
  title: string;
  rows: unknown[];
}) {
  const theme = useTheme();
  return (
    <box
      {...props.paneStyle}
      flexShrink={0}
      flexDirection="column"
      paddingX={1}
      border={theme.chrome === Chrome.Framed}
      borderColor={theme.textDim}
    >
      <text fg={theme.accent}>{props.title}</text>
      <Show
        when={props.rows.length > 0}
        fallback={<text fg={theme.textMuted}>no history mined yet</text>}
      >
        {props.rows}
      </Show>
    </box>
  );
}
