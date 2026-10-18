import { expect, test } from "bun:test";

import type { CommandContext, PendingDelete } from "@commands/contexts.ts";
import { dispatchCommand } from "@commands/dispatchers.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { createThreadCommands } from "@commands/handlers/threads.ts";
import { parseCommandInput } from "@commands/parsers.ts";
import { COMMANDS } from "@commands/registries.ts";
import {
  type HookInitOutcome,
  HookInitResult,
  type HookStatus,
  HookTrust,
} from "@session/hooks.ts";
import {
  Decision,
  type HarnessRequest,
  RequestKind,
  RequestStatus,
} from "@session/requests.ts";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";

const TEST_COMMANDS = {
  ...COMMANDS,
  ...createThreadCommands(() => ["claude", "codex"]),
};

function thread(overrides: Partial<Thread> & { id: string }): Thread {
  return {
    workspace_id: "ws1",
    status: ThreadStatus.Active,
    mode: ThreadMode.Build,
    agent: null,
    model: null,
    active_harness: null,
    native_session: null,
    last_event_seq: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    opening_message: null,
    ...overrides,
  };
}

type Notification = {
  threadId: string | null;
  tone: FeedbackTone;
  lines: readonly string[];
};
type MutationCall = { name: string; args: unknown[] };

function createFakeContext(options: {
  threads?: Thread[];
  selected?: Thread | null;
  defaultHarness?: string | null;
  requests?: HarnessRequest[];
  failWith?: Partial<
    Record<
      | "reassignHarness"
      | "archiveThread"
      | "compactThread"
      | "deleteThread"
      | "noteThread"
      | "setMode"
      | "answerRequest"
      | "abortThread"
      | "forkThread"
      | "shareThread"
      | "diffThread"
      | "setAgent"
      | "setModel"
      | "listAgents"
      | "listModels"
      | "runCommand"
      | "hooksStatus"
      | "hooksInit",
      Error
    >
  >;
  forkedThread?: Thread;
  sharedUrl?: string;
  diffLines?: string[];
  agentLines?: string[];
  modelLines?: string[];
  commandOutput?: string;
  hookStatus?: HookStatus[];
  hookOutcomes?: HookInitOutcome[];
}) {
  const notes: Notification[] = [];
  const calls: MutationCall[] = [];
  const state = {
    selected: options.selected ?? null,
    currentHarness: null as string | null,
    pendingDelete: null as PendingDelete | null,
    refreshed: 0,
    usageBumps: 0,
    newThreadCalls: [] as (string | null)[],
  };
  const threads = options.threads ?? [];

  const ctx: CommandContext = {
    list: { threads: () => threads, loading: () => false, refresh: () => {} },
    selected: () => state.selected,
    setSelected: (t) => {
      state.selected = t;
    },
    setCurrentHarness: (h) => {
      state.currentHarness = h;
    },
    defaultHarness: () => options.defaultHarness ?? null,
    startNewThread: (h) => {
      state.newThreadCalls.push(h);
      state.selected = null;
      state.currentHarness = h;
    },
    events: { events: () => [], sync: () => {} },
    pendingDelete: () => state.pendingDelete,
    setPendingDelete: (v) => {
      state.pendingDelete = v;
    },
    pendingRequests: () => options.requests ?? [],
    refresh: () => {
      state.refreshed += 1;
    },
    bumpUsage: () => {
      state.usageBumps += 1;
    },
    note: (threadId, tone, lines) =>
      notes.push({
        threadId,
        tone,
        lines: typeof lines === "string" ? [lines] : lines,
      }),
    mutations: {
      reassignHarness: async (threadId, to) => {
        calls.push({ name: "reassignHarness", args: [threadId, to] });
        if (options.failWith?.reassignHarness)
          throw options.failWith.reassignHarness;
      },
      archiveThread: async (threadId) => {
        calls.push({ name: "archiveThread", args: [threadId] });
        if (options.failWith?.archiveThread)
          throw options.failWith.archiveThread;
      },
      compactThread: async (threadId, mode) => {
        calls.push({ name: "compactThread", args: [threadId, mode] });
        if (options.failWith?.compactThread)
          throw options.failWith.compactThread;
      },
      deleteThread: async (threadId) => {
        calls.push({ name: "deleteThread", args: [threadId] });
        if (options.failWith?.deleteThread) throw options.failWith.deleteThread;
      },
      noteThread: async (threadId, fields) => {
        calls.push({ name: "noteThread", args: [threadId, fields] });
        if (options.failWith?.noteThread) throw options.failWith.noteThread;
      },
      setMode: async (threadId, mode) => {
        calls.push({ name: "setMode", args: [threadId, mode] });
        if (options.failWith?.setMode) throw options.failWith.setMode;
      },
      answerRequest: async (threadId, requestId, reply) => {
        calls.push({
          name: "answerRequest",
          args: [threadId, requestId, reply],
        });
        if (options.failWith?.answerRequest)
          throw options.failWith.answerRequest;
      },
      abortThread: async (threadId) => {
        calls.push({ name: "abortThread", args: [threadId] });
        if (options.failWith?.abortThread) throw options.failWith.abortThread;
      },
      forkThread: async (threadId) => {
        calls.push({ name: "forkThread", args: [threadId] });
        if (options.failWith?.forkThread) throw options.failWith.forkThread;
        return (
          options.forkedThread ??
          thread({ id: "thrn_fork", active_harness: "opencode" })
        );
      },
      shareThread: async (threadId) => {
        calls.push({ name: "shareThread", args: [threadId] });
        if (options.failWith?.shareThread) throw options.failWith.shareThread;
        return options.sharedUrl ?? "https://opencode.ai/s/abc";
      },
      diffThread: async (threadId) => {
        calls.push({ name: "diffThread", args: [threadId] });
        if (options.failWith?.diffThread) throw options.failWith.diffThread;
        return options.diffLines ?? [];
      },
      setAgent: async (threadId, agent) => {
        calls.push({ name: "setAgent", args: [threadId, agent] });
        if (options.failWith?.setAgent) throw options.failWith.setAgent;
      },
      setModel: async (threadId, model) => {
        calls.push({ name: "setModel", args: [threadId, model] });
        if (options.failWith?.setModel) throw options.failWith.setModel;
      },
      listAgents: async (threadId) => {
        calls.push({ name: "listAgents", args: [threadId] });
        if (options.failWith?.listAgents) throw options.failWith.listAgents;
        return options.agentLines ?? ["build: writes code", "plan: read only"];
      },
      listModels: async (threadId) => {
        calls.push({ name: "listModels", args: [threadId] });
        if (options.failWith?.listModels) throw options.failWith.listModels;
        return options.modelLines ?? ["opencode/glm-5.3"];
      },
      runCommand: async (threadId, command, args) => {
        calls.push({ name: "runCommand", args: [threadId, command, args] });
        if (options.failWith?.runCommand) throw options.failWith.runCommand;
        return options.commandOutput ?? "done";
      },
      hooksStatus: async () => {
        calls.push({ name: "hooksStatus", args: [] });
        if (options.failWith?.hooksStatus) throw options.failWith.hooksStatus;
        return options.hookStatus ?? [];
      },
      hooksInit: async (harnesses) => {
        calls.push({ name: "hooksInit", args: [harnesses] });
        if (options.failWith?.hooksInit) throw options.failWith.hooksInit;
        return options.hookOutcomes ?? [];
      },
    },
  };

  return { ctx, notes, calls, state };
}

