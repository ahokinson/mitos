import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { AppStateProvider } from "@app/states.tsx";
import type { MiningReport, UsageSnapshot } from "@database/views.ts";
import { createFocusRing } from "@input/focusRings.ts";
import { TextAlign } from "@layout/configs.ts";
import type { RGBA } from "@opentui/core";
import { testRender } from "@opentui/solid";
import { TemplatePanel } from "@panels/custom/customPanels.tsx";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";

const activeThread: Thread = {
  id: "a1b2c3d4-5678-90ab-cdef-1234567890ab",
  workspace_id: "ws1",
  status: ThreadStatus.Active,
  mode: ThreadMode.Build,
  active_harness: "codex",
  native_session: null,
  last_event_seq: 5,
  created_at: "2026-10-01T08:48:08.520280533+00:00",
  updated_at: "2026-10-01T08:48:17.819925381+00:00",
  opening_message: null,
};

function fakeAppState(
  usage: UsageSnapshot | null = null,
  mining: MiningReport | null = null,
) {
  return {
    list: { threads: () => [], loading: () => false, refresh: () => {} },
    selected: () => activeThread,
    currentHarness: () => "codex",
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
    focusRing: createFocusRing(() => ["stats"]),
  } as unknown as Parameters<typeof AppStateProvider>[0]["value"];
}

async function renderSetup(
  template: string,
  workspaceRoot = "/tmp/workspace",
  usage: UsageSnapshot | null = null,
  themeName = "mocha",
  align?: TextAlign,
  mining: MiningReport | null = null,
  height = 20,
) {
  const previous = process.env.MITOS_WORKSPACE_ROOT;
  process.env.MITOS_WORKSPACE_ROOT = workspaceRoot;
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: themeName })}>
        <AppStateProvider value={fakeAppState(usage, mining)}>
          <TemplatePanel template={template} align={align} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 50, height },
  );
  await setup.renderOnce();
  process.env.MITOS_WORKSPACE_ROOT = previous;
  return setup;
}

async function render(
  template: string,
  workspaceRoot = "/tmp/workspace",
  usage: UsageSnapshot | null = null,
  mining: MiningReport | null = null,
  height = 20,
) {
  const setup = await renderSetup(
    template,
    workspaceRoot,
    usage,
    "mocha",
    undefined,
    mining,
    height,
  );
  const frame = setup.captureCharFrame();
  setup.renderer.destroy();
  return frame;
}

function fgHex(fg: RGBA): string {
  const [r, g, b] = fg.toInts();
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

/** Non-whitespace spans only, in order, with fg as `#rrggbb` — the padding
 * and border chrome around the panel are not this test's concern. */
function visibleSpans(setup: Awaited<ReturnType<typeof renderSetup>>) {
  return setup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0)
    .map((span) => ({ text: span.text, fg: fgHex(span.fg) }));
}

test("a hand-drawn template interpolates every referenced metric", async () => {
  const frame = await render(
    "{{harness}}\n# {{thread.id}}  {{thread.status}}\n· created {{thread.created}}\n⌘ {{workspace}}",
  );
  expect(frame).toContain("codex");
  expect(frame).toContain("a1b2c3d4");
  expect(frame).toContain("active");
  expect(frame).toContain("2026-10-01 08:48:08");
  expect(frame).toContain("/tmp/workspace");
});

const exampleUsage: UsageSnapshot = {
  harness: "opencode",
  observed_at: "2026-10-01T08:48:17Z",
  input_tokens: 123456,
  output_tokens: 7890,
  cached_input_tokens: 4321,
  cost_usd: 2.1016,
  context_used_tokens: 64000,
  context_limit_tokens: 128000,
  model: "glm-5.3",
  turns: 4,
  plan_five_hour_percent: null,
  plan_five_hour_resets_at: null,
  plan_week_percent: null,
  plan_week_resets_at: null,
  plan_month_percent: null,
  plan_month_resets_at: null,
};

