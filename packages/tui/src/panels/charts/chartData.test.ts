import { describe, expect, test } from "bun:test";

import type { MiningReport } from "@database/views.ts";
import {
  chartTitle,
  compact,
  formatSeriesValue,
  isCostMetric,
  miningSeries,
  seriesMax,
  usd,
} from "@panels/charts/chartData.ts";

const REPORT: MiningReport = {
  totals: {
    sessions: 0,
    messages: 0,
    messages_user: 0,
    messages_assistant: 0,
    tool_calls: 0,
    patches: 0,
    files_patched: 0,
    forks: 0,
    cost_usd: 0,
    tool_seconds: 0,
    tokens_input: 0,
    tokens_output: 0,
    tokens_reasoning: 0,
    tokens_cache_read: 0,
    tokens_cache_write: 0,
  },
  harnesses: [],
  cost_by_day: [{ label: "2026-10-10", value: 8.0 }],
  cost_by_project: [{ label: "/etc/nixos", value: 11.77 }],
  cost_by_model: [{ label: "glm-5.3", value: 28.65 }],
  cost_by_agent: [{ label: "build", value: 6.5 }],
  cost_by_harness: [{ label: "claude", value: 7.25 }],
  sessions_by_day: [{ label: "2026-10-10", value: 3 }],
  sessions_by_agent: [{ label: "explore", value: 2 }],
  sessions_by_model: [{ label: "glm-5.3", value: 1 }],
  tokens_by_day: [{ label: "2026-10-10", value: 641366 }],
  tool_usage: [{ label: "bash", value: 1332 }],
  tool_time: [{ label: "bash", value: 42.5 }],
  activity_by_day: [],
  sessions: [],
};

describe("miningSeries", () => {
  test("every metric name points at its own series", () => {
    const metrics = [
      "cost_by_day",
      "cost_by_project",
      "cost_by_model",
      "cost_by_agent",
      "cost_by_harness",
      "sessions_by_day",
      "sessions_by_agent",
      "sessions_by_model",
      "tokens_by_day",
      "tool_usage",
      "tool_time",
    ] as const;
    for (const metric of metrics)
      expect(miningSeries(REPORT, metric)).toEqual(REPORT[metric]);
  });

  test("an unknown metric, a missing metric or no report draws nothing", () => {
    expect(miningSeries(REPORT, "no_such_series")).toEqual([]);
    expect(miningSeries(REPORT, "activity_by_day")).toEqual([]);
    expect(miningSeries(REPORT, undefined)).toEqual([]);
    expect(miningSeries(null, "cost_by_day")).toEqual([]);
  });
});

test("seriesMax reads the largest value and zero on an empty series", () => {
  expect(seriesMax(REPORT.cost_by_day)).toBe(8.0);
  expect(seriesMax([])).toBe(0);
});

test("a cost metric formats as money, every other as a count", () => {
  expect(isCostMetric("cost_by_project")).toBe(true);
  expect(isCostMetric("tool_usage")).toBe(false);
  expect(isCostMetric(undefined)).toBe(false);
  expect(formatSeriesValue("cost_by_project", 11.77)).toBe("$11.77");
  expect(formatSeriesValue("tool_usage", 1332)).toBe("1.3K");
  expect(usd(48.8682245)).toBe("$48.87");
  expect(usd(0.0011634)).toBe("$0.0012");
  expect(compact(228224618)).toBe("228M");
});

test("a chart title spells the metric out", () => {
  expect(chartTitle("cost_by_project")).toBe("cost by project");
  expect(chartTitle(undefined)).toBe("recent sessions");
});
