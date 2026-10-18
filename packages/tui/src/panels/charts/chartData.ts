import type { MiningReport, MiningSeriesPoint } from "@database/views.ts";

/** The mined series a `metric` name points at; an unknown name draws nothing. */
export function miningSeries(
  report: MiningReport | null,
  metric: string | undefined,
): MiningSeriesPoint[] {
  if (!report || !metric) return [];
  switch (metric) {
    case "cost_by_day":
      return report.cost_by_day;
    case "cost_by_project":
      return report.cost_by_project;
    case "cost_by_model":
      return report.cost_by_model;
    case "cost_by_agent":
      return report.cost_by_agent;
    case "cost_by_harness":
      return report.cost_by_harness;
    case "sessions_by_day":
      return report.sessions_by_day;
    case "sessions_by_agent":
      return report.sessions_by_agent;
    case "sessions_by_model":
      return report.sessions_by_model;
    case "tokens_by_day":
      return report.tokens_by_day;
    case "tool_usage":
      return report.tool_usage;
    case "tool_time":
      return report.tool_time;
    default:
      return [];
  }
}

/** A `cost_*` metric reads as money; every other series reads as a count. */
export function isCostMetric(metric: string | undefined): boolean {
  return metric?.startsWith("cost") ?? false;
}

export function seriesMax(series: MiningSeriesPoint[]): number {
  return series.reduce((max, point) => Math.max(max, point.value), 0);
}

export function formatSeriesValue(
  metric: string | undefined,
  value: number,
): string {
  return isCostMetric(metric) ? usd(value) : compact(value);
}

/** Sub-cent costs keep four decimals so a cheap series doesn't read as $0.00. */
export function usd(value: number): string {
  return `$${value.toFixed(value > 0 && value < 0.01 ? 4 : 2)}`;
}

const COMPACT = new Intl.NumberFormat("en-US", { notation: "compact" });

export function compact(value: number): string {
  return COMPACT.format(value);
}

/** A chart row's title, as config would name it. */
export function chartTitle(metric: string | undefined): string {
  return metric === undefined ? "recent sessions" : metric.replaceAll("_", " ");
}
