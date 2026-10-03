import { expect, test } from "bun:test";

import {
  processNotification,
  processTurnCompleted,
  requestFromServer,
  resultFor,
  TurnTokens,
  threadSandbox,
  turnPolicy,
} from "@harnesses/codex/streams.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { RequestKind } from "@protocol/asks.ts";
import { AdapterEventKind } from "@protocol/events.ts";

// Notification and request shapes follow the schema emitted by
// `codex app-server generate-json-schema` (codex-cli 0.159.3).

test("agent message deltas and completed items become assistant events", () => {
  expect(
    processNotification(
      "item/agentMessage/delta",
      { delta: "Hel", itemId: "i" },
      "turn-1",
    ),
  ).toEqual([
    {
      event: AdapterEventKind.AssistantDelta,
      turn_id: "turn-1",
      role: "assistant",
      content: "Hel",
    },
  ]);
  expect(
    processNotification(
      "item/completed",
      { item: { type: "agentMessage", id: "i", text: "Done." } },
      "turn-1",
    ),
  ).toEqual([
    {
      event: AdapterEventKind.AssistantMessage,
      turn_id: "turn-1",
      role: "assistant",
      content: "Done.",
    },
  ]);
});

test("command executions surface as tool calls and results", () => {
  const item = {
    type: "commandExecution",
    id: "c",
    command: "ls",
    aggregatedOutput: "a\nb",
  };
  expect(processNotification("item/started", { item }, "turn-1")).toEqual([
    {
      event: AdapterEventKind.ToolCall,
      turn_id: "turn-1",
      content: "ls",
      payload: item,
    },
  ]);
  expect(processNotification("item/completed", { item }, "turn-1")).toEqual([
    {
      event: AdapterEventKind.ToolResult,
      turn_id: "turn-1",
      role: "tool",
      content: "a\nb",
      payload: item,
    },
  ]);
});

test("reasoning items do not enter the conversation", () => {
  expect(
    processNotification(
      "item/completed",
      { item: { type: "reasoning", id: "r" } },
      "turn-1",
    ),
  ).toEqual([]);
});

function breakdown(
  input: number,
  output: number,
  cached: number,
  total = input + output,
) {
  return {
    inputTokens: input,
    outputTokens: output,
    cachedInputTokens: cached,
    reasoningOutputTokens: 0,
    totalTokens: total,
  };
}

function tokenUsage(
  total: ReturnType<typeof breakdown>,
  last: ReturnType<typeof breakdown>,
) {
  return {
    tokenUsage: { total, last, modelContextWindow: 200000 },
  };
}

test("a fresh thread's first update subtracts its own last request as the baseline", () => {
  const tokens = new TurnTokens();
  tokens.begin();
  const first = processNotification(
    "thread/tokenUsage/updated",
    tokenUsage(breakdown(100, 20, 40), breakdown(100, 20, 40, 90)),
    "turn-1",
    tokens,
  );
  expect(first).toEqual([
    {
      event: AdapterEventKind.Usage,
      turn_id: "turn-1",
      usage: {
        input_tokens: 100,
        output_tokens: 20,
        cached_input_tokens: 40,
        context_used_tokens: 90,
        context_limit_tokens: 200000,
      },
    },
  ]);
  const second = processNotification(
    "thread/tokenUsage/updated",
    tokenUsage(breakdown(250, 50, 90), breakdown(150, 30, 50, 180)),
    "turn-1",
    tokens,
  );
  expect(second[0]?.usage).toMatchObject({
    input_tokens: 250,
    output_tokens: 50,
    cached_input_tokens: 90,
  });
});

test("a resumed thread counts only what this turn used on top of prior totals", () => {
  const tokens = new TurnTokens();
  expect(
    processNotification(
      "thread/tokenUsage/updated",
      tokenUsage(breakdown(1000, 200, 500), breakdown(80, 10, 40)),
      "turn-1",
      tokens,
    )[0]?.usage?.input_tokens,
  ).toBeUndefined();
  tokens.begin();
  const during = processNotification(
    "thread/tokenUsage/updated",
    tokenUsage(breakdown(1150, 230, 560), breakdown(150, 30, 60)),
    "turn-1",
    tokens,
  );
  expect(during[0]?.usage).toMatchObject({
    input_tokens: 150,
    output_tokens: 30,
    cached_input_tokens: 60,
  });
});

