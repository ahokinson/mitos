import { Database } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { collectHermesHandoff } from "@harnesses/hermes/handoffs.ts";
import { Action } from "@protocol/actions.ts";
import { Harness } from "@protocol/harnesses.ts";

let home: string;
let originalHermesHome: string | undefined;

beforeEach(async () => {
  home = join(tmpdir(), `mitos-hermes-test-${crypto.randomUUID()}`);
  await mkdir(home, { recursive: true });
  originalHermesHome = process.env.HERMES_HOME;
  process.env.HERMES_HOME = home;
});

afterEach(async () => {
  if (originalHermesHome === undefined) delete process.env.HERMES_HOME;
  else process.env.HERMES_HOME = originalHermesHome;
  await rm(home, { recursive: true, force: true });
});

type SessionSeed = {
  id: string;
  cwd: string;
  startedAt?: number;
  lastActivityAt?: number | null;
  archived?: number;
  hidden?: number;
  model?: string | null;
  tokens?: { input: number; output: number; cacheRead: number };
  estimatedCost?: number | null;
  actualCost?: number | null;
};

// Mirrors the columns of hermes 0.20's `sessions` and `messages` tables that
// the reader touches. Timestamps are epoch seconds.
function openDatabase(): Database {
  const database = new Database(join(home, "state.db"));
  database.run(`CREATE TABLE sessions (
    id TEXT PRIMARY KEY, source TEXT NOT NULL, model TEXT,
    started_at REAL NOT NULL, ended_at REAL, last_activity_at REAL,
    input_tokens INTEGER DEFAULT 0, output_tokens INTEGER DEFAULT 0,
    cache_read_tokens INTEGER DEFAULT 0, cwd TEXT,
    estimated_cost_usd REAL, actual_cost_usd REAL,
    archived INTEGER NOT NULL DEFAULT 0, hidden INTEGER NOT NULL DEFAULT 0)`);
  database.run(`CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL,
    role TEXT NOT NULL, content TEXT, tool_name TEXT,
    timestamp REAL NOT NULL)`);
  return database;
}

function seedSession(database: Database, seed: SessionSeed): void {
  const started = seed.startedAt ?? Date.now() / 1000;
  database.run(
    "INSERT INTO sessions (id, source, model, started_at, last_activity_at, input_tokens, output_tokens, cache_read_tokens, estimated_cost_usd, actual_cost_usd, cwd, archived, hidden) VALUES (?, 'cli', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    [
      seed.id,
      seed.model ?? null,
      started,
      seed.lastActivityAt === undefined ? started : seed.lastActivityAt,
      seed.tokens?.input ?? 0,
      seed.tokens?.output ?? 0,
      seed.tokens?.cacheRead ?? 0,
      seed.estimatedCost ?? null,
      seed.actualCost ?? null,
      seed.cwd,
      seed.archived ?? 0,
      seed.hidden ?? 0,
    ],
  );
}

function seedMessage(
  database: Database,
  sessionId: string,
  role: string,
  content: string | null,
  toolName: string | null = null,
): void {
  database.run(
    "INSERT INTO messages (session_id, role, content, tool_name, timestamp) VALUES (?, ?, ?, ?, ?)",
    [sessionId, role, content, toolName, Date.now() / 1000],
  );
}

function request(nativeSession: string | null = null) {
  return {
    protocol_version: 1,
    action: Action.CollectHandoff as const,
    harness: Harness.Hermes,
    workdir: "/workspace",
    launched_at: new Date().toISOString(),
    native_session: nativeSession,
  };
}

