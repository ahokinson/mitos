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
  turns: number | null;
  plan_five_hour_percent: number | null;
  plan_five_hour_resets_at: string | null;
  plan_week_percent: number | null;
  plan_week_resets_at: string | null;
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
