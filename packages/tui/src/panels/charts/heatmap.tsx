import { useAppState } from "@app/states.tsx";
import {
  DEFAULT_TOP,
  innerWidth,
  magnitudeTone,
  SPARK_STEPS,
  sparkStep,
} from "@panels/charts/columns.ts";
import { ChartFrame, type ChartPanelProps } from "@panels/charts/frame.tsx";
import { useTheme } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import { createMemo, For } from "solid-js";

export function HeatmapChart(props: ChartPanelProps) {
  const { mining } = useAppState();
  const theme = useTheme();
  const framed = () => theme.chrome === Chrome.Framed;
  const days = createMemo(() =>
    (mining()?.activity_by_day ?? []).slice(-(props.top ?? DEFAULT_TOP)),
  );
  const max = () =>
    Math.max(
      1,
      ...days().flatMap((day) => day.values.map((value) => value || 0)),
    );
  const labelPad = createMemo(() => {
    const inner = innerWidth(props.paneWidth, framed());
    return inner === undefined ? 10 : Math.max(6, Math.min(10, inner - 24));
  });

  function cell(value: number): { char: string; fg: string | undefined } {
    if (value === 0) return { char: "·", fg: theme.textDim };
    return {
      char: SPARK_STEPS[sparkStep(value, max())],
      fg: magnitudeTone(theme, value / max()) ?? theme.accent,
    };
  }

  return (
    <ChartFrame
      paneStyle={props.paneStyle}
      title="activity by hour (0-23 UTC)"
      rows={days().map((day) => (
        <text>
          <span>{day.label.slice(5).padEnd(labelPad())}</span>
          <For each={day.values}>
            {(value) => {
              const c = cell(value);
              return <span style={{ fg: c.fg }}>{c.char}</span>;
            }}
          </For>
        </text>
      ))}
    />
  );
}