async function run(line: string, ctx: CommandContext): Promise<void> {
  const parsed = parseCommandInput(line);
  if (parsed.kind !== "command") throw new Error("expected a command");
  await dispatchCommand(parsed.command, ctx, TEST_COMMANDS);
}

const CLAUDE_THREAD = thread({
  id: "11111111-aaaa-bbbb-cccc-111111111111",
  active_harness: "claude",
  opening_message: "fix the bug",
});
const CODEX_THREAD = thread({
  id: "22222222-aaaa-bbbb-cccc-222222222222",
  active_harness: "codex",
  opening_message: "fix another bug",
});
const OPENCODE_THREAD = thread({
  id: "33333333-aaaa-bbbb-cccc-333333333333",
  active_harness: "opencode",
  native_session: "ses_1",
  opening_message: "build the thing",
});

test("unknown command produces an error note, not a mutation", async () => {
  const { ctx, notes, calls } = createFakeContext({});
  await run("/bogus", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
  expect(notes[0]?.lines[0]).toContain("Unknown command");
});

test("/new with no args starts a draft with the configured default harness", async () => {
  const { ctx, state } = createFakeContext({ defaultHarness: "claude" });
  await run("/new", ctx);
  expect(state.newThreadCalls).toEqual(["claude"]);
});

test("/new rejects an uninstalled or unknown harness without starting a draft", async () => {
  const { ctx, state, notes } = createFakeContext({});
  await run("/new opencode", ctx);
  expect(state.newThreadCalls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/new with an installed harness starts a draft with it", async () => {
  const { ctx, state } = createFakeContext({});
  await run("/new codex", ctx);
  expect(state.newThreadCalls).toEqual(["codex"]);
});

test("/harness with no thread selected errors without reassigning", async () => {
  const { ctx, calls, notes } = createFakeContext({ selected: null });
  await run("/harness codex", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/harness reassigns the selected thread and refreshes", async () => {
  const { ctx, calls, state } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/harness codex", ctx);
  expect(calls).toEqual([
    { name: "reassignHarness", args: [CLAUDE_THREAD.id, "codex"] },
  ]);
  expect(state.currentHarness).toBe("codex");
  expect(state.refreshed).toBe(1);
});

test("/compact with no thread selected errors without compacting", async () => {
  const { ctx, calls, notes } = createFakeContext({ selected: null });
  await run("/compact", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/compact defaults to the mechanical mode and refreshes", async () => {
  const { ctx, calls, state } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/compact", ctx);
  expect(calls).toEqual([
    { name: "compactThread", args: [CLAUDE_THREAD.id, "mechanical"] },
  ]);
  expect(state.refreshed).toBe(1);
});

test("/compact intelligent passes the mode through", async () => {
  const { ctx, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/compact intelligent", ctx);
  expect(calls).toEqual([
    { name: "compactThread", args: [CLAUDE_THREAD.id, "intelligent"] },
  ]);
});

test("/compact rejects an unknown mode", async () => {
  const { ctx, calls, notes } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/compact lossy", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/compact surfaces a failure as an error note without throwing", async () => {
  const { ctx, notes, state } = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { compactThread: new Error("no summary") },
  });
  await run("/compact intelligent", ctx);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
  expect(state.refreshed).toBe(0);
});

test("/threads with no threads says so instead of printing an empty list", async () => {
  const { ctx, notes } = createFakeContext({ threads: [] });
  await run("/threads", ctx);
  expect(notes[0]?.lines[0]).toContain("No threads yet");
});

test("/threads lists every thread", async () => {
  const { ctx, notes } = createFakeContext({
    threads: [CLAUDE_THREAD, CODEX_THREAD],
  });
  await run("/threads", ctx);
  expect(notes[0]?.lines).toHaveLength(2);
});

test("/resume with a unique match switches the selected thread", async () => {
  const { ctx, state } = createFakeContext({
    threads: [CLAUDE_THREAD, CODEX_THREAD],
  });
  await run("/resume fix the bug", ctx);
  expect(state.selected).toEqual(CLAUDE_THREAD);
});

test("/resume with an ambiguous query does not switch, lists candidates", async () => {
  const { ctx, state, notes } = createFakeContext({
    threads: [CLAUDE_THREAD, CODEX_THREAD],
  });
  await run("/resume fix", ctx);
  expect(state.selected).toBeNull();
  expect(notes[0]?.lines.length).toBeGreaterThan(1);
});

test("/resume with no matches errors", async () => {
  const { ctx, notes } = createFakeContext({ threads: [CLAUDE_THREAD] });
  await run("/resume nonexistent", ctx);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/archive with no query archives the selected thread and clears selection", async () => {
  const { ctx, calls, state } = createFakeContext({
    threads: [CLAUDE_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/archive", ctx);
  expect(calls).toEqual([{ name: "archiveThread", args: [CLAUDE_THREAD.id] }]);
  expect(state.selected).toBeNull();
});

test("/archive by query targets a different, non-selected thread and keeps the current selection", async () => {
  const { ctx, calls, state } = createFakeContext({
    threads: [CLAUDE_THREAD, CODEX_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/archive fix another", ctx);
  expect(calls).toEqual([{ name: "archiveThread", args: [CODEX_THREAD.id] }]);
  expect(state.selected).toEqual(CLAUDE_THREAD);
});

test("/archive with an ambiguous query does not archive anything", async () => {
  const { ctx, calls } = createFakeContext({
    threads: [CLAUDE_THREAD, CODEX_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/archive fix", ctx);
  expect(calls).toEqual([]);
});

test("/delete then /delete confirm with the right id deletes the thread", async () => {
  const { ctx, calls, state } = createFakeContext({
    threads: [CLAUDE_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/delete fix the bug", ctx);
  expect(state.pendingDelete?.threadId).toBe(CLAUDE_THREAD.id);
  await run(`/delete confirm ${CLAUDE_THREAD.id.slice(0, 8)}`, ctx);
  expect(calls).toEqual([{ name: "deleteThread", args: [CLAUDE_THREAD.id] }]);
  expect(state.pendingDelete).toBeNull();
  expect(state.selected).toBeNull();
});

test("/delete confirm with a mismatched id is rejected and deletes nothing", async () => {
  const { ctx, calls, state } = createFakeContext({
    threads: [CLAUDE_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/delete fix the bug", ctx);
  await run("/delete confirm 99999999", ctx);
  expect(calls).toEqual([]);
  expect(state.pendingDelete?.threadId).toBe(CLAUDE_THREAD.id);
});

test("/delete cancel clears a pending delete", async () => {
  const { ctx, state } = createFakeContext({
    threads: [CLAUDE_THREAD],
    selected: CLAUDE_THREAD,
  });
  await run("/delete fix the bug", ctx);
  await run("/delete cancel", ctx);
  expect(state.pendingDelete).toBeNull();
});

test("/note on the selected thread calls noteThread and syncs events", async () => {
  const { ctx, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/note watch out for the race condition", ctx);
  expect(calls).toEqual([
    {
      name: "noteThread",
      args: [CLAUDE_THREAD.id, { note: "watch out for the race condition" }],
    },
  ]);
});

test("/decision and /question map to their own noteThread field", async () => {
  const { ctx, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/decision use postgres", ctx);
  await run("/question is this thread-safe?", ctx);
  expect(calls).toEqual([
    {
      name: "noteThread",
      args: [CLAUDE_THREAD.id, { decisions: ["use postgres"] }],
    },
    {
      name: "noteThread",
      args: [CLAUDE_THREAD.id, { questions: ["is this thread-safe?"] }],
    },
  ]);
});

test("/note with no thread selected errors instead of calling noteThread", async () => {
  const { ctx, calls, notes } = createFakeContext({ selected: null });
  await run("/note anything", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/help lists every registered command and never mutates anything", async () => {
  const { ctx, calls, notes } = createFakeContext({});
  await run("/help", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.lines.some((line) => line.startsWith("/new"))).toBe(true);
});

function request(kind: RequestKind, id = "req-1"): HarnessRequest {
  return {
    id,
    thread_id: CLAUDE_THREAD.id,
    turn_id: null,
    harness: "claude",
    kind,
    payload: { id: "native-1", kind, title: "do it" },
    status: RequestStatus.Pending,
    response: null,
    created_at: "2026-01-01T00:00:00Z",
    answered_at: null,
  };
}

test("/mode with no argument reports the current mode without mutating", async () => {
  const { ctx, notes, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/mode", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.lines[0]).toContain("build");
});

test("/mode plan sets the mode and refreshes", async () => {
  const { ctx, calls, state } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/mode plan", ctx);
  expect(calls).toEqual([
    { name: "setMode", args: [CLAUDE_THREAD.id, "plan"] },
  ]);
  expect(state.refreshed).toBe(1);
});

test("/mode rejects an unknown mode", async () => {
  const { ctx, notes, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/mode yolo", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/approve approves the oldest pending request", async () => {
  const { ctx, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [
      request(RequestKind.Permission, "req-1"),
      request(RequestKind.Permission, "req-2"),
    ],
  });
  await run("/approve", ctx);
  expect(calls).toEqual([
    {
      name: "answerRequest",
      args: [CLAUDE_THREAD.id, "req-1", { decision: Decision.Approve }],
    },
  ]);
});

test("/deny carries the feedback as the reason", async () => {
  const { ctx, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.PlanApproval)],
  });
  await run("/deny split it into two steps", ctx);
  expect(calls).toEqual([
    {
      name: "answerRequest",
      args: [
        CLAUDE_THREAD.id,
        "req-1",
        { decision: Decision.Deny, text: "split it into two steps" },
      ],
    },
  ]);
});

test("/approve on a question points at /answer instead of mutating", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.Question)],
  });
  await run("/approve", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.lines[0]).toContain("/answer");
});

test("/answer sends the text for a pending question", async () => {
  const { ctx, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.Question)],
  });
  await run("/answer blue", ctx);
  expect(calls).toEqual([
    {
      name: "answerRequest",
      args: [CLAUDE_THREAD.id, "req-1", { text: "blue" }],
    },
  ]);
});

test("answering with nothing pending is an error note", async () => {
  const { ctx, notes, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/approve", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("a failed answer surfaces the error and does not throw", async () => {
  const { ctx, notes } = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.Permission)],
    failWith: {
      answerRequest: new Error("the turn that asked is no longer running"),
    },
  });
  await run("/approve", ctx);
  expect(notes[0]?.lines[0]).toContain("no longer running");
});

const CODEX_UNTRUSTED: HookStatus = {
  harness: "codex",
  target: "/h/.codex/hooks.json",
  installed: true,
  trust: HookTrust.Untrusted,
  blocked_by: null,
  problems: [],
  last_seen: null,
};

test("/hooks shows status, including untrusted and read-only hooks", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    hookStatus: [
      CODEX_UNTRUSTED,
      {
        ...CODEX_UNTRUSTED,
        harness: "hermes",
        installed: false,
        trust: HookTrust.NotApplicable,
        blocked_by: "managed by Nix, read-only (/nix/store/x)",
      },
    ],
  });
  await run("/hooks", ctx);
  expect(calls).toEqual([{ name: "hooksStatus", args: [] }]);
  const text = notes[0]?.lines.join("\n") ?? "";
  expect(text).toContain("codex     installed, NOT APPROVED yet");
  expect(text).toContain("cannot write: managed by Nix, read-only");
  expect(notes[0]?.tone).toBe(FeedbackTone.Info);
});

test("/hooks status is the same as /hooks", async () => {
  const { ctx, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/hooks status", ctx);
  expect(calls).toEqual([{ name: "hooksStatus", args: [] }]);
});

test("/hooks init installs every harness and reports each result", async () => {
  const outcomes: HookInitOutcome[] = [
    {
      harness: "claude",
      result: HookInitResult.Installed,
      message: "statusLine now runs the hook",
      backup: "/h/settings.json.mitos-backup-1",
      snippet: null,
    },
  ];
  const { ctx, notes, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    hookOutcomes: outcomes,
  });
  await run("/hooks init", ctx);
  expect(calls).toEqual([{ name: "hooksInit", args: [[]] }]);
  expect(notes[0]?.lines[0]).toBe(
    "claude    installed: statusLine now runs the hook",
  );
});

test("/hooks init can be limited to named harnesses", async () => {
  const { ctx, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/hooks init claude codex", ctx);
  expect(calls).toEqual([{ name: "hooksInit", args: [["claude", "codex"]] }]);
});

test("/hooks with an unknown action or stray arguments prints usage and calls nothing", async () => {
  const { ctx, notes, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/hooks frobnicate", ctx);
  await run("/hooks status claude", ctx);
  expect(calls).toEqual([]);
  expect(notes.map((n) => n.tone)).toEqual([
    FeedbackTone.Error,
    FeedbackTone.Error,
  ]);
  expect(notes[0]?.lines[0]).toContain("Usage: /hooks");
});

test("/hooks surfaces a core failure as an error note instead of throwing", async () => {
  const { ctx, notes } = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { hooksStatus: new Error("could not reach mitos") },
  });
  await run("/hooks", ctx);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
  expect(notes[0]?.lines[0]).toContain("could not reach mitos");
});

test("/hooks works with no thread selected", async () => {
  const { ctx, notes } = createFakeContext({ selected: null });
  await run("/hooks", ctx);
  expect(notes[0]?.threadId).toBeNull();
});

test("annotation commands need a selected thread", async () => {
  for (const line of ["/note x", "/decision x", "/question x"]) {
    const { ctx, notes, calls } = createFakeContext({});
    await run(line, ctx);
    expect(calls).toEqual([]);
    expect(notes[0]?.lines[0]).toBe("No thread selected.");
  }
});

test("annotation commands print usage when given no text", async () => {
  for (const name of ["note", "decision", "question"]) {
    const { ctx, notes, calls } = createFakeContext({
      selected: CLAUDE_THREAD,
    });
    await run(`/${name}`, ctx);
    expect(calls).toEqual([]);
    expect(notes[0]?.lines[0]).toBe(`Usage: /${name} <text>`);
  }
});

test("annotation commands surface a failure as an error note", async () => {
  for (const line of ["/note x", "/decision x", "/question x"]) {
    const { ctx, notes } = createFakeContext({
      selected: CLAUDE_THREAD,
      failWith: { noteThread: new Error("write failed") },
    });
    await run(line, ctx);
    expect(notes[0]?.tone).toBe(FeedbackTone.Error);
    expect(notes[0]?.lines[0]).toBe("write failed");
  }
});

test("/mode needs a selected thread and surfaces failures", async () => {
  const none = createFakeContext({});
  await run("/mode plan", none.ctx);
  expect(none.notes[0]?.lines[0]).toBe("No thread selected.");

  const failing = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { setMode: new Error("no mode") },
  });
  await run("/mode build", failing.ctx);
  expect(failing.notes[0]?.lines[0]).toBe("no mode");
});

test("request commands need a selected thread", async () => {
  const { ctx, notes, calls } = createFakeContext({});
  await run("/approve", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.lines[0]).toBe("No thread selected.");
});

test("/answer rejects non-questions and empty text", async () => {
  const wrongKind = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.Permission)],
  });
  await run("/answer yes", wrongKind.ctx);
  expect(wrongKind.calls).toEqual([]);
  expect(wrongKind.notes[0]?.lines[0]).toContain("use /approve or /deny");

  const empty = createFakeContext({
    selected: CLAUDE_THREAD,
    requests: [request(RequestKind.Question)],
  });
  await run("/answer", empty.ctx);
  expect(empty.calls).toEqual([]);
  expect(empty.notes[0]?.lines[0]).toBe("Usage: /answer <text>");
});

test("/harness validates its argument and surfaces failures", async () => {
  const bare = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/harness", bare.ctx);
  expect(bare.notes[0]?.lines[0]).toBe("Usage: /harness <name>");

  const unknown = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/harness hermes", unknown.ctx);
  expect(unknown.calls).toEqual([]);
  expect(unknown.notes[0]?.lines[0]).toContain("Unknown or uninstalled");

  const failing = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { reassignHarness: new Error("refused") },
  });
  await run("/harness codex", failing.ctx);
  expect(failing.notes[0]?.lines[0]).toBe("refused");
});

