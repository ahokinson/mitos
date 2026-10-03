import { expect, test } from "bun:test";

import {
  agentFor,
  replyFor,
  SessionStream,
} from "@harnesses/opencode/streams.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { RequestKind } from "@protocol/asks.ts";
import { AdapterEventKind } from "@protocol/events.ts";

const SESSION = "ses_1";

function stream() {
  return new SessionStream(SESSION, "turn-1");
}

function event(type: string, properties: Record<string, unknown>) {
  return { type, properties: { sessionID: SESSION, ...properties } };
}

function assistantMessage(stream: SessionStream, id = "msg_a") {
  stream.process(
    event("message.updated", {
      info: { id, role: "assistant", modelID: "glm-5.3" },
    }),
  );
}

test("plan mode runs the plan agent and build runs build", () => {
  expect(agentFor(ThreadMode.Plan)).toBe("plan");
  expect(agentFor(ThreadMode.Build)).toBe("build");
});

test("events from other sessions are ignored", () => {
  const step = stream().process({
    type: "session.idle",
    properties: { sessionID: "ses_other" },
  });
  expect(step).toEqual({ events: [], done: false });
});

test("assistant text deltas stream and the finished part is one message", () => {
  const s = stream();
  assistantMessage(s);
  s.process(
    event("message.part.updated", {
      part: { id: "prt_1", type: "text", messageID: "msg_a", text: "" },
    }),
  );
  const delta = s.process(
    event("message.part.delta", {
      messageID: "msg_a",
      partID: "prt_1",
      field: "text",
      delta: "Hel",
    }),
  );
  expect(delta.events).toEqual([
    {
      event: AdapterEventKind.AssistantDelta,
      turn_id: "turn-1",
      role: "assistant",
      content: "Hel",
    },
  ]);
  const part = {
    id: "prt_1",
    type: "text",
    messageID: "msg_a",
    text: "Hello",
    time: { start: 1, end: 2 },
  };
  const done = s.process(event("message.part.updated", { part }));
  expect(done.events).toHaveLength(1);
  expect(done.events[0]?.event).toBe(AdapterEventKind.AssistantMessage);
  expect(done.events[0]?.content).toBe("Hello");
  expect(s.process(event("message.part.updated", { part })).events).toEqual([]);
});

test("user text and unfinished text produce nothing", () => {
  const s = stream();
  s.process(event("message.updated", { info: { id: "msg_u", role: "user" } }));
  const user = s.process(
    event("message.part.updated", {
      part: {
        id: "prt_u",
        type: "text",
        messageID: "msg_u",
        text: "hi",
        time: { start: 1, end: 2 },
      },
    }),
  );
  expect(user.events).toEqual([]);
  assistantMessage(s);
  const unfinished = s.process(
    event("message.part.updated", {
      part: {
        id: "prt_a",
        type: "text",
        messageID: "msg_a",
        text: "par",
        time: { start: 1 },
      },
    }),
  );
  expect(unfinished.events).toEqual([]);
});

test("a tool part yields one call and one result", () => {
  const s = stream();
  const part = (state: Record<string, unknown>) =>
    event("message.part.updated", {
      part: {
        id: "prt_t",
        type: "tool",
        callID: "call_1",
        tool: "bash",
        state,
      },
    });
  const pending = s.process(part({ status: "pending", input: {}, raw: "" }));
  expect(pending.events).toEqual([]);
  const running = s.process(
    part({ status: "running", input: { command: "ls" }, time: { start: 1 } }),
  );
  expect(running.events.map((e) => e.event)).toEqual([
    AdapterEventKind.ToolCall,
  ]);
  expect(running.events[0]?.content).toBe("bash: ls");
  const completed = s.process(
    part({
      status: "completed",
      input: { command: "ls" },
      output: "a\nb",
      time: { start: 1, end: 2 },
    }),
  );
  expect(completed.events.map((e) => e.event)).toEqual([
    AdapterEventKind.ToolResult,
  ]);
  expect(completed.events[0]?.content).toBe("a\nb");
  expect(
    s.process(
      part({
        status: "completed",
        input: {},
        output: "a\nb",
        time: { start: 1, end: 2 },
      }),
    ).events,
  ).toEqual([]);
});

test("a failed tool reports its error as the result", () => {
  const step = stream().process(
    event("message.part.updated", {
      part: {
        id: "prt_t",
        type: "tool",
        callID: "call_1",
        tool: "edit",
        state: {
          status: "error",
          input: { filePath: "a.ts" },
          error: "no such file",
          time: { start: 1, end: 2 },
        },
      },
    }),
  );
  expect(step.events.map((e) => e.event)).toEqual([
    AdapterEventKind.ToolCall,
    AdapterEventKind.ToolResult,
  ]);
  expect(step.events[1]?.content).toBe("no such file");
});

