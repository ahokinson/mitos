import { useAppState } from "@app/states.tsx";
import {
  chartTitle,
  formatSeriesValue,
  miningSeries,
  seriesMax,
} from "@panels/charts/chartData.ts";
import {
  barColumns,
  clipLabel,
  DEFAULT_TOP,
  innerWidth,
  magnitudeTone,
} from "@panels/charts/columns.ts";
import { ChartFrame, type ChartPanelProps } from "@panels/charts/frame.tsx";
import { useTheme } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import { createMemo } from "solid-js";

export function BarsChart(props: ChartPanelProps) {
  const { mining } = useAppState();
  const theme = useTheme();
  const framed = () => theme.chrome === Chrome.Framed;
  const rows = createMemo(() =>
    miningSeries(mining(), props.metric).slice(0, props.top ?? DEFAULT_TOP),
  );
  const max = () => seriesMax(rows()) || 1;
  const columns = createMemo(() =>
    barColumns(innerWidth(props.paneWidth, framed())),
  );

  return (
    <ChartFrame
      paneStyle={props.paneStyle}
      title={chartTitle(props.metric)}
      rows={rows().map((point) => {
        const ratio = point.value / max();
        const filled =
          point.value <= 0 ? 0 : Math.max(1, Math.round(ratio * columns().bar));
        return (
          <text>
            <span>
              {clipLabel(point.label, columns().label).padEnd(columns().label)}
            </span>
            <span style={{ fg: magnitudeTone(theme, ratio) }}>
              {"█".repeat(filled)}
              {"░".repeat(Math.max(0, columns().bar - filled))}
            </span>
            <span style={{ fg: magnitudeTone(theme, ratio) }}>
              {" "}
              {formatSeriesValue(props.metric, point.value)}
            </span>
          </text>
        );
      })}
    />
  );
}
