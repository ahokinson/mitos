import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  isThreadEmpty,
  latestUsage,
  listThreads,
  listUserMessages,
  pendingRequests,
  syncEvents,
} from "@database/views.ts";
import { EventKind } from "@session/events.ts";
import { RequestKind, RequestStatus } from "@session/requests.ts";
import { ThreadMode, ThreadStatus } from "@session/threads.ts";

let dir: string;
let originalCore: string | undefined;

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "mitos-views-test-"));
  originalCore = process.env.MITOS_CORE;
});

afterEach(async () => {
  if (originalCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = originalCore;
  await rm(dir, { recursive: true, force: true });
});

/** A stand-in for `mitos` that logs its arguments and prints `stdout`. */
async function fakeCore(stdout: string, exitCode = 0): Promise<void> {
  const program = join(dir, "mitos");
  await writeFile(
    program,
    `#!/bin/sh\nprintf '%s\\n' "$*" >> '${join(dir, "calls")}'\ncat <<'MITOS_EOF'\n${stdout}\nMITOS_EOF\nexit ${exitCode}\n`,
  );
  await chmod(program, 0o755);
  process.env.MITOS_CORE = program;
}

async function calls(): Promise<string[]> {
  try {
    return (await readFile(join(dir, "calls"), "utf8")).trim().split("\n");
  } catch {
    return [];
  }
}

const THREAD = {
  id: "a",
  workspace_id: "ws-a",
  status: ThreadStatus.Active,
  mode: ThreadMode.Build,
  active_harness: "codex",
  native_session: null,
  last_event_seq: 3,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  opening_message: "continue auth work",
};

test("threads come from the workspace's view, parsed as given", async () => {
  await fakeCore(JSON.stringify([THREAD]));
  expect(listThreads("workspace-a")).toEqual([THREAD]);
  expect(await calls()).toEqual(["view threads --workspace-key workspace-a"]);
});

test("without a workspace key nothing is read", async () => {
  await fakeCore(JSON.stringify([THREAD]));
  expect(listThreads(undefined)).toEqual([]);
  expect(listUserMessages(undefined, 5)).toEqual([]);
  expect(await calls()).toEqual([]);
});

test("the message history asks for the workspace and the limit", async () => {
  await fakeCore(JSON.stringify(["latest", "earlier"]));
  expect(listUserMessages("workspace-a", 50)).toEqual(["latest", "earlier"]);
  expect(await calls()).toEqual([
    "view history --workspace-key workspace-a --limit 50",
  ]);
});

test("events are read past the last seen sequence number", async () => {
  const event = {
    thread_id: "a",
    seq: 4,
    turn_id: null,
    harness: null,
    kind: EventKind.AssistantMessage,
    role: "assistant",
    content: "done",
    payload: null,
    created_at: "2026-01-01T00:00:00Z",
  };
  await fakeCore(JSON.stringify([event]));
  expect(syncEvents("a", 3)).toEqual([event]);
  expect(await calls()).toEqual(["thread sync a --since 3 --json"]);
});

test("pending requests keep the payload the core decoded", async () => {
  const request = {
    id: "r1",
    thread_id: "a",
    turn_id: null,
    harness: "claude",
    kind: RequestKind.Permission,
    payload: { id: "native-1", title: "Bash make" },
    status: RequestStatus.Pending,
    response: null,
    created_at: "2026-01-01T00:00:01Z",
    answered_at: null,
  };
  await fakeCore(JSON.stringify([request]));
  expect(pendingRequests("a")).toEqual([request]);
  expect(await calls()).toEqual(["thread requests a --json"]);
});

test("usage is the core's figures for the thread under a harness", async () => {
  const usage = {
    harness: "codex",
    observed_at: "2026-01-01T00:00:00Z",
    input_tokens: 100,
    output_tokens: 50,
    cached_input_tokens: 25,
    cost_usd: 0.0421,
    context_used_tokens: 64000,
    context_limit_tokens: 128000,
    model: "gpt",
    turns: 2,
    plan_five_hour_percent: 42.5,
    plan_five_hour_resets_at: null,
    plan_week_percent: null,
    plan_week_resets_at: null,
  };
  await fakeCore(JSON.stringify(usage));
  expect(latestUsage("a", "codex")).toEqual(usage);
  expect(await calls()).toEqual(["view usage a --harness codex"]);
});

test("usage is null with no harness, and when the core has none", async () => {
  await fakeCore("null");
  expect(latestUsage("a", null)).toBeNull();
  expect(await calls()).toEqual([]);
  expect(latestUsage("a", "claude")).toBeNull();
  expect(await calls()).toEqual(["view usage a --harness claude"]);
});

test("a thread is empty only when the core says so", async () => {
  await fakeCore("true");
  expect(isThreadEmpty("fresh")).toBe(true);
  expect(await calls()).toEqual(["view empty fresh"]);
  await fakeCore("false");
  expect(isThreadEmpty("used")).toBe(false);
});

test("a failed read falls back, so a doubt never deletes a thread", async () => {
  await fakeCore("true", 1);
  expect(isThreadEmpty("a")).toBe(false);
  expect(listThreads("workspace-a")).toEqual([]);
  expect(syncEvents("a", 0)).toEqual([]);
  expect(pendingRequests("a")).toEqual([]);
  expect(latestUsage("a", "codex")).toBeNull();

  await fakeCore("not json");
  expect(isThreadEmpty("a")).toBe(false);
  expect(listUserMessages("workspace-a", 5)).toEqual([]);
});

test("a core that cannot be started reads as nothing", () => {
  process.env.MITOS_CORE = join(dir, "does-not-exist");
  expect(isThreadEmpty("a")).toBe(false);
  expect(listThreads("workspace-a")).toEqual([]);
  expect(latestUsage("a", "codex")).toBeNull();
});
