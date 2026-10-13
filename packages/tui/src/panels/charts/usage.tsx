import { useAppState } from "@app/states.tsx";
import { compact } from "@panels/charts/chartData.ts";
import {
  fitTail,
  innerWidth,
  magnitudeTone,
  percentOf,
} from "@panels/charts/columns.ts";
import { ChartFrame, type ChartPanelProps } from "@panels/charts/frame.tsx";
import {
  METRICS,
  type MetricContext,
  type MetricValue,
} from "@panels/custom/metrics.ts";
import { useTheme } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import { createMemo } from "solid-js";

/** The live rate windows and context of the active harness's account, as
 * severity bars with the time each window ends. Takes no metric. */
export function UsageChart(props: ChartPanelProps) {
  const { usage, currentHarness } = useAppState();
  const theme = useTheme();
  const framed = () => theme.chrome === Chrome.Framed;
  const inner = createMemo(() => innerWidth(props.paneWidth, framed()));

  function metric(key: string): MetricValue | null {
    const ctx: MetricContext = {
      selected: null,
      currentHarness: currentHarness(),
      usage: usage(),
      mining: null,
      workspaceRoot: process.env.MITOS_WORKSPACE_ROOT ?? null,
    };
    return METRICS[key]?.read(ctx) ?? null;
  }

  /** A labeled severity bar; the tail says when the window ends. */
  function windowRow(
    label: string,
    percentKey: string,
    resetsKey: string,
    remainingKey: string,
  ) {
    const percent = metric(percentKey);
    const pct = percentOf(percent);
    if (pct === null) return null;
    const resets = metric(resetsKey)?.value;
    const left = metric(remainingKey)?.value;
    const tail = fitTail(
      [
        ...(resets ? [`resets ${resets}`] : []),
        ...(left ? [`${left} left`] : []),
      ],
      inner(),
      7 + 1 + 4 + 1 + 6,
    );
    const bar =
      inner() === undefined
        ? 20
        : Math.max(6, Math.min(20, inner() - 13 - tail.length));
    const tone = magnitudeTone(theme, pct / 100);
    const filled = Math.round((pct / 100) * bar);
    return (
      <text>
        <span>{label.padEnd(7)}</span>
        <span style={{ fg: tone }}>{"█".repeat(filled)}</span>
        <span style={{ fg: theme.textDim }}>
          {"░".repeat(Math.max(0, bar - filled))}
        </span>
        <span style={{ fg: tone }}> {percent.value}</span>
        {tail.length > 0 ? (
          <span style={{ fg: theme.textDim }}> {tail}</span>
        ) : undefined}
      </text>
    );
  }

  function contextRow() {
    const percent = metric("usage.context_percent");
    const pct = percentOf(percent);
    const snapshot = usage();
    if (pct === null || !snapshot) return null;
    const tail = fitTail(
      [
        `${compact(snapshot.context_used_tokens ?? 0)} of ${compact(snapshot.context_limit_tokens ?? 0)} tokens`,
      ],
      inner(),
      7 + 1 + 4 + 1 + 6,
    );
    const bar =
      inner() === undefined
        ? 20
        : Math.max(6, Math.min(20, inner() - 13 - tail.length));
    const tone = magnitudeTone(theme, pct / 100);
    const filled = Math.round((pct / 100) * bar);
    return (
      <text>
        <span>{"context".padEnd(7)}</span>
        <span style={{ fg: tone }}>{"█".repeat(filled)}</span>
        <span style={{ fg: theme.textDim }}>
          {"░".repeat(Math.max(0, bar - filled))}
        </span>
        <span style={{ fg: tone }}> {percent.value}</span>
        {tail.length > 0 ? (
          <span style={{ fg: theme.textDim }}> {tail}</span>
        ) : undefined}
      </text>
    );
  }

  const harness = () => usage()?.harness ?? currentHarness() ?? "none";
  const source = () => {
    const plan = usage()?.plan_label;
    return plan ? `${plan} via ${harness()}` : harness();
  };
  const rows = createMemo(() => {
    const snapshot = usage();
    if (!snapshot) {
      return [
        <text fg={theme.textMuted}>
          waiting for the first turn to report usage…
        </text>,
      ];
    }
    const rows = [
      windowRow(
        "rate 5h",
        "usage.plan_5h",
        "usage.plan_5h_resets",
        "usage.plan_5h_remaining",
      ),
      windowRow(
        "rate wk",
        "usage.plan_week",
        "usage.plan_week_resets",
        "usage.plan_week_remaining",
      ),
      contextRow(),
    ].filter((row) => row !== null);
    if (rows.length === 0) {
      return [
        <text fg={theme.textDim}>windows not reported by this harness</text>,
      ];
    }
    return rows;
  });

  return (
    <ChartFrame
      paneStyle={props.paneStyle}
      title={`live usage · ${source()}`}
      rows={rows()}
    />
  );
}