test("/resume without a query prints usage", async () => {
  const { ctx, notes, state } = createFakeContext({ threads: [CLAUDE_THREAD] });
  await run("/resume", ctx);
  expect(state.selected).toBeNull();
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
  expect(notes[0]?.lines[0]).toContain("Usage: /resume");
});

test("/archive without a selection or query errors, and surfaces failures", async () => {
  const none = createFakeContext({});
  await run("/archive", none.ctx);
  expect(none.calls).toEqual([]);
  expect(none.notes[0]?.lines[0]).toContain("No thread selected");

  const failing = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { archiveThread: new Error("locked") },
  });
  await run("/archive", failing.ctx);
  expect(failing.notes[0]?.lines[0]).toBe("locked");
});

test("/delete <query> stages a pending delete with a warning", async () => {
  const { ctx, notes, state } = createFakeContext({
    threads: [CLAUDE_THREAD],
    selected: null,
  });
  await run("/delete fix the bug", ctx);
  expect(state.pendingDelete).toEqual({
    threadId: CLAUDE_THREAD.id,
    label: "fix the bug",
  });
  expect(notes[0]?.lines).toHaveLength(2);
});

test("/delete without a query prints usage", async () => {
  const { ctx, notes } = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/delete", ctx);
  expect(notes[0]?.lines[0]).toContain("Usage: /delete <query>");
});

