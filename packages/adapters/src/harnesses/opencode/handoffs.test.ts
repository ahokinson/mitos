import { Database } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { collectOpenCodeHandoff } from "@harnesses/opencode/handoffs.ts";
import { Action } from "@protocol/actions.ts";
import { Harness } from "@protocol/harnesses.ts";

let dataHome: string;
let originalXdgDataHome: string | undefined;

beforeEach(async () => {
  dataHome = join(tmpdir(), `mitos-opencode-test-${crypto.randomUUID()}`);
  await mkdir(join(dataHome, "opencode"), { recursive: true });
  originalXdgDataHome = process.env.XDG_DATA_HOME;
  process.env.XDG_DATA_HOME = dataHome;
});

afterEach(async () => {
  if (originalXdgDataHome === undefined) delete process.env.XDG_DATA_HOME;
  else process.env.XDG_DATA_HOME = originalXdgDataHome;
  await rm(dataHome, { recursive: true, force: true });
});

type SessionSeed = {
  id: string;
  directory: string;
  parentId?: string;
  updated?: number;
  model?: string;
  tokens?: { input: number; output: number; cacheRead: number };
  cost?: number;
};

// Mirrors the columns of opencode 1.18's `session`, `message` and `part`
// tables that the reader touches.
function openDatabase(name = "opencode-stable.db"): Database {
  const database = new Database(join(dataHome, "opencode", name));
  database.run(`CREATE TABLE session (
    id TEXT PRIMARY KEY, directory TEXT NOT NULL, parent_id TEXT, model TEXT,
    tokens_input INTEGER DEFAULT 0 NOT NULL,
    tokens_output INTEGER DEFAULT 0 NOT NULL,
    tokens_cache_read INTEGER DEFAULT 0 NOT NULL,
    cost REAL DEFAULT 0 NOT NULL,
    time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL)`);
  database.run(`CREATE TABLE message (
    id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
    time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL,
    data TEXT NOT NULL)`);
  database.run(`CREATE TABLE part (
    id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL,
    time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL,
    data TEXT NOT NULL)`);
  return database;
}

function seedSession(database: Database, seed: SessionSeed): void {
  const now = seed.updated ?? Date.now();
  database.run(
    "INSERT INTO session (id, directory, parent_id, model, tokens_input, tokens_output, tokens_cache_read, cost, time_created, time_updated) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    [
      seed.id,
      seed.directory,
      seed.parentId ?? null,
      seed.model
        ? JSON.stringify({ id: seed.model, providerID: "opencode-go" })
        : null,
      seed.tokens?.input ?? 0,
      seed.tokens?.output ?? 0,
      seed.tokens?.cacheRead ?? 0,
      seed.cost ?? 0,
      now,
      now,
    ],
  );
}

let clock = 1;

function seedMessage(
  database: Database,
  sessionId: string,
  id: string,
  role: string,
  parts: Array<Record<string, unknown>>,
): void {
  const created = clock++;
  database.run(
    "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?, ?, ?, ?, ?)",
    [id, sessionId, created, created, JSON.stringify({ role })],
  );
  for (const [index, part] of parts.entries()) {
    database.run(
      "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?, ?, ?, ?, ?, ?)",
      [`${id}_${index}`, id, sessionId, created, created, JSON.stringify(part)],
    );
  }
}

function request(nativeSession: string | null = null) {
  return {
    protocol_version: 1,
    action: Action.CollectHandoff as const,
    harness: Harness.OpenCode,
    workdir: "/workspace",
    launched_at: new Date().toISOString(),
    native_session: nativeSession,
  };
}

