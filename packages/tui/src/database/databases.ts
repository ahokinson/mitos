import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";

import { resolveDbPath } from "@config/paths.ts";
import type { EventKind, ThreadEvent } from "@session/events.ts";
import type { HarnessRequest } from "@session/requests.ts";
import type { Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";

type Row = Record<string, unknown>;

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

function openReadonly(path: string): Database | null {
  if (!existsSync(path)) return null;
  try {
    return new Database(path, { readonly: true });
  } catch {
    return null;
  }
}

let cachedDb: Database | null = null;
let cachedDbPath: string | null = null;

function getDb(path: string): Database | null {
  if (cachedDb && cachedDbPath === path) return cachedDb;
  const opened = openReadonly(path);
  if (opened) {
    cachedDb = opened;
    cachedDbPath = path;
  }
  return opened;
}

function mapThread(row: Row): Thread {
  return {
    id: row.id as string,
    workspace_id: row.workspace_id as string,
    status: row.status as ThreadStatus,
    mode: row.mode as ThreadMode,
    active_harness: row.active_harness as string | null,
    native_session: row.native_session
      ? JSON.parse(row.native_session as string)
      : null,
    last_event_seq: row.last_event_seq as number,
    created_at: row.created_at as string,
    updated_at: row.updated_at as string,
    opening_message: row.opening_message as string | null,
  };
}

/** Reads only the workspace that launched this TUI, never the global DB. */
export function listThreads(
  workspaceKey: string | undefined,
  dbPath: string = resolveDbPath(),
): Thread[] {
  if (!workspaceKey) return [];
  const db = getDb(dbPath);
  if (!db) return [];
  try {
    const rows = db
      .query(
        `SELECT threads.*, (
          SELECT content FROM thread_events
          WHERE thread_id = threads.id AND kind = 'user_message' AND content IS NOT NULL
          ORDER BY seq ASC LIMIT 1
        ) AS opening_message
        FROM threads
        JOIN workspaces ON workspaces.id = threads.workspace_id
        WHERE workspaces.workspace_key = ? AND threads.status != 'archived'
        ORDER BY threads.updated_at DESC`,
      )
      .all(workspaceKey) as Row[];
    return rows.map(mapThread);
  } catch {
    return [];
  }
}

/** Messages the user sent in this workspace, newest first, each text once. */
export function listUserMessages(
  workspaceKey: string | undefined,
  limit: number,
  dbPath: string = resolveDbPath(),
): string[] {
  if (!workspaceKey) return [];
  const db = getDb(dbPath);
  if (!db) return [];
  try {
    const rows = db
      .query(
        `SELECT thread_events.content AS content FROM thread_events
        JOIN threads ON threads.id = thread_events.thread_id
        JOIN workspaces ON workspaces.id = threads.workspace_id
        WHERE workspaces.workspace_key = ?
          AND thread_events.kind = 'user_message'
          AND thread_events.role = 'user'
          AND thread_events.content IS NOT NULL
        ORDER BY thread_events.created_at DESC, thread_events.seq DESC`,
      )
      .all(workspaceKey) as Row[];
    const seen = new Set<string>();
    for (const row of rows) {
      const text = (row.content as string).trim();
      if (text) seen.add(text);
      if (seen.size >= limit) break;
    }
    return [...seen];
  } catch {
    return [];
  }
}

function mapThreadEvent(row: Row): ThreadEvent {
  return {
    thread_id: row.thread_id as string,
    seq: row.seq as number,
    turn_id: row.turn_id as string | null,
    harness: row.harness as string | null,
    kind: row.kind as EventKind,
    role: row.role as string | null,
    content: row.content as string | null,
    payload: row.payload_json ? JSON.parse(row.payload_json as string) : null,
    created_at: row.created_at as string,
  };
}

export function syncEvents(
  threadId: string,
  sinceSeq: number,
  dbPath: string = resolveDbPath(),
): ThreadEvent[] {
  const db = getDb(dbPath);
  if (!db) return [];
  try {
    const rows = db
      .query(
        "SELECT * FROM thread_events WHERE thread_id = ? AND seq > ? ORDER BY seq",
      )
      .all(threadId, sinceSeq) as Row[];
    return rows.map(mapThreadEvent);
  } catch {
    return [];
  }
}

export function pendingRequests(
  threadId: string,
  dbPath: string = resolveDbPath(),
): HarnessRequest[] {
  const db = getDb(dbPath);
  if (!db) return [];
  try {
    const rows = db
      .query(
        "SELECT * FROM harness_requests WHERE thread_id = ? AND status = 'pending' ORDER BY created_at",
      )
      .all(threadId) as Row[];
    return rows.map((row) => ({
      id: row.id as string,
      thread_id: row.thread_id as string,
      turn_id: row.turn_id as string | null,
      harness: row.harness as string | null,
      kind: row.kind as HarnessRequest["kind"],
      payload: row.payload_json ? JSON.parse(row.payload_json as string) : null,
      status: row.status as HarnessRequest["status"],
      response: row.response_json
        ? JSON.parse(row.response_json as string)
        : null,
      created_at: row.created_at as string,
      answered_at: row.answered_at as string | null,
    }));
  } catch {
    return [];
  }
}

/** The figures that accumulate over a thread; the rest of a snapshot is a
 * point-in-time reading. */
const CUMULATIVE_COLUMNS = [
  "input_tokens",
  "output_tokens",
  "cached_input_tokens",
  "cost_usd",
] as const;

type CumulativeColumn = (typeof CUMULATIVE_COLUMNS)[number];

/** Hooks only report cost, so only that column has an observed source. */
const OBSERVED_COLUMNS: ReadonlySet<CumulativeColumn> = new Set(["cost_usd"]);

/** One figure summed across the whole thread, any harness, from the most
 * precise source each harness has: streamed turns, else hook observations
 * (one running total per native session), else the handoff snapshot (a
 * session total). A turn may report a running figure many times, so only its
 * last report counts. `null` when nothing reported it. `column` is never
 * user input. */
function threadTotal(
  db: Database,
  threadId: string,
  column: CumulativeColumn,
): number | null {
  const observed = OBSERVED_COLUMNS.has(column) ? column : "NULL";
  const row = db
    .query(
      `WITH reported AS (
         SELECT rowid AS rid, harness, turn_id, ${column} AS value
         FROM usage_snapshots
         WHERE thread_id = ?1 AND ${column} IS NOT NULL
       ),
       streamed AS (
         SELECT value, harness FROM reported AS r
         WHERE turn_id IS NOT NULL
           AND rid = (SELECT MAX(rid) FROM reported WHERE turn_id = r.turn_id)
       ),
       seen AS (
         SELECT harness, ${observed} AS value FROM hook_observations
         WHERE thread_id = ?1 AND ${observed} IS NOT NULL
       ),
       observed AS (
         SELECT value, harness FROM seen
         WHERE harness NOT IN (SELECT harness FROM streamed)
       ),
       collected AS (
         SELECT value FROM reported AS r
         WHERE turn_id IS NULL
           AND harness NOT IN (SELECT harness FROM streamed)
           AND harness NOT IN (SELECT harness FROM observed)
           AND rid = (SELECT MAX(rid) FROM reported
                      WHERE turn_id IS NULL AND harness = r.harness)
       )
       SELECT CASE WHEN (SELECT COUNT(*) FROM reported) + (SELECT COUNT(*) FROM seen) = 0 THEN NULL
              ELSE COALESCE((SELECT SUM(value) FROM streamed), 0)
                 + COALESCE((SELECT SUM(value) FROM observed), 0)
                 + COALESCE((SELECT SUM(value) FROM collected), 0)
              END AS total`,
    )
    .get(threadId) as { total: number | null } | null;
  return row?.total ?? null;
}

type ContextReading = {
  observed_at: string;
  model: string | null;
  context_used_tokens: number | null;
  context_limit_tokens: number | null;
};

/** Latest context and model readings (a live hook reading beats an older
 * snapshot), the thread's cumulative tokens and cost, plus account-wide rate
 * windows for the harness. A harness with only hook data still has usage. */
export function latestUsage(
  threadId: string,
  harness: string | null,
  dbPath: string = resolveDbPath(),
): UsageSnapshot | null {
  if (!harness) return null;
  const db = getDb(dbPath);
  if (!db) return null;
  try {
    const snapshot = db
      .query(
        `SELECT observed_at, model, turns, context_used_tokens, context_limit_tokens
         FROM usage_snapshots
         WHERE thread_id = ? AND harness = ?
         ORDER BY observed_at DESC LIMIT 1`,
      )
      .get(threadId, harness) as
      | (ContextReading & { turns: number | null })
      | null;
    const observation = db
      .query(
        `SELECT observed_at, model, context_used_tokens, context_limit_tokens
         FROM hook_observations
         WHERE thread_id = ? AND harness = ?
         ORDER BY observed_at DESC LIMIT 1`,
      )
      .get(threadId, harness) as ContextReading | null;
    if (!snapshot && !observation) return null;
    const limits = (db
      .query(
        `SELECT plan_five_hour_percent, plan_five_hour_resets_at,
                plan_week_percent, plan_week_resets_at
         FROM harness_plan_usage WHERE harness = ?`,
      )
      .get(harness) ?? {
      plan_five_hour_percent: null,
      plan_five_hour_resets_at: null,
      plan_week_percent: null,
      plan_week_resets_at: null,
    }) as Row;
    const liveReading =
      observation !== null &&
      observation.context_limit_tokens !== null &&
      (!snapshot || observation.observed_at > snapshot.observed_at);
    const context = liveReading ? observation : (snapshot ?? observation);
    const totals = Object.fromEntries(
      CUMULATIVE_COLUMNS.map((column) => [
        column,
        threadTotal(db, threadId, column),
      ]),
    );
    return {
      harness,
      observed_at: [snapshot?.observed_at, observation?.observed_at]
        .filter((at): at is string => typeof at === "string")
        .sort()
        .at(-1) as string,
      model: context?.model ?? snapshot?.model ?? observation?.model ?? null,
      turns: snapshot?.turns ?? null,
      context_used_tokens: context?.context_used_tokens ?? null,
      context_limit_tokens: context?.context_limit_tokens ?? null,
      ...limits,
      ...totals,
    } as UsageSnapshot;
  } catch {
    return null;
  }
}
