import { expect, test } from "bun:test";
import type { MiningReport } from "@database/views.ts";
import {
  buildMetricContext,
  METRICS,
  type MetricContext,
  shortenHome,
} from "@panels/custom/metrics.ts";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";
import { Tone } from "@theme/themes.ts";

function thread(overrides: Partial<Thread> & { id: string }): Thread {
  return {
    workspace_id: "ws1",
    status: ThreadStatus.Active,
    mode: ThreadMode.Build,
    active_harness: "codex",
    native_session: null,
    last_event_seq: 5,
    created_at: "2026-10-01T08:48:08.520280533+00:00",
    updated_at: "2026-10-01T08:48:17.819925381+00:00",
    opening_message: null,
    ...overrides,
  };
}

const EMPTY_CTX: MetricContext = {
  selected: null,
  currentHarness: null,
  usage: null,
  mining: null,
  workspaceRoot: null,
};

test("harness falls back to the draft currentHarness when no thread is selected", () => {
  expect(METRICS.harness?.read(EMPTY_CTX)).toEqual({
    value: "none",
    tone: Tone.Dim,
  });
  expect(
    METRICS.harness?.read({ ...EMPTY_CTX, currentHarness: "claude" }),
  ).toEqual({ value: "claude", tone: Tone.Accent });
});

test("harness prefers the selected thread's real harness over the draft", () => {
  const ctx = {
    ...EMPTY_CTX,
    selected: thread({ id: "t1", active_harness: "codex" }),
    currentHarness: "claude",
  };
  expect(METRICS.harness?.read(ctx)).toEqual({
    value: "codex",
    tone: Tone.Accent,
  });
});