const example = (name: string) =>
  readFileSync(
    join(import.meta.dir, `../../../examples/panels/${name}`),
    "utf8",
  );

const pilotExample = example("pilot.hbs");
const accountantExample = example("accountant.hbs");
const optimizerExample = example("optimizer.hbs");
const profilerExample = example("profiler.hbs");
const standupExample = example("standup.hbs");
const minimalistExample = example("minimalist.hbs");

const exampleReport: MiningReport = {
  totals: {
    sessions: 179,
    messages: 28274,
    messages_user: 1986,
    messages_assistant: 26288,
    tool_calls: 18874,
    patches: 552,
    files_patched: 696,
    forks: 7,
    cost_usd: 745.35,
    tool_seconds: 100635,
    tokens_input: 13935684,
    tokens_output: 976932,
    tokens_reasoning: 5528811,
    tokens_cache_read: 268261437,
    tokens_cache_write: 26558836,
  },
  harnesses: [
    {
      label: "opencode",
      sessions: 52,
      messages: 3195,
      tool_calls: 4008,
      patches: 552,
      cost_usd: 64.47,
      tokens_input: 14193925,
      tokens_output: 1050539,
      tokens_cache_read: 278060625,
    },
    {
      label: "claude",
      sessions: 83,
      messages: 23276,
      tool_calls: 11905,
      patches: 0,
      cost_usd: 680.88,
      tokens_input: 18746,
      tokens_output: 8947716,
      tokens_cache_read: 2856700995,
    },
    {
      label: "codex",
      sessions: 44,
      messages: 1803,
      tool_calls: 2961,
      patches: 0,
      cost_usd: 0,
      tokens_input: 184370114,
      tokens_output: 1070792,
      tokens_cache_read: 179346688,
    },
  ],
  cost_by_day: [{ label: "2026-10-10", value: 27.39 }],
  cost_by_project: [
    { label: "/home/anders/Local/Developer/mitos", value: 27.58 },
    { label: "/etc/nixos", value: 11.77 },
  ],
  cost_by_model: [{ label: "claude-sonnet-5", value: 437.14 }],
  cost_by_agent: [{ label: "build", value: 61.31 }],
  cost_by_harness: [
    { label: "claude", value: 680.88 },
    { label: "opencode", value: 64.47 },
    { label: "codex", value: 0 },
  ],
  sessions_by_day: [{ label: "2026-10-10", value: 8 }],
  sessions_by_agent: [{ label: "build", value: 34 }],
  sessions_by_model: [
    { label: "claude-sonnet-5-5", value: 44 },
    { label: "gpt-5.6-terra", value: 42 },
  ],
  tokens_by_day: [{ label: "2026-10-10", value: 92757070 }],
  tool_usage: [
    { label: "bash", value: 1525 },
    { label: "read", value: 1004 },
  ],
  tool_time: [
    { label: "bash", value: 10228.4 },
    { label: "question", value: 8025.7 },
  ],
  activity_by_day: [{ label: "2026-10-10", values: [0, 4, 135, 7, 254] }],
  sessions: [
    {
      id: "ses_1",
      harness: "opencode",
      title: "Map mitos harness internals",
      directory: "/home/anders/Local/Developer/mitos",
      cost_usd: 0.5633,
      tokens_input: 107258,
      tokens_output: 13084,
      tokens_reasoning: 500,
      tokens_cache_read: 1367672,
      model: "glm-5.3",
      agent: "explore",
      time_created: 1791655826858,
      time_updated: 1791656066265,
    },
  ],
  est_windows: {
    tier: "plus",
    burn_5h: 0.06,
    oldest_5h_ms: Date.now() - 3 * 3_600_000,
    burn_week: 0.3,
    burn_month: 0.4,
  },
};