test("reads transcript and usage from the real sessions and messages tables", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "20260829_230910_a6f34e",
    cwd: "/workspace",
    model: "deepseek/deepseek-v4-flash",
    tokens: { input: 17372, output: 101, cacheRead: 2048 },
    estimatedCost: 0.0011634671,
  });
  seedMessage(database, "20260829_230910_a6f34e", "user", "hi");
  seedMessage(database, "20260829_230910_a6f34e", "tool", "ls output", "bash");
  seedMessage(database, "20260829_230910_a6f34e", "assistant", "hello");
  database.close();

  const response = collectHermesHandoff(request());

  expect(response.native_session).toBe("20260829_230910_a6f34e");
  expect(response.transcript?.messages).toEqual([
    { role: "user", text: "hi" },
    { role: "assistant", text: "hello" },
  ]);
  expect(response.usage).toEqual({
    input_tokens: 17372,
    output_tokens: 101,
    cached_input_tokens: 2048,
    cost_usd: 0.0011634671,
    model: "deepseek/deepseek-v4-flash",
    turns: 1,
  });
});

test("actual cost wins over the estimate", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "s1",
    cwd: "/workspace",
    tokens: { input: 1, output: 1, cacheRead: 0 },
    estimatedCost: 0.5,
    actualCost: 0.4,
  });
  seedMessage(database, "s1", "assistant", "hello");
  database.close();

  expect(collectHermesHandoff(request()).usage?.cost_usd).toBe(0.4);
});

test("a session with no recorded cost leaves cost_usd unset", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "s1",
    cwd: "/workspace",
    tokens: { input: 1, output: 1, cacheRead: 0 },
  });
  seedMessage(database, "s1", "assistant", "hello");
  database.close();

  expect(collectHermesHandoff(request()).usage?.cost_usd).toBeUndefined();
});

test("a requested native session wins over another in the workdir", () => {
  const database = openDatabase();
  seedSession(database, { id: "old", cwd: "/elsewhere" });
  seedSession(database, { id: "new", cwd: "/workspace" });
  seedMessage(database, "old", "assistant", "old reply");
  seedMessage(database, "new", "assistant", "new reply");
  database.close();

  const response = collectHermesHandoff(request("old"));

  expect(response.native_session).toBe("old");
  expect(response.transcript?.messages).toEqual([
    { role: "assistant", text: "old reply" },
  ]);
});

test("falls back to start time when a session has no recorded activity", () => {
  const database = openDatabase();
  seedSession(database, { id: "s1", cwd: "/workspace", lastActivityAt: null });
  seedMessage(database, "s1", "assistant", "hello");
  database.close();

  expect(collectHermesHandoff(request()).native_session).toBe("s1");
});

test("a stale session in the workdir is not picked", () => {
  const database = openDatabase();
  const hourAgo = Date.now() / 1000 - 3600;
  seedSession(database, {
    id: "s1",
    cwd: "/workspace",
    startedAt: hourAgo,
    lastActivityAt: hourAgo,
  });
  seedMessage(database, "s1", "assistant", "old");
  database.close();

  expect(collectHermesHandoff(request()).transcript).toBeNull();
});

test("archived and hidden sessions are not picked", () => {
  const database = openDatabase();
  seedSession(database, { id: "a", cwd: "/workspace", archived: 1 });
  seedSession(database, { id: "h", cwd: "/workspace", hidden: 1 });
  seedMessage(database, "a", "assistant", "archived");
  seedMessage(database, "h", "assistant", "hidden");
  database.close();

  expect(collectHermesHandoff(request()).transcript).toBeNull();
});

test("a found session without token data still reports its turn count", () => {
  const database = openDatabase();
  seedSession(database, { id: "s1", cwd: "/workspace" });
  seedMessage(database, "s1", "assistant", "hello");
  database.close();

  expect(collectHermesHandoff(request()).usage).toEqual({ turns: 1 });
});

test("usage and transcript are null when nothing matches", () => {
  const database = openDatabase();
  seedSession(database, { id: "s1", cwd: "/somewhere-else" });
  database.close();

  const response = collectHermesHandoff(request());

  expect(response.transcript).toBeNull();
  expect(response.usage).toBeNull();
});

test("no database at all yields an empty handoff", () => {
  const response = collectHermesHandoff(request("s9"));

  expect(response.native_session).toBe("s9");
  expect(response.transcript).toBeNull();
});