test("without a last request the turn's tokens are left unreported", () => {
  const tokens = new TurnTokens();
  tokens.begin();
  const events = processNotification(
    "thread/tokenUsage/updated",
    {
      tokenUsage: {
        total: breakdown(100, 20, 40),
        modelContextWindow: 200000,
      },
    },
    "turn-1",
    tokens,
  );
  expect(events[0]?.usage?.input_tokens).toBeUndefined();
  expect(events[0]?.usage?.context_limit_tokens).toBe(200000);
});

test("a total that goes backwards is clamped to zero", () => {
  const tokens = new TurnTokens();
  tokens.consumed(
    tokenUsage(breakdown(100, 20, 40), breakdown(10, 2, 4)).tokenUsage,
  );
  tokens.begin();
  expect(
    tokens.consumed(
      tokenUsage(breakdown(50, 10, 20), breakdown(10, 2, 4)).tokenUsage,
    ),
  ).toEqual({ input: 0, output: 0, cached: 0 });
});

test("a retryable error notification is not surfaced", () => {
  expect(
    processNotification(
      "error",
      { error: { message: "rate" }, willRetry: true },
      "turn-1",
    ),
  ).toEqual([]);
  expect(
    processNotification("error", { error: { message: "boom" } }, "turn-1")[0]
      ?.content,
  ).toBe("boom");
});

test("turn/completed ends the turn, failure becomes an error", () => {
  expect(
    processTurnCompleted({ turn: { status: "completed" } }, "turn-1"),
  ).toEqual({
    event: AdapterEventKind.TurnComplete,
    turn_id: "turn-1",
  });
  const failed = processTurnCompleted(
    { turn: { status: "failed", error: { message: "nope" } } },
    "turn-1",
  );
  expect(failed.event).toBe(AdapterEventKind.Error);
  expect(failed.content).toBe("nope");
});

function defined<T>(value: T | null | undefined): T {
  if (value === null || value === undefined)
    throw new Error("expected a value");
  return value;
}

test("command approval becomes a permission request answered with accept/decline", () => {
  const result = defined(
    requestFromServer(
      7,
      "item/commandExecution/requestApproval",
      { command: "make clean", reason: "outside sandbox" },
      "turn-1",
    ),
  );
  expect(result.event.payload).toMatchObject({
    id: "7",
    kind: RequestKind.Permission,
    title: "Run: make clean (outside sandbox)",
  });
  expect(resultFor(result.pending, { allow: true })).toEqual({
    decision: "accept",
  });
  expect(resultFor(result.pending, { allow: false })).toEqual({
    decision: "decline",
  });
});

test("user-input requests become questions answered per question id", () => {
  const result = defined(
    requestFromServer(
      "q-1",
      "item/tool/requestUserInput",
      {
        questions: [
          {
            id: "db",
            question: "Which db?",
            options: [{ label: "pg" }, { label: "sqlite" }],
          },
        ],
      },
      undefined,
    ),
  );
  expect(result.pending.kind).toBe(RequestKind.Question);
  expect((result.event.payload as { title: string }).title).toBe(
    "Which db? [pg / sqlite]",
  );
  expect(resultFor(result.pending, { answer: "pg" })).toEqual({
    answers: { db: { answers: ["pg"] } },
  });
});

test("server requests Mitos cannot answer are not turned into requests", () => {
  expect(
    requestFromServer(1, "attestation/generate", {}, undefined),
  ).toBeNull();
});

test("Codex never asks for approval; plan is read-only, build is unrestricted", () => {
  expect(turnPolicy(ThreadMode.Plan)).toEqual({
    approvalPolicy: "never",
    sandboxPolicy: { type: "readOnly" },
  });
  expect(turnPolicy(ThreadMode.Build)).toEqual({
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
  });
  expect(threadSandbox(ThreadMode.Plan)).toBe("read-only");
  expect(threadSandbox(ThreadMode.Build)).toBe("danger-full-access");
});
