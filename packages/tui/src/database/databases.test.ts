import { Database, type SQLQueryBindings } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import {
  latestUsage,
  listThreads,
  pendingRequests,
  syncEvents,
} from "@database/databases.ts";
import { ThreadMode, ThreadStatus } from "@session/threads.ts";

function root(start: string): string {
  let dir = start;
  while (
    !(
      existsSync(join(dir, "Cargo.toml")) &&
      existsSync(join(dir, "package.json"))
    )
  ) {
    const parent = dirname(dir);
    if (parent === dir) throw new Error("workspace root not found");
    dir = parent;
  }
  return dir;
}

const ROOT = root(import.meta.dir);
const MIGRATIONS = ["0001_init.sql"].map((name) =>
  join(ROOT, "crates/mitos/migrations", name),
);
let dir: string;
let dbPath: string;

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-db-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  dbPath = join(dir, "mitos.db");
  const db = new Database(dbPath, { create: true });
  for (const path of MIGRATIONS) {
    for (const statement of (await readFile(path, "utf8"))
      .split(";")
      .map((value) => value.trim())
      .filter(Boolean))
      db.run(statement);
  }
  db.close();
});

afterEach(async () => rm(dir, { recursive: true, force: true }));

function seed(sql: string, ...params: SQLQueryBindings[]): void {
  const db = new Database(dbPath);
  db.query(sql).run(...params);
  db.close();
}

function workspace(id: string, key: string): void {
  seed(
    "INSERT INTO workspaces (id, root, workspace_key, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    id,
    `/${id}`,
    key,
    "2026-01-01T00:00:00Z",
    "2026-01-01T00:00:00Z",
  );
}

function thread(
  id: string,
  workspaceId: string,
  updated = "2026-01-01T00:00:00Z",
): void {
  seed(
    "INSERT INTO threads (id, workspace_id, status, active_harness, last_event_seq, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    id,
    workspaceId,
    "active",
    "codex",
    0,
    updated,
    updated,
  );
}

test("lists only sessions from the launched workspace", () => {
  workspace("ws-a", "workspace-a");
  workspace("ws-b", "workspace-b");
  thread("a", "ws-a");
  thread("b", "ws-b", "2026-01-02T00:00:00Z");
  seed(
    "INSERT INTO thread_events (thread_id, seq, kind, content, created_at) VALUES (?, ?, ?, ?, ?)",
    "a",
    1,
    "user_message",
    "continue auth work",
    "2026-01-01T00:00:00Z",
  );
  expect(listThreads("workspace-a", dbPath)).toEqual([
    expect.objectContaining({
      id: "a",
      workspace_id: "ws-a",
      status: ThreadStatus.Active,
      opening_message: "continue auth work",
    }),
  ]);
  expect(listThreads("workspace-b", dbPath)).toHaveLength(1);
});

test("reads new events and latest context/token/rate usage", () => {
  workspace("ws", "workspace");
  thread("session", "ws");
  seed(
    "INSERT INTO thread_events (thread_id, seq, kind, content, created_at) VALUES (?, ?, ?, ?, ?)",
    "session",
    1,
    "assistant_message",
    "done",
    "2026-01-01T00:00:00Z",
  );
  seed(
    "INSERT INTO usage_snapshots (id, thread_id, harness, observed_at, input_tokens, output_tokens, cached_input_tokens, cost_usd, context_used_tokens, context_limit_tokens, model, turns) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    "u",
    "session",
    "codex",
    "2026-01-01T00:00:00Z",
    100,
    50,
    25,
    0.0421,
    64000,
    128000,
    "gpt",
    2,
  );
  seed(
    "INSERT INTO harness_plan_usage (harness, plan_five_hour_percent, observed_at) VALUES (?, ?, ?)",
    "codex",
    42.5,
    "2026-01-01T00:00:00Z",
  );
  expect(syncEvents("session", 0, dbPath)).toHaveLength(1);
  expect(latestUsage("session", "codex", dbPath)).toEqual(
    expect.objectContaining({
      context_used_tokens: 64000,
      context_limit_tokens: 128000,
      input_tokens: 100,
      cost_usd: 0.0421,
      plan_five_hour_percent: 42.5,
    }),
  );
});

let snapshotCount = 0;