test("the pilot example shows rounded limit percents with reset times and thread tokens", async () => {
  const frame = await render(
    pilotExample,
    "/tmp/workspace",
    {
      ...exampleUsage,
      plan_five_hour_percent: 50.6,
      plan_five_hour_resets_at: new Date(Date.now() + 3_600_000).toISOString(),
      plan_week_percent: 63,
      plan_week_resets_at: new Date(Date.now() + 2 * 86_400_000).toISOString(),
      plan_month_percent: 34,
      plan_month_resets_at: new Date(
        Date.now() + 20 * 86_400_000,
      ).toISOString(),
    },
    null,
    26,
  );
  expect(frame).toContain("rate 5h");
  expect(frame).toContain("51%");
  expect(frame).toContain("63%");
  expect(frame).toMatch(/resets \d{1,2}:\d{2}(am|pm)/);
  expect(frame).toContain("this thread");
  expect(frame).toContain("123,456");
  expect(frame).toContain("$2.10");
  expect(frame).not.toContain("not reported");
});

test("the pilot example hides context when the harness reports no limit", async () => {
  const frame = await render(
    pilotExample,
    "/tmp/workspace",
    {
      ...exampleUsage,
      context_limit_tokens: null,
    },
    exampleReport,
  );
  expect(frame).not.toContain("context");
  expect(frame).toContain("this thread");
  expect(frame).toContain("go plan · est");
  expect(frame).toContain("30%");
  expect(frame).toContain("2h 0m until it eases");
  expect(frame).toContain("60%");
  expect(frame).toContain("40%");
});

test("the optimizer example falls back to estimated windows when the harness reports none", async () => {
  const frame = await render(
    optimizerExample,
    "/tmp/workspace",
    exampleUsage,
    exampleReport,
    44,
  );
  expect(frame).toContain("go plan · est");
  expect(frame).toContain("30%");
  expect(frame).toContain("2h 0m until it eases");
});

test("the pilot example waits for the first turn when there is no usage", async () => {
  const frame = await render(pilotExample);
  expect(frame).toContain("waiting for the first turn");
});

test("the accountant example totals the history and ranks its spend", async () => {
  const frame = await render(
    accountantExample,
    "/tmp/workspace",
    null,
    exampleReport,
    44,
  );
  expect(frame).toContain("sessions    179");
  expect(frame).not.toContain("no history mined");
  expect(frame).toContain("messages     28,274");
  expect(frame).toContain("$745.35");
  expect(frame).toContain("2026-10-10 $27.39");
  expect(frame).toContain("/etc/nixos $11.77");
});

test("the accountant example says so when nothing is mined yet", async () => {
  const frame = await render(accountantExample);
  expect(frame).toContain("no history mined yet");
});

test("the optimizer example leads with live windows and flags a hot one", async () => {
  const frame = await render(
    optimizerExample,
    "/tmp/workspace",
    {
      ...exampleUsage,
      harness: "claude",
      plan_five_hour_percent: 75,
      plan_five_hour_resets_at: new Date(Date.now() + 3_600_000).toISOString(),
      plan_week_percent: 40,
      plan_week_resets_at: new Date(Date.now() + 86_400_000).toISOString(),
      plan_month_percent: 69,
      plan_month_resets_at: new Date(
        Date.now() + 30 * 86_400_000,
      ).toISOString(),
    },
    exampleReport,
    48,
  );
  expect(frame).toContain("right now");
  expect(frame).toContain("claude · glm-5.3");
  expect(frame).toContain("rate 5h");
  expect(frame).toContain("1h 0m left");
  expect(frame).toContain("rate week");
  expect(frame).toContain("left · resets");
  expect(frame).toContain("5h at 75% — plan a switch");
  expect(frame).toContain("context this session");
  expect(frame).toContain("50%");
  expect(frame).toContain("the record · by harness");
  expect(frame).toContain("no cost reported");
  expect(frame).toContain("$680.88");
  expect(frame).toContain("gpt-5.6-terra");
});

