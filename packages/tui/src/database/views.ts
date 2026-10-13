import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { readJson } from "@core/invocations.ts";
import type { ThreadEvent } from "@session/events.ts";
import type { HarnessRequest } from "@session/requests.ts";
import type { Thread } from "@session/threads.ts";

export type UsageSnapshot = {
  harness: string;
  observed_at: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cached_input_tokens: number | null;
  cost_usd: number | null;
  context_used_tokens: number | null;
  context_limit_tokens: number | null;
  model: string | null;
  provider?: string | null;
  plan?: string | null;
  plan_label?: string | null;
  turns: number | null;
  plan_five_hour_percent: number | null;
  plan_five_hour_resets_at: string | null;
  plan_week_percent: number | null;
  plan_week_resets_at: string | null;
  plan_month_percent: number | null;
  plan_month_resets_at: string | null;
};

/** Reads only the workspace that launched this TUI, never the global state. */
export function listThreads(workspaceKey: string | undefined): Thread[] {
  if (!workspaceKey) return [];
  return readJson<Thread[]>(
    ["view", "threads", "--workspace-key", workspaceKey],
    [],
  );
}

/** Messages the user sent in this workspace, newest first, each text once. */
export function listUserMessages(
  workspaceKey: string | undefined,
  limit: number,
): string[] {
  if (!workspaceKey) return [];
  return readJson<string[]>(
    [
      "view",
      "history",
      "--workspace-key",
      workspaceKey,
      "--limit",
      String(limit),
    ],
    [],
  );
}

export function syncEvents(threadId: string, sinceSeq: number): ThreadEvent[] {
  return readJson<ThreadEvent[]>(
    ["thread", "sync", threadId, "--since", String(sinceSeq), "--json"],
    [],
  );
}

/** True only when the thread exists and has recorded nothing beyond its own
 * setup events. Any read failure counts as not empty, so a doubt never
 * deletes a thread. */
export function isThreadEmpty(threadId: string): boolean {
  return readJson<boolean>(["view", "empty", threadId], false);
}

export function pendingRequests(threadId: string): HarnessRequest[] {
  return readJson<HarnessRequest[]>(
    ["thread", "requests", threadId, "--json"],
    [],
  );
}

/** Latest context and model readings, the thread's cumulative tokens and cost,
 * plus the harness's account-wide rate windows. */
export function latestUsage(
  threadId: string,
  harness: string | null,
): UsageSnapshot | null {
  if (!harness) return null;
  return readJson<UsageSnapshot | null>(
    ["view", "usage", threadId, "--harness", harness],
    null,
  );
}

/** One bar of a grouped mining figure, such as a project's share of cost. */
export type MiningSeriesPoint = {
  label: string;
  value: number;
};

/** One day of message activity, one count per hour of the day. */
export type MiningActivityDay = {
  label: string;
  values: number[];
};

export type MiningTotals = {
  sessions: number;
  messages: number;
  messages_user: number;
  messages_assistant: number;
  tool_calls: number;
  patches: number;
  files_patched: number;
  forks: number;
  cost_usd: number;
  tool_seconds: number;
  tokens_input: number;
  tokens_output: number;
  tokens_reasoning: number;
  tokens_cache_read: number;
  tokens_cache_write: number;
};

/** One harness's share of the merged report, so a view can compare them. */
export type MiningHarness = {
  label: string;
  sessions: number;
  messages: number;
  tool_calls: number;
  patches: number;
  cost_usd: number;
  tokens_input: number;
  tokens_output: number;
  tokens_cache_read: number;
};

export type MinedSession = {
  id: string;
  harness: string;
  title: string;
  directory: string;
  cost_usd: number;
  tokens_input: number;
  tokens_output: number;
  tokens_reasoning: number;
  tokens_cache_read: number;
  model: string | null;
  agent: string | null;
  time_created: number;
  time_updated: number;
};

/** The merged mining payload of every harness with a minable history:
 * totals, per-harness aggregates, grouped series and sessions. */
export type MiningReport = {
  totals: MiningTotals;
  harnesses: MiningHarness[];
  cost_by_day: MiningSeriesPoint[];
  cost_by_project: MiningSeriesPoint[];
  cost_by_model: MiningSeriesPoint[];
  cost_by_agent: MiningSeriesPoint[];
  cost_by_harness: MiningSeriesPoint[];
  sessions_by_day: MiningSeriesPoint[];
  sessions_by_agent: MiningSeriesPoint[];
  sessions_by_model: MiningSeriesPoint[];
  tokens_by_day: MiningSeriesPoint[];
  tool_usage: MiningSeriesPoint[];
  tool_time: MiningSeriesPoint[];
  activity_by_day: MiningActivityDay[];
  sessions: MinedSession[];
  /** Estimated Go plan windows from mined spend, one machine's share. */
  est_windows?: EstWindows | null;
};

/** Each `burn_*` is a fraction of the tier's monthly allowance spent inside
 * that window, so `0.4` against the 5-hour window's 20% share is twice it. */
export type EstWindows = {
  tier: string;
  burn_5h: number;
  oldest_5h_ms: number | null;
  burn_week: number;
  burn_month: number;
};

/** Grouped series are read once for every panel, so the ceiling stays high
 * and each panel trims to its own `top`. */
const MINING_TOP = 100;

/** Mines every harness's history at once; a harness without a minable
 * history contributes nothing to the merged report. */
export function mineAll(): MiningReport | null {
  return readJson<MiningReport | null>(
    ["view", "mining", "--top", String(MINING_TOP)],
    null,
  );
}

/** The SQLite histories the merged mining report reads, whose writes mean
 * the report changed; a harness yet to mine contributes no path. */
export function resolveMiningWatchPaths(): string[] {
  const paths: string[] = [];
  const dataHome =
    process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share");
  for (const name of ["opencode-stable.db", "opencode.db"]) {
    const path = join(dataHome, "opencode", name);
    if (existsSync(path)) {
      paths.push(path);
      break;
    }
  }
  return paths;
}

/** The JSONL history roots the merged report also reads: Claude's projects
 * and Codex's sessions, each watched recursively because a new session file
 * can land anywhere in the tree. A harness yet to mine contributes no root. */
export function resolveHistoryWatchRoots(): string[] {
  const roots: string[] = [];
  const home = homedir();
  const claudeRoot = join(
    process.env.CLAUDE_CONFIG_DIR ?? join(home, ".claude"),
    "projects",
  );
  if (existsSync(claudeRoot)) roots.push(claudeRoot);
  const codexRoot = join(
    process.env.CODEX_HOME ?? join(home, ".codex"),
    "sessions",
  );
  if (existsSync(codexRoot)) roots.push(codexRoot);
  return roots;
}
