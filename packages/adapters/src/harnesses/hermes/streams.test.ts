import { expect, test } from "bun:test";

import {
  AcpStream,
  outcomeFor,
  permissionRequest,
} from "@harnesses/hermes/streams.ts";
import { RequestKind } from "@protocol/asks.ts";
import { AdapterEventKind } from "@protocol/events.ts";

const SESSION = "sess-1";

function stream() {
  return new AcpStream(SESSION, "turn-1");
}

function update(body: Record<string, unknown>, sessionId = SESSION) {
  return { sessionId, update: body };
}

function chunk(text: string) {
  return update({
    sessionUpdate: "agent_message_chunk",
    content: { type: "text", text },
  });
}

test("message chunks stream as deltas", () => {
  expect(stream().update(chunk("Hel"))).toEqual([
    {
      event: AdapterEventKind.AssistantDelta,
      turn_id: "turn-1",
      role: "assistant",
      content: "Hel",
    },
  ]);
});

test("updates for another session and reasoning chunks are ignored", () => {
  const s = stream();
  expect(
    s.update(
      update(
        {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "x" },
        },
        "other",
      ),
    ),
  ).toEqual([]);
  expect(
    s.update(
      update({
        sessionUpdate: "agent_thought_chunk",
        content: { type: "text", text: "hmm" },
      }),
    ),
  ).toEqual([]);
});

test("non-text content is ignored", () => {
  expect(
    stream().update(
      update({
        sessionUpdate: "agent_message_chunk",
        content: { type: "image", data: "..." },
      }),
    ),
  ).toEqual([]);
});

test("the turn ends with the whole text as one message then turn_complete", () => {
  const s = stream();
  s.update(chunk("Hel"));
  s.update(chunk("lo"));
  const events = s.finish({ stopReason: "end_turn" });
  expect(events.map((e) => e.event)).toEqual([
    AdapterEventKind.AssistantMessage,
    AdapterEventKind.TurnComplete,
  ]);
  expect(events[0]?.content).toBe("Hello");
});

test("text before a tool call is flushed as a message first", () => {
  const s = stream();
  s.update(chunk("Let me look."));
  const events = s.update(
    update({
      sessionUpdate: "tool_call",
      toolCallId: "t1",
      title: "Read a.ts",
    }),
  );
  expect(events.map((e) => e.event)).toEqual([
    AdapterEventKind.AssistantMessage,
    AdapterEventKind.ToolCall,
  ]);
  expect(events[0]?.content).toBe("Let me look.");
  expect(events[1]?.content).toBe("Read a.ts");
  const end = s.finish({ stopReason: "end_turn" });
  expect(end.map((e) => e.event)).toEqual([AdapterEventKind.TurnComplete]);
});

test("a tool call yields one call and one result with its text output", () => {
  const s = stream();
  const call = update({
    sessionUpdate: "tool_call",
    toolCallId: "t1",
    title: "ls",
  });
  expect(s.update(call)).toHaveLength(1);
  expect(s.update(call)).toEqual([]);
  const progress = s.update(
    update({
      sessionUpdate: "tool_call_update",
      toolCallId: "t1",
      status: "in_progress",
    }),
  );
  expect(progress).toEqual([]);
  const done = update({
    sessionUpdate: "tool_call_update",
    toolCallId: "t1",
    status: "completed",
    content: [{ type: "content", content: { type: "text", text: "a\nb" } }],
  });
  const result = s.update(done);
  expect(result.map((e) => e.event)).toEqual([AdapterEventKind.ToolResult]);
  expect(result[0]?.content).toBe("a\nb");
  expect(s.update(done)).toEqual([]);
});

test("a failed tool falls back to raw output then title", () => {
  const s = stream();
  const raw = s.update(
    update({
      sessionUpdate: "tool_call_update",
      toolCallId: "t1",
      status: "failed",
      rawOutput: "permission denied",
    }),
  );
  expect(raw[0]?.content).toBe("permission denied");
  const titled = s.update(
    update({
      sessionUpdate: "tool_call_update",
      toolCallId: "t2",
      status: "failed",
      title: "Edit a.ts",
    }),
  );
  expect(titled[0]?.content).toBe("Edit a.ts");
});