test("/delete confirm needs a pending delete and surfaces failures", async () => {
  const none = createFakeContext({ selected: CLAUDE_THREAD });
  await run("/delete confirm 1111", none.ctx);
  expect(none.notes[0]?.lines[0]).toContain("No delete is pending");

  const failing = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { deleteThread: new Error("busy") },
  });
  failing.state.pendingDelete = { threadId: CLAUDE_THREAD.id, label: "x" };
  await run("/delete confirm 1111", failing.ctx);
  expect(failing.notes[0]?.lines[0]).toBe("busy");
});

test("/delete confirm on another thread keeps the current selection", async () => {
  const { ctx, state, calls } = createFakeContext({ selected: CODEX_THREAD });
  state.pendingDelete = { threadId: CLAUDE_THREAD.id, label: "x" };
  await run("/delete confirm 1111", ctx);
  expect(calls).toEqual([{ name: "deleteThread", args: [CLAUDE_THREAD.id] }]);
  expect(state.selected).toEqual(CODEX_THREAD);
});

test("/delete confirm without an id prefix is rejected", async () => {
  const { ctx, state, calls } = createFakeContext({ selected: CLAUDE_THREAD });
  state.pendingDelete = { threadId: CLAUDE_THREAD.id, label: "x" };
  await run("/delete confirm", ctx);
  expect(calls).toEqual([]);
});

