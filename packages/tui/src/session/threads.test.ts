import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  createThreadListState,
  type Thread,
  ThreadMode,
  ThreadStatus,
} from "@session/threads.ts";

let dir: string;
let savedCore: string | undefined;

function thread(overrides: Partial<Thread> = {}): Thread {
  return {
    id: "t1",
    workspace_id: "w1",
    status: ThreadStatus.Active,
    mode: ThreadMode.Plan,
    active_harness: "codex",
    native_session: { id: 1 },
    last_event_seq: 1,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    opening_message: "hello",
    ...overrides,
  };
}

async function serve(threads: Thread[]): Promise<void> {
  const path = join(dir, "core.sh");
  await writeFile(
    path,
    `#!/bin/sh\ncat <<'EOF'\n${JSON.stringify(threads)}\nEOF\n`,
  );
  await chmod(path, 0o755);
  process.env.MITOS_CORE = path;
}

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-threads-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  savedCore = process.env.MITOS_CORE;
});

afterEach(async () => {
  if (savedCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = savedCore;
  await rm(dir, { recursive: true, force: true });
});

test("starts loading and empty until the first refresh", () => {
  const state = createThreadListState("key");
  expect(state.loading()).toBe(true);
  expect(state.threads()).toEqual([]);
});

test("refresh loads threads and clears loading", async () => {
  await serve([thread()]);
  const state = createThreadListState("key");
  state.refresh();
  expect(state.loading()).toBe(false);
  expect(state.threads()).toEqual([thread()]);
});

test("refresh without a workspace key yields no threads", () => {
  const state = createThreadListState(undefined);
  state.refresh();
  expect(state.loading()).toBe(false);
  expect(state.threads()).toEqual([]);
});

test("an unchanged poll preserves the current array", async () => {
  await serve([thread()]);
  const state = createThreadListState("key");
  state.refresh();
  const first = state.threads();
  state.refresh();
  expect(state.threads()).toBe(first);
});

test("each changed field replaces the array", async () => {
  const changes: Partial<Thread>[] = [
    { id: "t2" },
    { workspace_id: "w2" },
    { status: ThreadStatus.Paused },
    { mode: ThreadMode.Build },
    { active_harness: null },
    { last_event_seq: 2 },
    { created_at: "x" },
    { updated_at: "x" },
    { opening_message: null },
    { native_session: { id: 2 } },
  ];
  for (const change of changes) {
    await serve([thread()]);
    const state = createThreadListState("key");
    state.refresh();
    const first = state.threads();
    await serve([thread(change)]);
    state.refresh();
    expect(state.threads()).not.toBe(first);
    expect(state.threads()).toEqual([thread(change)]);
  }
});

test("a length change replaces the array", async () => {
  await serve([thread()]);
  const state = createThreadListState("key");
  state.refresh();
  await serve([thread(), thread({ id: "t2" })]);
  state.refresh();
  expect(state.threads()).toHaveLength(2);
});
