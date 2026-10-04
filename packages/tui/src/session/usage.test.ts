import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";
import { createSelectionUsageState } from "@session/usage.ts";
import { createRoot, createSignal } from "solid-js";

let dir: string;
let savedCore: string | undefined;

function thread(overrides: Partial<Thread> = {}): Thread {
  return {
    id: "t1",
    workspace_id: "w1",
    status: ThreadStatus.Active,
    mode: ThreadMode.Build,
    active_harness: "codex",
    native_session: null,
    last_event_seq: 0,
    created_at: "x",
    updated_at: "x",
    opening_message: null,
    ...overrides,
  };
}

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-usage-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  savedCore = process.env.MITOS_CORE;
  const script = join(dir, "core.sh");
  await writeFile(
    script,
    `#!/bin/sh
here=$(dirname "$0")
case "$1" in
  view) echo '{"harness":"codex","model":"m"}' ;;
  thread) cat "$here/requests.json" ;;
esac
`,
  );
  await chmod(script, 0o755);
  process.env.MITOS_CORE = script;
  await serveRequests('[{"id":"r1"}]');
});

afterEach(async () => {
  if (savedCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = savedCore;
  await rm(dir, { recursive: true, force: true });
});

async function serveRequests(json: string): Promise<void> {
  await writeFile(join(dir, "requests.json"), json);
}

test("no selection means no usage and no requests", () => {
  createRoot((dispose) => {
    const state = createSelectionUsageState(() => null);
    expect(state.usage()).toBeNull();
    expect(state.requests()).toEqual([]);
    dispose();
  });
});

test("a selected thread reads its usage and pending requests", () => {
  createRoot((dispose) => {
    const state = createSelectionUsageState(() => thread());
    expect(state.usage()?.model).toBe("m");
    expect(state.requests().map((request) => request.id)).toEqual(["r1"]);
    dispose();
  });
});

test("bump re-reads, and an unchanged request list keeps its identity", async () => {
  await createRoot(async (dispose) => {
    const state = createSelectionUsageState(() => thread());
    const first = state.requests();
    state.bump();
    expect(state.requests()).toBe(first);
    await serveRequests('[{"id":"r1"},{"id":"r2"}]');
    state.bump();
    expect(state.requests()).toHaveLength(2);
    await serveRequests('[{"id":"r3"},{"id":"r2"}]');
    state.bump();
    expect(state.requests().map((request) => request.id)).toEqual(["r3", "r2"]);
    dispose();
  });
});

test("switching threads re-reads usage", () => {
  createRoot((dispose) => {
    const [selected, setSelected] = createSignal<Thread | null>(null);
    const state = createSelectionUsageState(selected);
    expect(state.usage()).toBeNull();
    setSelected(thread());
    expect(state.usage()?.harness).toBe("codex");
    setSelected(thread({ active_harness: null }));
    expect(state.usage()).toBeNull();
    dispose();
  });
});