function snapshot(
  threadId: string,
  harness: string,
  turnId: string | null,
  cost: number | null,
): void {
  snapshotCount += 1;
  seed(
    "INSERT INTO usage_snapshots (id, thread_id, harness, turn_id, observed_at, cost_usd) VALUES (?, ?, ?, ?, ?, ?)",
    `snap-${snapshotCount}`,
    threadId,
    harness,
    turnId,
    `2026-01-01T00:00:${String(snapshotCount).padStart(2, "0")}Z`,
    cost,
  );
}

function threadCost(threadId: string, harness: string): number | null {
  return latestUsage(threadId, harness, dbPath)?.cost_usd ?? null;
}

test("cost is cumulative across turns, counting each turn's last report once", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "opencode", "turn-1", 0.1);
  snapshot("t", "opencode", "turn-1", 0.3);
  snapshot("t", "opencode", "turn-1", 0.5);
  snapshot("t", "opencode", "turn-2", 0.25);
  expect(threadCost("t", "opencode")).toBeCloseTo(0.75);
});

test("cost adds up across harnesses within one thread", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "claude", "turn-1", 1);
  snapshot("t", "opencode", "turn-2", 2);
  expect(threadCost("t", "opencode")).toBeCloseTo(3);
  expect(threadCost("t", "claude")).toBeCloseTo(3);
});

test("a handoff total does not double count a harness that streamed its cost", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "opencode", "turn-1", 0.5);
  snapshot("t", "opencode", null, 0.5);
  expect(threadCost("t", "opencode")).toBeCloseTo(0.5);
});

test("a handoff-only harness contributes its latest session total", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "claude", "turn-1", 1);
  snapshot("t", "hermes", null, 0.2);
  snapshot("t", "hermes", null, 0.4);
  expect(threadCost("t", "hermes")).toBeCloseTo(1.4);
});

function tokenSnapshot(
  threadId: string,
  harness: string,
  turnId: string | null,
  tokens: { input?: number; output?: number; cached?: number },
): void {
  snapshotCount += 1;
  seed(
    "INSERT INTO usage_snapshots (id, thread_id, harness, turn_id, observed_at, input_tokens, output_tokens, cached_input_tokens) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    `snap-${snapshotCount}`,
    threadId,
    harness,
    turnId,
    `2026-01-01T00:00:${String(snapshotCount).padStart(2, "0")}Z`,
    tokens.input ?? null,
    tokens.output ?? null,
    tokens.cached ?? null,
  );
}

test("tokens are cumulative across turns and harnesses, each turn counted once", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  tokenSnapshot("t", "claude", "turn-1", { input: 10, output: 1 });
  tokenSnapshot("t", "claude", "turn-1", { input: 100, output: 20, cached: 5 });
  tokenSnapshot("t", "codex", "turn-2", { input: 200, output: 30, cached: 50 });
  const usage = latestUsage("t", "codex", dbPath);
  expect(usage?.input_tokens).toBe(300);
  expect(usage?.output_tokens).toBe(50);
  expect(usage?.cached_input_tokens).toBe(55);
});

test("a turn's tokens come from the last report that carried them", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  tokenSnapshot("t", "hermes", "turn-1", {});
  tokenSnapshot("t", "hermes", "turn-1", { input: 40, output: 8 });
  tokenSnapshot("t", "hermes", "turn-1", {});
  const usage = latestUsage("t", "hermes", dbPath);
  expect(usage?.input_tokens).toBe(40);
  expect(usage?.output_tokens).toBe(8);
});

test("a handoff token total does not double count streamed turns", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  tokenSnapshot("t", "opencode", "turn-1", { input: 100, output: 10 });
  tokenSnapshot("t", "opencode", null, { input: 100, output: 10 });
  tokenSnapshot("t", "hermes", null, { input: 7, output: 3 });
  const usage = latestUsage("t", "opencode", dbPath);
  expect(usage?.input_tokens).toBe(107);
  expect(usage?.output_tokens).toBe(13);
});

test("tokens are null until something reports them", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  tokenSnapshot("t", "codex", "turn-1", {});
  const usage = latestUsage("t", "codex", dbPath);
  expect(usage?.input_tokens).toBeNull();
  expect(usage?.cached_input_tokens).toBeNull();
});

function observe(
  threadId: string,
  harness: string,
  session: string,
  fields: {
    cost?: number;
    contextUsed?: number;
    contextLimit?: number;
    model?: string;
    at?: string;
  },
): void {
  seed(
    "INSERT INTO hook_observations (harness, native_session, thread_id, model, cost_usd, context_used_tokens, context_limit_tokens, observed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    harness,
    session,
    threadId,
    fields.model ?? null,
    fields.cost ?? null,
    fields.contextUsed ?? null,
    fields.contextLimit ?? null,
    fields.at ?? "2026-01-01T00:10:00Z",
  );
}