test("the optimizer example recommends staying put under a healthy window", async () => {
  const frame = await render(
    optimizerExample,
    "/tmp/workspace",
    {
      ...exampleUsage,
      plan_five_hour_percent: 20,
      plan_five_hour_resets_at: new Date(Date.now() + 3_600_000).toISOString(),
      plan_week_percent: 30,
      plan_week_resets_at: new Date(Date.now() + 86_400_000).toISOString(),
      plan_month_percent: 69,
      plan_month_resets_at: new Date(
        Date.now() + 30 * 86_400_000,
      ).toISOString(),
    },
    exampleReport,
    44,
  );
  expect(frame).toContain("rate 5h");
  expect(frame).toContain("rate week");
  expect(frame).toContain("healthy — stay put");
});

test("the profiler example lists tool calls next to the time they burned", async () => {
  const frame = await render(
    profilerExample,
    "/tmp/workspace",
    null,
    exampleReport,
    26,
  );
  expect(frame).toContain("bash ×1525");
  expect(frame).toContain("bash 2h 50m");
  expect(frame).toContain("the aftermath");
  expect(frame).toContain("files touched 696");
});

test("the standup example narrates recent sessions and the pace of work", async () => {
  const frame = await render(
    standupExample,
    "/tmp/workspace",
    null,
    exampleReport,
    50,
  );
  expect(frame).toContain("opencode — Map mitos h…");
  expect(frame).toContain("$0.56");
  expect(frame).toContain("8 sessions");
  expect(frame).toContain("1,986 yours");
});

test("the minimalist example stays quiet: one line per fact", async () => {
  const frame = await render(
    minimalistExample,
    "/tmp/workspace",
    exampleUsage,
    exampleReport,
    26,
  );
  expect(frame).toContain("codex");
  expect(frame).toContain("a1b2c3d4");
  expect(frame).toContain("50%");
  expect(frame).toContain("spent $2.10 on this thread");
  expect(frame).toContain("179 sessions");
});

async function blockColumn(align: TextAlign | undefined): Promise<number> {
  const setup = await renderSetup(
    "abcd\nab",
    "/tmp/workspace",
    null,
    "mocha",
    align,
  );
  const frame = setup.captureCharFrame();
  setup.renderer.destroy();
  const row = frame.split("\n").find((line) => line.includes("abcd")) ?? "";
  return row.indexOf("abcd");
}

test("align places the whole text block left, centered or right", async () => {
  const left = await blockColumn(undefined);
  const center = await blockColumn(TextAlign.Center);
  const right = await blockColumn(TextAlign.Right);
  expect(await blockColumn(TextAlign.Left)).toBe(left);
  expect(center).toBeGreaterThan(left);
  expect(right).toBeGreaterThan(center);
  expect(right).toBe(50 - left - 4);
});

test("alignment keeps a block's lines together instead of aligning each line", async () => {
  const setup = await renderSetup(
    "abcd\nab",
    "/tmp/workspace",
    null,
    "mocha",
    TextAlign.Right,
  );
  const lines = setup.captureCharFrame().split("\n");
  setup.renderer.destroy();
  const top = lines.find((line) => line.includes("abcd")) ?? "";
  const next = lines[lines.indexOf(top) + 1] ?? "";
  expect(next.indexOf("ab")).toBe(top.indexOf("abcd"));
});

test("static text in the template passes through untouched", async () => {
  const frame = await render("status: {{thread.status}} (custom label)");
  expect(frame).toContain("status: active (custom label)");
});

test("a reference to a metric with no current value interpolates as empty, not literally 'undefined'", async () => {
  const frame = await render("rate: [{{usage.plan_5h}}]");
  expect(frame).toContain("rate: []");
  expect(frame).not.toContain("undefined");
});

test("values are not HTML-escaped — this is terminal text, not markup", async () => {
  const frame = await render("{{workspace}}", "/tmp/a&b<c>");
  expect(frame).toContain("/tmp/a&b<c>");
  expect(frame).not.toContain("&amp;");
});