test("reads transcript and usage from the real session, message and part tables", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "ses_1",
    directory: "/workspace",
    model: "glm-5.3",
    tokens: { input: 250, output: 90, cacheRead: 1000 },
    cost: 2.1016592399999996,
  });
  seedMessage(database, "ses_1", "msg_1", "user", [
    { type: "text", text: "hi" },
  ]);
  seedMessage(database, "ses_1", "msg_2", "assistant", [
    { type: "step-start" },
    { type: "reasoning", text: "thinking" },
    { type: "text", text: "hello" },
    { type: "tool", tool: "bash" },
    { type: "text", text: "all done" },
  ]);
  database.close();

  const response = collectOpenCodeHandoff(request());

  expect(response.native_session).toBe("ses_1");
  expect(response.transcript?.messages).toEqual([
    { role: "user", text: "hi" },
    { role: "assistant", text: "hello\nall done" },
  ]);
  expect(response.usage).toEqual({
    input_tokens: 250,
    output_tokens: 90,
    cached_input_tokens: 1000,
    cost_usd: 2.1016592399999996,
    model: "glm-5.3",
    turns: 1,
  });
});

test("a requested native session wins over a newer one in the workdir", () => {
  const database = openDatabase();
  seedSession(database, { id: "ses_old", directory: "/elsewhere" });
  seedSession(database, { id: "ses_new", directory: "/workspace" });
  seedMessage(database, "ses_old", "msg_1", "assistant", [
    { type: "text", text: "old" },
  ]);
  seedMessage(database, "ses_new", "msg_2", "assistant", [
    { type: "text", text: "new" },
  ]);
  database.close();

  const response = collectOpenCodeHandoff(request("ses_old"));

  expect(response.native_session).toBe("ses_old");
  expect(response.transcript?.messages).toEqual([
    { role: "assistant", text: "old" },
  ]);
});

test("synthetic and ignored text parts are left out", () => {
  const database = openDatabase();
  seedSession(database, { id: "ses_1", directory: "/workspace" });
  seedMessage(database, "ses_1", "msg_1", "user", [
    { type: "text", text: "real" },
    { type: "text", text: "injected", synthetic: true },
    { type: "text", text: "hidden", ignored: true },
  ]);
  database.close();

  const response = collectOpenCodeHandoff(request());

  expect(response.transcript?.messages).toEqual([
    { role: "user", text: "real" },
  ]);
});

test("a subagent session is not picked for the workdir", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "ses_child",
    directory: "/workspace",
    parentId: "ses_parent",
  });
  seedMessage(database, "ses_child", "msg_1", "assistant", [
    { type: "text", text: "child" },
  ]);
  database.close();

  expect(collectOpenCodeHandoff(request()).transcript).toBeNull();
});

test("a stale session in the workdir is not picked", () => {
  const database = openDatabase();
  seedSession(database, {
    id: "ses_1",
    directory: "/workspace",
    updated: Date.now() - 3_600_000,
  });
  seedMessage(database, "ses_1", "msg_1", "assistant", [
    { type: "text", text: "old" },
  ]);
  database.close();

  expect(collectOpenCodeHandoff(request()).transcript).toBeNull();
});

test("a found session without token data still reports its turn count", () => {
  const database = openDatabase();
  seedSession(database, { id: "ses_1", directory: "/workspace" });
  seedMessage(database, "ses_1", "msg_1", "assistant", [
    { type: "text", text: "hello" },
  ]);
  database.close();

  expect(collectOpenCodeHandoff(request()).usage).toEqual({ turns: 1 });
});

test("falls back to opencode.db when the stable database is absent", () => {
  const database = openDatabase("opencode.db");
  seedSession(database, { id: "ses_1", directory: "/workspace" });
  seedMessage(database, "ses_1", "msg_1", "user", [
    { type: "text", text: "hi" },
  ]);
  database.close();

  expect(collectOpenCodeHandoff(request()).native_session).toBe("ses_1");
});

test("usage and transcript are null when nothing matches", () => {
  const database = openDatabase();
  seedSession(database, { id: "ses_1", directory: "/somewhere-else" });
  database.close();

  const response = collectOpenCodeHandoff(request());

  expect(response.transcript).toBeNull();
  expect(response.usage).toBeNull();
});

test("no database at all yields an empty handoff", () => {
  const response = collectOpenCodeHandoff(request("ses_x"));

  expect(response.native_session).toBe("ses_x");
  expect(response.transcript).toBeNull();
});