test("hook-observed session costs add up across a thread's native sessions", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  observe("t", "claude", "s1", { cost: 1.5 });
  observe("t", "claude", "s2", { cost: 0.5 });
  expect(threadCost("t", "claude")).toBeCloseTo(2);
});

test("an observed cost replaces a handoff snapshot's for the same harness", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "opencode", null, 5);
  observe("t", "opencode", "s1", { cost: 2 });
  expect(threadCost("t", "opencode")).toBeCloseTo(2);
});

test("streamed turn costs win over a hook observation of the same harness", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "opencode", "turn-1", 1);
  observe("t", "opencode", "s1", { cost: 1 });
  expect(threadCost("t", "opencode")).toBeCloseTo(1);
});

test("a harness with only hook data still reports usage, with its live context", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  observe("t", "claude", "s1", {
    cost: 0.75,
    contextUsed: 16000,
    contextLimit: 200000,
    model: "opus",
  });
  const usage = latestUsage("t", "claude", dbPath);
  expect(usage?.cost_usd).toBeCloseTo(0.75);
  expect(usage?.context_used_tokens).toBe(16000);
  expect(usage?.context_limit_tokens).toBe(200000);
  expect(usage?.model).toBe("opus");
  expect(usage?.input_tokens).toBeNull();
});

test("a newer hook reading beats an older snapshot's context, and an older one loses", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  seed(
    "INSERT INTO usage_snapshots (id, thread_id, harness, observed_at, context_used_tokens, context_limit_tokens) VALUES (?, ?, ?, ?, ?, ?)",
    "snap-ctx",
    "t",
    "claude",
    "2026-01-01T00:05:00Z",
    1000,
    200000,
  );
  observe("t", "claude", "s1", {
    contextUsed: 9000,
    contextLimit: 200000,
    at: "2026-01-01T00:10:00Z",
  });
  expect(latestUsage("t", "claude", dbPath)?.context_used_tokens).toBe(9000);

  seed("DELETE FROM hook_observations WHERE thread_id = ?", "t");
  observe("t", "claude", "s1", {
    contextUsed: 7,
    contextLimit: 200000,
    at: "2026-01-01T00:01:00Z",
  });
  expect(latestUsage("t", "claude", dbPath)?.context_used_tokens).toBe(1000);
});

test("a hook observation never leaks into another thread's usage", () => {
  workspace("ws", "workspace");
  thread("a", "ws");
  thread("b", "ws");
  observe("a", "claude", "s1", { cost: 9, contextLimit: 200000 });
  expect(latestUsage("b", "claude", dbPath)).toBeNull();
});

test("cost is null until something reports one", () => {
  workspace("ws", "workspace");
  thread("t", "ws");
  snapshot("t", "codex", "turn-1", null);
  expect(threadCost("t", "codex")).toBeNull();
});

test("one thread's cost never leaks into another's", () => {
  workspace("ws", "workspace");
  thread("a", "ws");
  thread("b", "ws");
  snapshot("a", "claude", "turn-1", 9);
  snapshot("b", "claude", "turn-2", 1);
  expect(threadCost("b", "claude")).toBeCloseTo(1);
});

test("reads each thread's mode and only its pending requests", () => {
  workspace("ws", "workspace");
  thread("session", "ws");
  seed("UPDATE threads SET mode = 'plan' WHERE id = ?", "session");
  seed(
    "INSERT INTO harness_requests (id, thread_id, kind, payload_json, status, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    "r1",
    "session",
    "permission",
    JSON.stringify({ id: "native-1", title: "Bash make" }),
    "pending",
    "2026-01-01T00:00:01Z",
  );
  seed(
    "INSERT INTO harness_requests (id, thread_id, kind, status, created_at) VALUES (?, ?, ?, ?, ?)",
    "r0",
    "session",
    "question",
    "answered",
    "2026-01-01T00:00:00Z",
  );
  expect(listThreads("workspace", dbPath)[0]?.mode).toBe(ThreadMode.Plan);
  expect(pendingRequests("session", dbPath)).toEqual([
    expect.objectContaining({
      id: "r1",
      kind: "permission",
      payload: { id: "native-1", title: "Bash make" },
    }),
  ]);
});
