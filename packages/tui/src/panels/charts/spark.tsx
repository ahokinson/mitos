import { useAppState } from "@app/states.tsx";
import {
  chartTitle,
  formatSeriesValue,
  miningSeries,
  seriesMax,
} from "@panels/charts/chartData.ts";
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

export function SparkChart(props: ChartPanelProps) {
  const { mining } = useAppState();
  const theme = useTheme();
  const framed = () => theme.chrome === Chrome.Framed;
  const rows = createMemo(() =>
    miningSeries(mining(), props.metric).slice(-(props.top ?? DEFAULT_TOP)),
  );
  const max = () => seriesMax(rows()) || 1;
  const inner = createMemo(() => innerWidth(props.paneWidth, framed()));

  /** The range line's labels come first; the max and total tails join
   * only while they still fit the pane. */
  function rangeLine(): string {
    const series = rows();
    if (series.length === 0) return "";
    let line = `${series[0].label} … ${series.at(-1)?.label}`;
    for (const tail of [
      ` · max ${formatSeriesValue(props.metric, max())}`,
      ` · total ${formatSeriesValue(
        props.metric,
        series.reduce((sum, point) => sum + point.value, 0),
      )}`,
    ]) {
      if (inner() === undefined || line.length + tail.length <= inner())
        line += tail;
    }
    return line;
  }

  return (
    <ChartFrame
      paneStyle={props.paneStyle}
      title={chartTitle(props.metric)}
      rows={(() => {
        const series = rows();
        if (series.length === 0) return [];
        const points = inner() === undefined ? series : series.slice(-inner());
        return [
          <text>
            <For each={points}>
              {(point) => (
                <span
                  style={{
                    fg:
                      magnitudeTone(theme, point.value / max()) ?? theme.accent,
                  }}
                >
                  {SPARK_STEPS[sparkStep(point.value, max())]}
                </span>
              )}
            </For>
          </text>,
          <text fg={theme.textDim}>{rangeLine()}</text>,
        ];
      })()}
    />
  );
}
