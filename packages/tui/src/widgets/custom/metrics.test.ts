import { expect, test } from "bun:test";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";
import { Tone } from "@theme/themes.ts";
import {
  METRICS,
  type MetricContext,
  shortenHome,
} from "@widgets/custom/metrics.ts";

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