test("/abort asks OpenCode to stop the selected thread's turn", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: OPENCODE_THREAD,
  });
  await run("/abort", ctx);
  expect(calls).toEqual([{ name: "abortThread", args: [OPENCODE_THREAD.id] }]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Info);
});

test("/abort without a selected thread prints usage", async () => {
  const { ctx, notes, calls } = createFakeContext({});
  await run("/abort", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("/fork selects the forked thread and notes its id", async () => {
  const forked = thread({
    id: "44444444-aaaa-bbbb-cccc-444444444444",
    active_harness: "opencode",
  });
  const { ctx, notes, calls, state } = createFakeContext({
    selected: OPENCODE_THREAD,
    forkedThread: forked,
  });
  await run("/fork", ctx);
  expect(calls).toEqual([{ name: "forkThread", args: [OPENCODE_THREAD.id] }]);
  expect(state.selected).toEqual(forked);
  expect(state.currentHarness).toBe("opencode");
  expect(notes[0]?.lines[0]).toContain("44444444");
});

test("/fork failure surfaces the core's error", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: CLAUDE_THREAD,
    failWith: { forkThread: new Error("only OpenCode threads support fork") },
  });
  await run("/fork", ctx);
  expect(calls.length).toBe(1);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
  expect(notes[0]?.lines[0]).toContain("only OpenCode threads support fork");
});

