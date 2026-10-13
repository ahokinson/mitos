import { useAppState } from "@app/states.tsx";
import { compact, usd } from "@panels/charts/chartData.ts";
import {
  clipLabel,
  DAY_COL,
  DEFAULT_TOP,
  HARNESS_COL,
  innerWidth,
  magnitudeTone,
  sessionColumns,
} from "@panels/charts/columns.ts";
import { ChartFrame, type ChartPanelProps } from "@panels/charts/frame.tsx";
import { useTheme } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import { createMemo } from "solid-js";

export function SessionsChart(props: ChartPanelProps) {
  const { mining } = useAppState();
  const theme = useTheme();
  const framed = () => theme.chrome === Chrome.Framed;
  const rows = createMemo(() =>
    (mining()?.sessions ?? []).slice(0, props.top ?? DEFAULT_TOP),
  );
  const columns = createMemo(() =>
    sessionColumns(innerWidth(props.paneWidth, framed())),
  );
  const maxCost = () =>
    rows().reduce((max, session) => Math.max(max, session.cost_usd), 0) || 1;

  return (
    <ChartFrame
      paneStyle={props.paneStyle}
      title="recent sessions"
      rows={(() => {
        const sessions = rows();
        if (sessions.length === 0) return [];
        return [
          <text fg={theme.textDim}>
            {"day".padEnd(DAY_COL)}
            {"title".padEnd(columns().title)}{" "}
            {columns().harness ? "harness".padEnd(HARNESS_COL) : ""}
            cost
          </text>,
          ...sessions.map((session) => {
            const day = new Date(session.time_updated)
              .toISOString()
              .slice(5, 10);
            const tokens =
              session.tokens_input +
              session.tokens_output +
              session.tokens_cache_read;
            return (
              <text>
                <span>{day.padEnd(DAY_COL)}</span>
                <span>
                  {clipLabel(
                    session.title.split("\n")[0] || session.id,
                    columns().title,
                  ).padEnd(columns().title)}
                </span>
                <span> </span>
                <span style={{ fg: theme.accent }}>
                  {columns().harness ? session.harness.padEnd(HARNESS_COL) : ""}
                </span>
                <span
                  style={{
                    fg: magnitudeTone(theme, session.cost_usd / maxCost()),
                  }}
                >
                  {" "}
                  {usd(session.cost_usd)}
                </span>
                {columns().tokens ? (
                  <span style={{ fg: theme.textDim }}> {compact(tokens)}</span>
                ) : undefined}
              </text>
            );
          }),
        ];
      })()}
    />
  );
}
