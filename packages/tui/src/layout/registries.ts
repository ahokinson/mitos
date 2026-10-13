import type { ChartKind, PaneStyle } from "@layout/configs.ts";
import { PanelKind } from "@layout/configs.ts";
import { BarsChart } from "@panels/charts/bars.tsx";
import { HeatmapChart } from "@panels/charts/heatmap.tsx";
import { SessionsChart } from "@panels/charts/sessions.tsx";
import { SparkChart } from "@panels/charts/spark.tsx";
import { UsageChart } from "@panels/charts/usage.tsx";
import { ThreadViewPanel } from "@panels/conversation/threadViews.tsx";
import type { Component } from "solid-js";

export type PanelProps = {
  paneStyle?: PaneStyle;
  focusId: string;
};

export type ChartPanelRendererProps = {
  paneStyle?: PaneStyle;
  /** The pane's measured width, so a chart can size its columns to fit;
   * `undefined` leaves the chart on its default widths. */
  paneWidth?: number;
  metric?: string;
  top?: number;
};

/** The built-in, interactive panel registry. File-backed templates and
 * charts are rendered separately because they deliberately do not accept
 * focus props. */
export const PANEL_REGISTRY: Record<PanelKind, Component<PanelProps>> = {
  [PanelKind.Conversation]: ThreadViewPanel,
};

/** The read-only chart registry, one component per chart kind. */
export const CHART_REGISTRY: Record<
  ChartKind,
  Component<ChartPanelRendererProps>
> = {
  bars: BarsChart,
  spark: SparkChart,
  heatmap: HeatmapChart,
  sessions: SessionsChart,
  usage: UsageChart,
};