test("/share prints the returned link", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: OPENCODE_THREAD,
    sharedUrl: "https://opencode.ai/s/xyz",
  });
  await run("/share", ctx);
  expect(calls).toEqual([{ name: "shareThread", args: [OPENCODE_THREAD.id] }]);
  expect(notes[0]?.lines[0]).toBe("https://opencode.ai/s/xyz");
});

test("/diff lists the changed files and notes when none", async () => {
  const withChanges = createFakeContext({
    selected: OPENCODE_THREAD,
    diffLines: ["src/a.rs: +3 -1", "src/b.rs: +0 -2"],
  });
  await run("/diff", withChanges.ctx);
  expect(withChanges.notes[0]?.lines).toEqual([
    "src/a.rs: +3 -1",
    "src/b.rs: +0 -2",
  ]);

  const empty = createFakeContext({ selected: OPENCODE_THREAD });
  await run("/diff", empty.ctx);
  expect(empty.notes[0]?.lines[0]).toContain("No file changes");
});

test("/agent with a name sets it; bare lists the server's agents", async () => {
  const setting = createFakeContext({ selected: OPENCODE_THREAD });
  await run("/agent review", setting.ctx);
  expect(setting.calls).toEqual([
    { name: "setAgent", args: [OPENCODE_THREAD.id, "review"] },
  ]);
  expect(setting.notes[0]?.lines[0]).toBe("Agent set to review.");

  const listing = createFakeContext({ selected: OPENCODE_THREAD });
  await run("/agent", listing.ctx);
  expect(listing.calls).toEqual([
    { name: "listAgents", args: [OPENCODE_THREAD.id] },
  ]);
  expect(listing.notes[0]?.lines).toEqual([
    "build: writes code",
    "plan: read only",
  ]);
});

