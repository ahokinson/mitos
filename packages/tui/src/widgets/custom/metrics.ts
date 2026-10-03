import { homedir } from "node:os";

import type { UsageSnapshot } from "@database/databases.ts";
import { type Thread, ThreadStatus } from "@session/threads.ts";
import { Tone } from "@theme/themes.ts";

export type MetricValue = { value: string; tone?: Tone };

export type MetricContext = {
  selected: Thread | null;
  currentHarness: string | null;
  usage: UsageSnapshot | null;
  workspaceRoot: string | null;
};

export type Metric = {
  /** Shown unless a template row overrides it with its own `label`. */
  label: string;
  read: (ctx: MetricContext) => MetricValue | null;
};

function formatTimestamp(iso: string): string {
  return iso.slice(0, 19).replace("T", " ");
}

function statusTone(status: ThreadStatus): Tone {
  switch (status) {
    case ThreadStatus.Active:
      return "success";
    case ThreadStatus.Paused:
      return "warning";
    case ThreadStatus.Archived:
      return "dim";
  }
}

/** Replaces the home directory prefix with `~` — same convention
 * `config/paths.ts` reads back in `expandHome`. */
export function shortenHome(path: string): string {
  const home = homedir();
  if (path === home) return "~";
  return path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}

function tokens(value: number | null): string | null {
  return value === null ? null : value.toLocaleString();
}

/** Sub-cent costs keep four decimals so a cheap turn doesn't read as $0.00. */
function usd(value: number | null): string | null {
  if (value === null) return null;
  return `$${value.toFixed(value > 0 && value < 0.01 ? 4 : 2)}`;
}

const WARN_PERCENT = 70;
const ERROR_PERCENT = 90;

function planPercent(percent: number | null): MetricValue | null {
  if (percent === null) return null;
  const rounded = Math.round(percent);
  const tone =
    rounded >= ERROR_PERCENT
      ? Tone.Error
      : rounded >= WARN_PERCENT
        ? Tone.Warning
        : undefined;
  return { value: `${rounded}%`, tone };
}

function futureDate(iso: string | null, now: number): Date | null {
  if (!iso) return null;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) || date.getTime() <= now ? null : date;
}

/** Local time like `3:19pm`; the week window adds the date (`Oct 5, 2:59pm`). */
function resetsAt(iso: string | null, withDate: boolean): MetricValue | null {
  const date = futureDate(iso, Date.now());
  if (!date) return null;
  const time = date
    .toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
    .replace(/\s+/g, "")
    .toLowerCase();
  if (!withDate) return { value: time };
  const day = date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
  return { value: `${day}, ${time}` };
}

function remaining(iso: string | null): MetricValue | null {
  const now = Date.now();
  const date = futureDate(iso, now);
  if (!date) return null;
  const minutes = Math.max(1, Math.round((date.getTime() - now) / 60_000));
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const mins = minutes % 60;
  if (days > 0) return { value: `${days}d ${hours}h` };
  if (hours > 0) return { value: `${hours}h ${mins}m` };
  return { value: `${mins}m` };
}

/** The data a custom-widget template draws on, keyed by a template row's
 * `metric` field. */
export const METRICS: Record<string, Metric> = {
  harness: {
    label: "harness",
    read: (ctx) => {
      const value = ctx.selected?.active_harness ?? ctx.currentHarness;
      return value
        ? { value, tone: Tone.Accent }
        : { value: "none", tone: Tone.Dim };
    },
  },
  "thread.id": {
    label: "thread",
    read: (ctx) =>
      ctx.selected ? { value: ctx.selected.id.slice(0, 8) } : null,
  },
  "thread.status": {
    label: "status",
    read: (ctx) =>
      ctx.selected
        ? { value: ctx.selected.status, tone: statusTone(ctx.selected.status) }
        : null,
  },
  "thread.created": {
    label: "created",
    read: (ctx) =>
      ctx.selected ? { value: formatTimestamp(ctx.selected.created_at) } : null,
  },
  "thread.updated": {
    label: "updated",
    read: (ctx) => {
      if (!ctx.selected) return null;
      if (ctx.selected.last_event_seq <= 1)
        return { value: "no activity yet", tone: Tone.Dim };
      return { value: formatTimestamp(ctx.selected.updated_at) };
    },
  },
  workspace: {
    label: "workspace",
    read: (ctx) =>
      ctx.workspaceRoot ? { value: shortenHome(ctx.workspaceRoot) } : null,
  },
  "usage.harness": {
    label: "harness",
    read: (ctx) => (ctx.usage ? { value: ctx.usage.harness } : null),
  },
  "usage.context_percent": {
    label: "context",
    read: (ctx) => {
      if (
        !ctx.usage ||
        ctx.usage.context_used_tokens === null ||
        ctx.usage.context_limit_tokens === null
      )
        return null;
      const percent = Math.round(
        (ctx.usage.context_used_tokens / ctx.usage.context_limit_tokens) * 100,
      );
      return { value: `${percent}%` };
    },
  },
  "usage.tokens_in": {
    label: "in",
    read: (ctx) => {
      const value = ctx.usage ? tokens(ctx.usage.input_tokens) : null;
      return value ? { value } : null;
    },
  },
  "usage.tokens_out": {
    label: "out",
    read: (ctx) => {
      const value = ctx.usage ? tokens(ctx.usage.output_tokens) : null;
      return value ? { value } : null;
    },
  },
  "usage.tokens_cached": {
    label: "cached",
    read: (ctx) => {
      const value = ctx.usage ? tokens(ctx.usage.cached_input_tokens) : null;
      return value ? { value } : null;
    },
  },
  "usage.cost": {
    label: "cost",
    read: (ctx) => {
      const value = ctx.usage ? usd(ctx.usage.cost_usd) : null;
      return value ? { value } : null;
    },
  },
  "usage.plan_5h": {
    label: "rate 5h",
    read: (ctx) => planPercent(ctx.usage?.plan_five_hour_percent ?? null),
  },
  "usage.plan_5h_resets": {
    label: "resets",
    read: (ctx) => resetsAt(ctx.usage?.plan_five_hour_resets_at ?? null, false),
  },
  "usage.plan_5h_remaining": {
    label: "resets in",
    read: (ctx) => remaining(ctx.usage?.plan_five_hour_resets_at ?? null),
  },
  "usage.plan_week": {
    label: "rate week",
    read: (ctx) => planPercent(ctx.usage?.plan_week_percent ?? null),
  },
  "usage.plan_week_resets": {
    label: "resets",
    read: (ctx) => resetsAt(ctx.usage?.plan_week_resets_at ?? null, true),
  },
  "usage.plan_week_remaining": {
    label: "resets in",
    read: (ctx) => remaining(ctx.usage?.plan_week_resets_at ?? null),
  },
};

/** Flattens `METRICS` into the nested object `{{dotted.paths}}` resolve
 * against; a metric without a value contributes `""`. */
export function buildMetricContext(
  ctx: MetricContext,
): Record<string, unknown> {
  const root: Record<string, unknown> = {};
  for (const [key, metric] of Object.entries(METRICS)) {
    const value = metric.read(ctx)?.value ?? "";
    const parts = key.split(".");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      const existing = node[part];
      if (typeof existing === "object" && existing !== null) {
        node = existing as Record<string, unknown>;
      } else {
        const created: Record<string, unknown> = {};
        node[part] = created;
        node = created;
      }
    }
    node[parts.at(-1) ?? key] = value;
  }
  return root;
}