test("step-finish accumulates usage across steps", () => {
  const s = stream();
  assistantMessage(s);
  const finish = (id: string, tokens: Record<string, unknown>) =>
    s.process(
      event("message.part.updated", {
        part: { id, type: "step-finish", messageID: "msg_a", tokens },
      }),
    );
  finish("prt_s1", {
    total: 150,
    input: 100,
    output: 50,
    reasoning: 0,
    cache: { read: 10, write: 0 },
  });
  const second = finish("prt_s2", {
    total: 260,
    input: 200,
    output: 60,
    reasoning: 0,
    cache: { read: 20, write: 0 },
  });
  expect(second.events[0]?.usage).toEqual({
    input_tokens: 300,
    output_tokens: 110,
    cached_input_tokens: 30,
    context_used_tokens: 260,
    model: "glm-5.3",
  });
  const repeat = finish("prt_s2", {
    total: 260,
    input: 200,
    output: 60,
    reasoning: 0,
    cache: { read: 20, write: 0 },
  });
  expect(repeat.events[0]?.usage?.input_tokens).toBe(300);
});

test("step-finish costs sum across steps and a repeated step is not double counted", () => {
  const s = stream();
  assistantMessage(s);
  const finish = (id: string, cost: number | undefined) =>
    s.process(
      event("message.part.updated", {
        part: {
          id,
          type: "step-finish",
          messageID: "msg_a",
          cost,
          tokens: {
            input: 1,
            output: 1,
            reasoning: 0,
            cache: { read: 0, write: 0 },
          },
        },
      }),
    );
  expect(
    finish("prt_s0", undefined).events[0]?.usage?.cost_usd,
  ).toBeUndefined();
  finish("prt_s1", 0.25);
  expect(finish("prt_s2", 0.5).events[0]?.usage?.cost_usd).toBeCloseTo(0.75);
  expect(finish("prt_s2", 0.5).events[0]?.usage?.cost_usd).toBeCloseTo(0.75);
});

test("idle before any activity is not completion", () => {
  expect(stream().process(event("session.idle", {})).done).toBe(false);
});

test("idle after activity completes the turn exactly once", () => {
  const s = stream();
  assistantMessage(s);
  const step = s.process(event("session.idle", {}));
  expect(step).toEqual({
    events: [{ event: AdapterEventKind.TurnComplete, turn_id: "turn-1" }],
    done: true,
  });
});

test("an ask counts as activity so a later idle completes the turn", () => {
  const s = stream();
  s.process(event("permission.asked", { id: "per_1", permission: "bash" }));
  expect(s.process(event("session.idle", {})).done).toBe(true);
});

test("a session status of idle completes like session.idle", () => {
  const s = stream();
  s.process(event("session.status", { status: { type: "busy" } }));
  const step = s.process(event("session.status", { status: { type: "idle" } }));
  expect(step.done).toBe(true);
  expect(step.events[0]?.event).toBe(AdapterEventKind.TurnComplete);
});

test("a session error is the terminal event", () => {
  const step = stream().process(
    event("session.error", {
      error: { name: "ProviderAuthError", data: { message: "bad key" } },
    }),
  );
  expect(step.done).toBe(true);
  expect(step.events[0]?.event).toBe(AdapterEventKind.Error);
  expect(step.events[0]?.content).toBe("bad key");
});

test("a permission ask becomes a permission request and maps to a reply", () => {
  const s = stream();
  const step = s.process(
    event("permission.asked", {
      id: "per_1",
      permission: "bash",
      patterns: ["rm -rf build"],
    }),
  );
  const request = step.events[0];
  expect(request?.event).toBe(AdapterEventKind.Request);
  expect(request?.payload).toMatchObject({
    id: "per_1",
    kind: RequestKind.Permission,
    title: "bash: rm -rf build",
  });
  const pending = s.pendingFor(request as never);
  expect(pending).toEqual({
    id: "per_1",
    kind: RequestKind.Permission,
    questionCount: 0,
  });
  if (!pending) throw new Error("expected a pending permission");
  expect(replyFor(pending, { allow: true })).toEqual({
    path: "/permission/per_1/reply",
    body: { reply: "once" },
  });
  expect(replyFor(pending, { allow: false }).body).toEqual({
    reply: "reject",
  });
});

test("a question ask lists options and answers every question", () => {
  const s = stream();
  const step = s.process(
    event("question.asked", {
      id: "que_1",
      questions: [
        {
          question: "Which db?",
          header: "DB",
          options: [
            { label: "pg", description: "" },
            { label: "sqlite", description: "" },
          ],
        },
        { question: "Why?", header: "Why", options: [] },
      ],
    }),
  );
  const request = step.events[0];
  expect(request?.payload).toMatchObject({
    kind: RequestKind.Question,
    title: "Which db? [pg / sqlite]",
  });
  const pending = s.pendingFor(request as never);
  if (!pending) throw new Error("expected a pending question");
  expect(pending.questionCount).toBe(2);
  expect(replyFor(pending, { answer: "pg" })).toEqual({
    path: "/question/que_1/reply",
    body: { answers: [["pg"], ["pg"]] },
  });
});