test("/model with a reference sets it; bare lists the defaults", async () => {
  const setting = createFakeContext({ selected: OPENCODE_THREAD });
  await run("/model opencode/glm-5.3", setting.ctx);
  expect(setting.calls).toEqual([
    { name: "setModel", args: [OPENCODE_THREAD.id, "opencode/glm-5.3"] },
  ]);
  expect(setting.notes[0]?.lines[0]).toBe("Model set to opencode/glm-5.3.");

  const listing = createFakeContext({ selected: OPENCODE_THREAD });
  await run("/model", listing.ctx);
  expect(listing.calls).toEqual([
    { name: "listModels", args: [OPENCODE_THREAD.id] },
  ]);
  expect(listing.notes[0]?.lines).toEqual(["opencode/glm-5.3"]);
});

test("/oc runs the command and prints its text", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: OPENCODE_THREAD,
    commandOutput: "initialized",
  });
  await run("/oc init", ctx);
  expect(calls).toEqual([
    { name: "runCommand", args: [OPENCODE_THREAD.id, "init", []] },
  ]);
  expect(notes[0]?.lines[0]).toBe("initialized");
});

test("/oc without a command prints usage", async () => {
  const { ctx, notes, calls } = createFakeContext({
    selected: OPENCODE_THREAD,
  });
  await run("/oc", ctx);
  expect(calls).toEqual([]);
  expect(notes[0]?.tone).toBe(FeedbackTone.Error);
});

