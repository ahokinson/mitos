import { expect, test } from "bun:test";
import { AppStateProvider } from "@app/states.tsx";
import type { MiningReport, UsageSnapshot } from "@database/views.ts";
import { createFocusRing } from "@input/focusRings.ts";
import type { PaneStyle } from "@layout/configs.ts";
import type { RGBA } from "@opentui/core";
import { testRender } from "@opentui/solid";
import { BarsChart } from "@panels/charts/bars.tsx";
import { HeatmapChart } from "@panels/charts/heatmap.tsx";
import { SessionsChart } from "@panels/charts/sessions.tsx";
import { SparkChart } from "@panels/charts/spark.tsx";
import { UsageChart } from "@panels/charts/usage.tsx";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";

const REPORT: MiningReport = {
  totals: {
    sessions: 51,
    messages: 2810,
    messages_user: 228,
    messages_assistant: 2582,
    tool_calls: 3525,
    patches: 431,
    files_patched: 696,
    forks: 7,
    cost_usd: 48.87,
    tool_seconds: 10228.4,
    tokens_input: 13007947,
    tokens_output: 827278,
    tokens_reasoning: 5528811,
    tokens_cache_read: 228224618,
    tokens_cache_write: 26558836,
  },
  harnesses: [
    {
      label: "opencode",
      sessions: 51,
      messages: 2810,
      tool_calls: 3525,
      patches: 431,
      cost_usd: 48.87,
      tokens_input: 13007947,
      tokens_output: 827278,
      tokens_cache_read: 228224618,
    },
  ],
  cost_by_day: [
    { label: "2026-10-08", value: 1.5 },
    { label: "2026-10-09", value: 2.5 },
    { label: "2026-10-10", value: 8.0 },
  ],
  cost_by_project: [
    { label: "/home/anders/Local/Developer/mitos", value: 16.35 },
    { label: "/etc/nixos", value: 11.77 },
    { label: "/home/anders/Local/Developer/corkboard", value: 8.2 },
  ],
  cost_by_model: [{ label: "glm-5.3", value: 28.65 }],
  cost_by_agent: [{ label: "build", value: 30.01 }],
  cost_by_harness: [{ label: "opencode", value: 48.87 }],
  sessions_by_day: [{ label: "2026-10-10", value: 4 }],
  sessions_by_agent: [{ label: "build", value: 34 }],
  sessions_by_model: [{ label: "glm-5.3", value: 17 }],
  tokens_by_day: [{ label: "2026-10-10", value: 641366 }],
  tool_usage: [
    { label: "bash", value: 1332 },
    { label: "read", value: 958 },
    { label: "edit", value: 643 },
  ],
  tool_time: [
    { label: "bash", value: 10228.4 },
    { label: "read", value: 1666.0 },
  ],
  activity_by_day: [
    { label: "2026-10-09", values: [0, 0, 0, 4, 8, 2, 0, 0] },
    { label: "2026-10-10", values: [0, 3, 9, 7, 0, 0, 0, 0] },
  ],
  sessions: [
    {
      id: "ses_1",
      harness: "opencode",
      title: "Multi-OS Nix config for NixOS, Asahi Fedora, macOS",
      directory: "/etc/nixos",
      cost_usd: 10.5,
      tokens_input: 1000,
      tokens_output: 100,
      tokens_reasoning: 40,
      tokens_cache_read: 20000,
      model: "glm-5.3",
      agent: "build",
      time_created: 1785120254102,
      time_updated: 1785130254102,
    },
    {
      id: "ses_2",
      harness: "claude",
      title: "Coverage for TypeScript",
      directory: "/w",
      cost_usd: 0.56,
      tokens_input: 500,
      tokens_output: 50,
      tokens_reasoning: 0,
      tokens_cache_read: 5000,
      model: "claude-sonnet-5",
      agent: null,
      time_created: 1785120254102,
      time_updated: 1785121254102,
    },
  ],
};

const exampleUsage: UsageSnapshot = {
  harness: "opencode",
  observed_at: "",
  input_tokens: 123456,
  output_tokens: 7890,
  cached_input_tokens: 4321,
  cost_usd: 2.1016,
  context_used_tokens: 64000,
  context_limit_tokens: 128000,
  model: "glm-5.3",
  turns: 4,
  plan_five_hour_percent: 74.2,
  plan_five_hour_resets_at: new Date(Date.now() + 45 * 60_000).toISOString(),
  plan_week_percent: 61,
  plan_week_resets_at: new Date(Date.now() + 40 * 3_600_000).toISOString(),
  plan_month_percent: 62,
  plan_month_resets_at: new Date(Date.now() + 30 * 86_400_000).toISOString(),
};