test("thread.id, thread.status, thread.created are null with no thread selected", () => {
  expect(METRICS["thread.id"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["thread.status"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["thread.created"]?.read(EMPTY_CTX)).toBeNull();
});

test("thread.status maps each status to its tone", () => {
  const ctx = (status: ThreadStatus) => ({
    ...EMPTY_CTX,
    selected: thread({ id: "t1", status }),
  });
  expect(METRICS["thread.status"]?.read(ctx(ThreadStatus.Active))).toEqual({
    value: "active",
    tone: Tone.Success,
  });
  expect(METRICS["thread.status"]?.read(ctx(ThreadStatus.Paused))).toEqual({
    value: "paused",
    tone: Tone.Warning,
  });
  expect(METRICS["thread.status"]?.read(ctx(ThreadStatus.Archived))).toEqual({
    value: "archived",
    tone: Tone.Dim,
  });
});

test("thread.created drops sub-second precision and the UTC offset", () => {
  const ctx = { ...EMPTY_CTX, selected: thread({ id: "t1" }) };
  expect(METRICS["thread.created"]?.read(ctx)).toEqual({
    value: "2026-10-01 08:48:08",
  });
});

test("thread.updated reports no activity for a fresh thread, the real timestamp otherwise", () => {
  const fresh = {
    ...EMPTY_CTX,
    selected: thread({ id: "t1", last_event_seq: 1 }),
  };
  expect(METRICS["thread.updated"]?.read(fresh)).toEqual({
    value: "no activity yet",
    tone: Tone.Dim,
  });

  const active = {
    ...EMPTY_CTX,
    selected: thread({ id: "t1", last_event_seq: 5 }),
  };
  expect(METRICS["thread.updated"]?.read(active)).toEqual({
    value: "2026-10-01 08:48:17",
  });
});

test("workspace shortens the home directory prefix", () => {
  expect(shortenHome(`${process.env.HOME}/Local/Developer/mitos`)).toBe(
    "~/Local/Developer/mitos",
  );
  expect(shortenHome("/opt/elsewhere")).toBe("/opt/elsewhere");
});

test("usage.model names the live model and stays null without one", () => {
  expect(METRICS["usage.model"]?.read(EMPTY_CTX)).toBeNull();
  const none = {
    ...EMPTY_CTX,
    usage: {
      harness: "codex",
      observed_at: "",
      input_tokens: null,
      output_tokens: null,
      cached_input_tokens: null,
      cost_usd: null,
      context_used_tokens: null,
      context_limit_tokens: null,
      model: null,
      turns: null,
      plan_five_hour_percent: null,
      plan_five_hour_resets_at: null,
      plan_week_percent: null,
      plan_week_resets_at: null,
      plan_month_percent: null,
      plan_month_resets_at: null,
    },
  };
  expect(METRICS["usage.model"]?.read(none)).toBeNull();
  const live = { ...none, usage: { ...none.usage, model: "glm-5.3" } };
  expect(METRICS["usage.model"]?.read(live)).toEqual({ value: "glm-5.3" });
});

test("usage.context_percent is null-safe when context fields are missing", () => {
  expect(METRICS["usage.context_percent"]?.read(EMPTY_CTX)).toBeNull();
  const ctx = {
    ...EMPTY_CTX,
    usage: {
      harness: "codex",
      observed_at: "",
      input_tokens: null,
      output_tokens: null,
      cached_input_tokens: null,
      cost_usd: null,
      context_used_tokens: 4096,
      context_limit_tokens: 8192,
      model: null,
      turns: null,
      plan_five_hour_percent: null,
      plan_five_hour_resets_at: null,
      plan_week_percent: null,
      plan_week_resets_at: null,
      plan_month_percent: null,
      plan_month_resets_at: null,
    },
  };
  expect(METRICS["usage.context_percent"]?.read(ctx)).toEqual({ value: "50%" });
});

function planContext(plan: {
  fivePercent?: number | null;
  fiveResets?: string | null;
  weekPercent?: number | null;
  weekResets?: string | null;
}): MetricContext {
  return {
    ...EMPTY_CTX,
    usage: {
      harness: "claude",
      observed_at: "",
      input_tokens: null,
      output_tokens: null,
      cached_input_tokens: null,
      cost_usd: null,
      context_used_tokens: null,
      context_limit_tokens: null,
      model: null,
      turns: null,
      plan_five_hour_percent: plan.fivePercent ?? null,
      plan_five_hour_resets_at: plan.fiveResets ?? null,
      plan_week_percent: plan.weekPercent ?? null,
      plan_week_resets_at: plan.weekResets ?? null,
    },
  };
}

test("plan percents round and escalate tone at 70 and 90", () => {
  const read = (percent: number) =>
    METRICS["usage.plan_5h"]?.read(planContext({ fivePercent: percent }));
  expect(read(50.6)).toEqual({ value: "51%", tone: undefined });
  expect(read(70)).toEqual({ value: "70%", tone: Tone.Warning });
  expect(read(93.2)).toEqual({ value: "93%", tone: Tone.Error });
  expect(METRICS["usage.plan_5h"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["usage.plan_week"]?.read(planContext({}))).toBeNull();
});

test("plan reset metrics format future windows and drop past ones", () => {
  const soon = new Date(Date.now() + 90 * 60_000 + 45_000).toISOString();
  const later = new Date(Date.now() + (2 * 24 + 3) * 3_600_000 + 60_000);
  const past = new Date(Date.now() - 60_000).toISOString();
  const ctx = planContext({
    fiveResets: soon,
    weekResets: later.toISOString(),
  });
  expect(METRICS["usage.plan_5h_resets"]?.read(ctx)?.value).toMatch(
    /^\d{1,2}:\d{2}(am|pm)$/,
  );
  expect(METRICS["usage.plan_week_resets"]?.read(ctx)?.value).toMatch(
    /^[A-Z][a-z]{2} \d{1,2}, \d{1,2}:\d{2}(am|pm)$/,
  );
  expect(METRICS["usage.plan_5h_remaining"]?.read(ctx)).toEqual({
    value: "1h 31m",
  });
  expect(METRICS["usage.plan_week_remaining"]?.read(ctx)).toEqual({
    value: "2d 3h",
  });
  const expired = planContext({ fiveResets: past, weekResets: "garbage" });
  expect(METRICS["usage.plan_5h_resets"]?.read(expired)).toBeNull();
  expect(METRICS["usage.plan_5h_remaining"]?.read(expired)).toBeNull();
  expect(METRICS["usage.plan_week_resets"]?.read(expired)).toBeNull();
});

function costContext(cost: number | null): MetricContext {
  return {
    ...EMPTY_CTX,
    usage: {
      harness: "claude",
      observed_at: "",
      input_tokens: null,
      output_tokens: null,
      cached_input_tokens: null,
      cost_usd: cost,
      context_used_tokens: null,
      context_limit_tokens: null,
      model: null,
      turns: null,
      plan_five_hour_percent: null,
      plan_five_hour_resets_at: null,
      plan_week_percent: null,
      plan_week_resets_at: null,
      plan_month_percent: null,
      plan_month_resets_at: null,
    },
  };
}

test("usage.cost is null without a snapshot or without a reported cost", () => {
  expect(METRICS["usage.cost"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["usage.cost"]?.read(costContext(null))).toBeNull();
});

test("usage.cost formats dollars, keeping four decimals below a cent", () => {
  expect(METRICS["usage.cost"]?.read(costContext(2.1016))).toEqual({
    value: "$2.10",
  });
  expect(METRICS["usage.cost"]?.read(costContext(0))).toEqual({
    value: "$0.00",
  });
  expect(METRICS["usage.cost"]?.read(costContext(0.0011634671))).toEqual({
    value: "$0.0012",
  });
});

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
    cost_usd: 48.8682245,
    tool_seconds: 100635.722,
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
  cost_by_day: [{ label: "2026-10-09", value: 2.5 }],
  cost_by_project: [
    { label: "/home/anders/.dotfiles", value: 8.2 },
    { label: "/etc/nixos", value: 11.77 },
  ],
  cost_by_model: [{ label: "glm-5.3", value: 28.65 }],
  cost_by_agent: [{ label: "build", value: 30.01 }],
  cost_by_harness: [
    { label: "opencode", value: 48.87 },
    { label: "codex", value: 0 },
  ],
  sessions_by_day: [{ label: "2026-10-09", value: 4 }],
  sessions_by_agent: [{ label: "build", value: 34 }],
  sessions_by_model: [{ label: "glm-5.3", value: 17 }],
  tokens_by_day: [{ label: "2026-10-09", value: 641366 }],
  tool_usage: [
    { label: "bash", value: 1332 },
    { label: "read", value: 958 },
  ],
  tool_time: [{ label: "bash", value: 10228.4 }],
  activity_by_day: [{ label: "2026-10-09", values: [0, 0, 5, 3] }],
  sessions: [
    {
      id: "ses_1",
      harness: "opencode",
      title: "Multi-OS Nix config",
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
  ],
};

test("mining metrics read null without a report and format with one", () => {
  expect(METRICS["mining.sessions"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["mining.cost"]?.read(EMPTY_CTX)).toBeNull();
  const ctx: MetricContext = { ...EMPTY_CTX, mining: REPORT };
  expect(METRICS["mining.sessions"]?.read(ctx)).toEqual({ value: "51" });
  expect(METRICS["mining.messages"]?.read(ctx)).toEqual({ value: "2,810" });
  expect(METRICS["mining.messages_user"]?.read(ctx)).toEqual({
    value: "228",
  });
  expect(METRICS["mining.messages_assistant"]?.read(ctx)).toEqual({
    value: "2,582",
  });
  expect(METRICS["mining.tool_calls"]?.read(ctx)).toEqual({
    value: "3,525",
  });
  expect(METRICS["mining.patches"]?.read(ctx)).toEqual({ value: "431" });
  expect(METRICS["mining.files_patched"]?.read(ctx)).toEqual({
    value: "696",
  });
  expect(METRICS["mining.forks"]?.read(ctx)).toEqual({ value: "7" });
  expect(METRICS["mining.cost"]?.read(ctx)).toEqual({ value: "$48.87" });
  expect(METRICS["mining.tokens_in"]?.read(ctx)).toEqual({
    value: "13,007,947",
  });
  expect(METRICS["mining.tokens_reasoning"]?.read(ctx)).toEqual({
    value: "5,528,811",
  });
  expect(METRICS["mining.tokens_cache_write"]?.read(ctx)).toEqual({
    value: "26,558,836",
  });
});

test("mining.tool_seconds formats a human duration and reads null at zero", () => {
  expect(METRICS["mining.tool_seconds"]?.read(EMPTY_CTX)).toBeNull();
  const ctx: MetricContext = { ...EMPTY_CTX, mining: REPORT };
  expect(METRICS["mining.tool_seconds"]?.read(ctx)).toEqual({
    value: "1d 3h",
  });
  const quiet = {
    ...EMPTY_CTX,
    mining: {
      ...REPORT,
      totals: { ...REPORT.totals, tool_seconds: 0 },
    },
  };
  expect(METRICS["mining.tool_seconds"]?.read(quiet)).toBeNull();
});

test("buildMetricContext keeps the mining series as raw arrays", () => {
  const root = buildMetricContext({ ...EMPTY_CTX, mining: REPORT });
  const mining = root.mining as Record<string, unknown>;
  expect(Array.isArray(mining.projects)).toBe(true);
  expect(mining.projects).toEqual(REPORT.cost_by_project);
  expect(mining.tools).toEqual(REPORT.tool_usage);
  expect(mining.recent).toEqual(REPORT.sessions);
  expect(mining.harnesses).toEqual(REPORT.harnesses);
  expect(mining.harness_costs).toEqual(REPORT.cost_by_harness);
  expect(mining.agents).toEqual(REPORT.cost_by_agent);
  expect(mining.agent_sessions).toEqual(REPORT.sessions_by_agent);
  expect(mining.model_sessions).toEqual(REPORT.sessions_by_model);
  expect(mining.tool_time).toEqual(REPORT.tool_time);
  expect(mining.session_days).toEqual(REPORT.sessions_by_day);
});

test("buildMetricContext leaves no series behind without a report", () => {
  const root = buildMetricContext(EMPTY_CTX);
  const mining = root.mining as Record<string, unknown>;
  expect(mining.projects).toBeUndefined();
});

test("estimate window metrics read the mined plan windows", () => {
  const est = {
    tier: "plus",
    burn_5h: 0.06,
    oldest_5h_ms: Date.now() - 3 * 3_600_000,
    burn_week: 0.3,
    burn_month: 0.4,
  };
  const ctx = {
    ...EMPTY_CTX,
    mining: { ...REPORT, est_windows: est },
  };
  expect(METRICS["estimate.5h"]?.read(ctx)).toEqual({ value: "30%" });
  expect(METRICS["estimate.week"]?.read(ctx)).toEqual({ value: "60%" });
  expect(METRICS["estimate.month"]?.read(ctx)).toEqual({ value: "40%" });
  expect(METRICS["estimate.5h_frees"]?.read(ctx)).toEqual({ value: "2h 0m" });
  const hot = {
    ...ctx,
    mining: {
      ...ctx.mining,
      est_windows: { ...est, burn_5h: 0.19 },
    },
  };
  expect(METRICS["estimate.5h"]?.read(hot)).toEqual({
    value: "95%",
    tone: Tone.Error,
  });
});

test("estimate window metrics stay null without mined windows", () => {
  expect(METRICS["estimate.5h"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["estimate.5h_frees"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["estimate.week"]?.read(EMPTY_CTX)).toBeNull();
  expect(METRICS["estimate.month"]?.read(EMPTY_CTX)).toBeNull();
});