test("session commands need a selected thread", async () => {
  for (const line of [
    "/fork",
    "/share",
    "/diff",
    "/agent",
    "/model",
    "/oc init",
  ]) {
    const { ctx, notes, calls } = createFakeContext({});
    await run(line, ctx);
    expect(calls).toEqual([]);
    expect(notes[0]?.tone).toBe(FeedbackTone.Error);
    expect(notes[0]?.lines[0]).toContain("No thread selected");
  }
});

type MutationFailure = NonNullable<
  Parameters<typeof createFakeContext>[0]
>["failWith"];

test("session commands surface a core failure instead of throwing", async () => {
  const cases: [
    string,
    (
      | "abortThread"
      | "shareThread"
      | "diffThread"
      | "setAgent"
      | "setModel"
      | "runCommand"
    ),
    string,
  ][] = [
    ["/abort", "abortThread", "no turn running"],
    ["/share", "shareThread", "publish refused"],
    ["/diff", "diffThread", "no native session"],
    ["/agent review", "setAgent", "unknown agent"],
    ["/model opencode/x", "setModel", "unknown model"],
    ["/oc init", "runCommand", "no such command"],
  ];
  for (const [line, mutation, message] of cases) {
    const { ctx, notes, calls } = createFakeContext({
      selected: OPENCODE_THREAD,
      failWith: { [mutation]: new Error(message) } as MutationFailure,
    });
    await run(line, ctx);
    expect(calls.map((call) => call.name)).toEqual([mutation]);
    expect(notes[0]?.tone).toBe(FeedbackTone.Error);
    expect(notes[0]?.lines[0]).toBe(message);
  }
});

test("empty outputs get their own fallback notes", async () => {
  const share = createFakeContext({
    selected: OPENCODE_THREAD,
    sharedUrl: "",
  });
  await run("/share", share.ctx);
  expect(share.notes[0]?.lines[0]).toBe("OpenCode returned no link.");

  const oc = createFakeContext({
    selected: OPENCODE_THREAD,
    commandOutput: "",
  });
  await run("/oc init", oc.ctx);
  expect(oc.notes[0]?.lines[0]).toBe("The command returned no text.");

  const agents = createFakeContext({
    selected: OPENCODE_THREAD,
    agentLines: [],
  });
  await run("/agent", agents.ctx);
  expect(agents.notes[0]?.lines[0]).toBe("The server reported no agents.");

  const models = createFakeContext({
    selected: OPENCODE_THREAD,
    modelLines: [],
  });
  await run("/model", models.ctx);
  expect(models.notes[0]?.lines[0]).toBe(
    "The server reported no default models.",
  );
});