test("a {{#color}} block renders its text in the resolved theme color", async () => {
  const setup = await renderSetup(
    '{{#color "accent"}}{{harness}}{{/color}} ({{thread.status}})',
  );
  const spans = visibleSpans(setup);
  setup.renderer.destroy();
  expect(spans).toContainEqual({ text: "codex", fg: "#89b4fa" });
  expect(spans.find((s) => s.text.includes("active"))?.fg).toBe("#cdd6f4");
});

test("under the default theme a {{#color}} block is painted with the terminal's palette, not a fixed color", async () => {
  const setup = await renderSetup(
    '{{#color "accent"}}{{harness}}{{/color}} plain',
    "/tmp/workspace",
    null,
    "default",
  );
  const spans = setup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0);
  setup.renderer.destroy();
  const accent = spans.find((span) => span.text === "codex");
  expect(accent?.fg.intent).toBe("indexed");
  expect(accent?.fg.slot).toBe(4);
  const plain = spans.find((span) => span.text.includes("plain"));
  expect(plain?.fg.intent).toBe("default");
});

test("under the default theme the usage bar's severity color comes from the palette", async () => {
  const frameSetup = await renderSetup(
    "{{bar usage.context_percent 10}}",
    "/tmp/workspace",
    { ...exampleUsage, context_used_tokens: 90, context_limit_tokens: 100 },
    "default",
  );
  const spans = frameSetup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0);
  frameSetup.renderer.destroy();
  const filled = spans.find((span) => span.text.includes("█"));
  const empty = spans.find((span) => span.text.includes("░"));
  expect(filled?.fg.slot).toBe(1);
  expect(empty?.fg.slot).toBe(8);
});

test("a {{#gradient}} block colors each character along the interpolation, not the default text color", async () => {
  const setup = await renderSetup(
    '{{#gradient "accent" "err"}}{{thread.id}}{{/gradient}}',
  );
  const spans = visibleSpans(setup);
  setup.renderer.destroy();
  const idSpans = spans.filter((s) => "a1b2c3d4".includes(s.text));
  expect(idSpans.map((s) => s.text).join("")).toBe("a1b2c3d4");
  expect(idSpans[0]?.fg).toBe("#89b4fa");
  expect(idSpans[idSpans.length - 1]?.fg).toBe("#f38ba8");
  const distinctColors = new Set(idSpans.map((s) => s.fg));
  expect(distinctColors.size).toBeGreaterThan(2);
});

test("a template walks a mining series with {{#each}} without escaping it", async () => {
  const report: MiningReport = {
    ...exampleReport,
    totals: {
      ...exampleReport.totals,
      sessions: 2,
      messages: 10,
      cost_usd: 3.25,
    },
    cost_by_day: [],
    cost_by_project: [
      { label: "/home/anders/.dotfiles", value: 8.2 },
      { label: "/etc/nixos & friends", value: 11.77 },
    ],
    cost_by_model: [],
    cost_by_agent: [],
    cost_by_harness: [],
    sessions_by_day: [],
    sessions_by_agent: [],
    sessions_by_model: [],
    tokens_by_day: [],
    tool_usage: [],
    tool_time: [],
    activity_by_day: [],
    sessions: [],
  };
  const frame = await render(
    "{{#each mining.projects}}{{this.label}}: {{this.value}}\n{{/each}}total {{mining.cost}}",
    "/tmp/workspace",
    null,
    report,
  );
  expect(frame).toContain("/home/anders/.dotfiles: 8.2");
  expect(frame).toContain("/etc/nixos & friends: 11.77");
  expect(frame).toContain("total $3.25");
  expect(frame).not.toContain("&amp;");
});

test("a template without a report renders its static text alone", async () => {
  const frame = await render(
    "{{#each mining.projects}}{{this.label}}\n{{/each}}no projects yet",
  );
  expect(frame).toContain("no projects yet");
  expect(frame).not.toContain("undefined");
});