test("usage_update becomes context usage", () => {
  expect(
    stream().update(
      update({ sessionUpdate: "usage_update", used: 1200, size: 128000 }),
    ),
  ).toEqual([
    {
      event: AdapterEventKind.Usage,
      turn_id: "turn-1",
      usage: { context_used_tokens: 1200, context_limit_tokens: 128000 },
    },
  ]);
});

test("usage_update carries USD cost and ignores other currencies", () => {
  const usd = stream().update(
    update({
      sessionUpdate: "usage_update",
      used: 10,
      size: 100,
      cost: { amount: 0.0123, currency: "USD" },
    }),
  );
  expect(usd[0]?.usage?.cost_usd).toBe(0.0123);
  const eur = stream().update(
    update({
      sessionUpdate: "usage_update",
      used: 10,
      size: 100,
      cost: { amount: 0.5, currency: "EUR" },
    }),
  );
  expect(eur[0]?.usage?.cost_usd).toBeUndefined();
  const onlyCost = stream().update(
    update({
      sessionUpdate: "usage_update",
      cost: { amount: 1, currency: "USD" },
    }),
  );
  expect(onlyCost[0]?.usage?.cost_usd).toBe(1);
});

test("a prompt result's token usage is emitted before completion", () => {
  const events = stream().finish({
    stopReason: "end_turn",
    usage: { inputTokens: 10, outputTokens: 4, cachedReadTokens: 2 },
  });
  expect(events.map((e) => e.event)).toEqual([
    AdapterEventKind.Usage,
    AdapterEventKind.TurnComplete,
  ]);
  expect(events[0]?.usage).toEqual({
    input_tokens: 10,
    output_tokens: 4,
    cached_input_tokens: 2,
  });
});

test("a non end_turn stop reason is an error", () => {
  const cancelled = stream().finish({ stopReason: "cancelled" });
  expect(cancelled.at(-1)?.event).toBe(AdapterEventKind.Error);
  expect(cancelled.at(-1)?.content).toBe("Hermes turn was cancelled");
  const refused = stream().finish({ stopReason: "refusal" });
  expect(refused.at(-1)?.content).toBe("Hermes refused this turn");
});

const OPTIONS = [
  { optionId: "ao", name: "Allow once", kind: "allow_once" },
  { optionId: "aa", name: "Always", kind: "allow_always" },
  { optionId: "ro", name: "Reject", kind: "reject_once" },
];

test("a permission request becomes a request and answers with an option id", () => {
  const request = permissionRequest(
    7,
    {
      toolCall: { toolCallId: "t1", title: "Run rm -rf build" },
      options: OPTIONS,
    },
    "turn-1",
  );
  expect(request?.event.event).toBe(AdapterEventKind.Request);
  expect(request?.event.payload).toMatchObject({
    id: "7",
    kind: RequestKind.Permission,
    title: "Run rm -rf build",
  });
  if (!request) throw new Error("expected a permission request");
  expect(outcomeFor(request.pending, { allow: true })).toEqual({
    outcome: { outcome: "selected", optionId: "ao" },
  });
  expect(outcomeFor(request.pending, { allow: false })).toEqual({
    outcome: { outcome: "selected", optionId: "ro" },
  });
});

test("allow falls back to allow_always and a missing reject cancels", () => {
  const onlyAlways = permissionRequest(
    1,
    { options: [{ optionId: "aa", kind: "allow_always" }] },
    undefined,
  );
  if (!onlyAlways) throw new Error("expected a permission request");
  expect(outcomeFor(onlyAlways.pending, { allow: true })).toEqual({
    outcome: { outcome: "selected", optionId: "aa" },
  });
  expect(outcomeFor(onlyAlways.pending, { allow: false })).toEqual({
    outcome: { outcome: "cancelled" },
  });
});

test("a permission request with no options cannot be answered", () => {
  expect(permissionRequest(1, { options: [] }, undefined)).toBeNull();
});