function fakeAppState(
  mining: MiningReport | null,
  usage: UsageSnapshot | null = null,
) {
  return {
    list: { threads: () => [], loading: () => false, refresh: () => {} },
    selected: () => null,
    currentHarness: () => "opencode",
    events: { events: () => [], sync: () => {} },
    feedback: { lines: () => [], push: () => {} },
    onSend: () => {},
    onDraftChange: () => {},
    registerSuggestionApplier: () => {},
    suggestions: () => [],
    suggestionCursor: () => 0,
    working: () => false,
    usage: () => usage,
    mining: () => mining,
    focusRing: createFocusRing(() => []),
  } as unknown as Parameters<typeof AppStateProvider>[0]["value"];
}

async function render(
  chart: () => unknown,
  mining: MiningReport | null = REPORT,
  usage: UsageSnapshot | null = null,
) {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <AppStateProvider value={fakeAppState(mining, usage)}>
          {chart()}
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 60, height: 20 },
  );
  await setup.renderOnce();
  return setup;
}

async function renderFrame(
  chart: () => unknown,
  mining: MiningReport | null = REPORT,
  usage: UsageSnapshot | null = null,
) {
  const setup = await render(chart, mining, usage);
  const frame = setup.captureCharFrame();
  setup.renderer.destroy();
  return frame;
}

/** Non-whitespace spans in order, with fg as `#rrggbb`. */
async function renderSpans(
  chart: () => unknown,
  mining: MiningReport | null = REPORT,
  usage: UsageSnapshot | null = null,
) {
  const setup = await render(chart, mining, usage);
  const spans = setup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0)
    .map((span) => ({ text: span.text, fg: fgHex(span.fg) }));
  setup.renderer.destroy();
  return spans;
}

function fgHex(fg: RGBA): string {
  const [r, g, b] = fg.toInts();
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

const paneStyle: PaneStyle = { flexGrow: 1 };

test("a bars chart draws each series point as a filled bar with its value", async () => {
  const frame = await renderFrame(() => (
    <BarsChart paneStyle={paneStyle} metric="cost_by_project" />
  ));
  expect(frame).toContain("cost by project");
  expect(frame).toContain("~/Local/Developer/mitos");
  expect(frame).toContain("/etc/nixos");
  expect(frame).toContain("█");
  expect(frame).toContain("$16.35");
  expect(frame).toContain("$11.77");
});

test("a bars chart truncates to its top and clips long labels", async () => {
  const frame = await renderFrame(() => (
    <BarsChart paneStyle={paneStyle} metric="cost_by_project" top={2} />
  ));
  expect(frame).toContain("/etc/nixos");
  expect(frame).not.toContain("~/Local/Developer/corkboard");
  const full = await renderFrame(() => (
    <BarsChart paneStyle={paneStyle} metric="cost_by_project" top={3} />
  ));
  expect(full).toContain("…");
});

test("a spark chart draws the ramp with span and totals", async () => {
  const frame = await renderFrame(() => (
    <SparkChart paneStyle={paneStyle} metric="cost_by_day" />
  ));
  expect(frame).toContain("cost by day");
  expect(frame).toContain("▂▃█");
  expect(frame).toContain("2026-10-08 … 2026-10-10");
  expect(frame).toContain("max $8.00");
  expect(frame).toContain("total $12.00");
});

test("a narrow pane sizes the bar columns so nothing wraps", async () => {
  const frame = await renderFrame(() => (
    <BarsChart
      paneStyle={paneStyle}
      paneWidth={38}
      metric="cost_by_project"
      top={1}
    />
  ));
  const row = frame.split("\n").find((line) => line.includes("$16.35"));
  expect(row).toBeDefined();
  expect(row).toMatch(/….*█.*\$16\.35/);
});

test("a narrow session row keeps the harness column and drops the tokens", async () => {
  const frame = await renderFrame(() => (
    <SessionsChart paneStyle={paneStyle} paneWidth={38} top={1} />
  ));
  const row = frame.split("\n").find((line) => line.includes("Multi-OS"));
  expect(row).toBeDefined();
  expect(row).toContain("opencode");
  expect(row).toContain("$10.50");
  expect(row).not.toContain("21K");
});

test("a zero-valued point draws an empty bar, not a token block", async () => {
  const withZero = {
    ...REPORT,
    cost_by_project: [{ label: "ghost", value: 0 }, ...REPORT.cost_by_project],
  };
  const frame = await renderFrame(
    () => <BarsChart paneStyle={paneStyle} metric="cost_by_project" top={2} />,
    withZero,
  );
  const row = frame.split("\n").find((line) => line.includes("ghost"));
  expect(row).toBeDefined();
  expect(row).toContain("$0.00");
  expect(row).not.toContain("█");
});

test("a heatmap chart draws one ramped row per day", async () => {
  const frame = await renderFrame(() => (
    <HeatmapChart paneStyle={paneStyle} metric="activity_by_day" />
  ));
  expect(frame).toContain("activity by hour");
  expect(frame).toContain("10-09");
  expect(frame).toContain("10-10");
  expect(frame).toContain("·");
});

test("a sessions chart lists recent sessions with cost and tokens", async () => {
  const frame = await renderFrame(() => (
    <SessionsChart paneStyle={paneStyle} />
  ));
  expect(frame).toContain("recent sessions");
  expect(frame).toContain("Multi-OS Nix config for");
  expect(frame).toContain("opencode");
  expect(frame).toContain("$10.50");
  expect(frame).toContain("21K");
});

test("charts without a report say so instead of drawing an empty pane", async () => {
  for (const chart of [
    () => <BarsChart paneStyle={paneStyle} metric="cost_by_project" />,
    () => <SparkChart paneStyle={paneStyle} metric="cost_by_day" />,
    () => <HeatmapChart paneStyle={paneStyle} metric="activity_by_day" />,
    () => <SessionsChart paneStyle={paneStyle} />,
  ]) {
    const frame = await renderFrame(chart, null);
    expect(frame).toContain("no history mined yet");
  }
});

test("a usage chart bars the live windows with the time each ends", async () => {
  const frame = await renderFrame(
    () => <UsageChart paneStyle={paneStyle} />,
    null,
    exampleUsage,
  );
  expect(frame).toContain("live usage · opencode");
  expect(frame).toContain("rate 5h");
  expect(frame).toContain("74%");
  expect(frame).toMatch(/resets \d{1,2}:\d{2}(am|pm)/);
  expect(frame).toContain("45m left");
  expect(frame).toContain("rate wk");
  expect(frame).toContain("61%");
  expect(frame).toContain("context");
  expect(frame).toContain("50%");
  expect(frame).toContain("64K of 128K tokens");
});

test("a usage chart waits for the first turn without a snapshot", async () => {
  const frame = await renderFrame(() => <UsageChart paneStyle={paneStyle} />);
  expect(frame).toContain("waiting for the first turn");
});

test("a usage chart names the windows a harness never reports", async () => {
  const quiet: UsageSnapshot = {
    ...exampleUsage,
    plan_five_hour_percent: null,
    plan_five_hour_resets_at: null,
    plan_week_percent: null,
    plan_week_resets_at: null,
    plan_month_percent: null,
    plan_month_resets_at: null,
    context_used_tokens: null,
    context_limit_tokens: null,
  };
  const frame = await renderFrame(
    () => <UsageChart paneStyle={paneStyle} />,
    null,
    quiet,
  );
  expect(frame).toContain("windows not reported by this harness");
});

test("a hot rate window paints its bar and percent in the warning tone", async () => {
  const spans = await renderSpans(
    () => <UsageChart paneStyle={paneStyle} />,
    null,
    exampleUsage,
  );
  expect(spans.find((span) => span.text.includes("74%"))?.fg).toBe("#f9e2af");
  expect(spans.find((span) => span.text.includes("61%"))?.fg).toBe("#f9e2af");
  expect(spans.find((span) => span.text.includes("░"))?.fg).toBe("#6c7086");
});

test("a session row paints its harness in accent and its cost by magnitude", async () => {
  const spans = await renderSpans(() => (
    <SessionsChart paneStyle={paneStyle} />
  ));
  expect(spans.find((span) => span.text.trim() === "opencode")?.fg).toBe(
    "#89b4fa",
  );
  expect(spans.find((span) => span.text.trim() === "claude")?.fg).toBe(
    "#89b4fa",
  );
  expect(spans.find((span) => span.text.includes("$10.50"))?.fg).toBe(
    "#f38ba8",
  );
  expect(spans.find((span) => span.text.includes("$0.56"))?.fg).toBe("#ffffff");
});

test("a bars chart paints each value in its bar's severity tone", async () => {
  const spans = await renderSpans(() => (
    <BarsChart paneStyle={paneStyle} metric="cost_by_project" />
  ));
  expect(spans.find((span) => span.text.includes("$16.35"))?.fg).toBe(
    "#f38ba8",
  );
  expect(spans.find((span) => span.text.includes("$11.77"))?.fg).toBe(
    "#f9e2af",
  );
});

test("a spark chart paints every step, escalating with heat", async () => {
  const spans = await renderSpans(() => (
    <SparkChart paneStyle={paneStyle} metric="cost_by_day" />
  ));
  expect(spans.find((span) => span.text === "▂▃")?.fg).toBe("#89b4fa");
  expect(spans.find((span) => span.text === "█")?.fg).toBe("#f38ba8");
});
