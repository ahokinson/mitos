import { expect, test } from "bun:test";
import {
  controlResponseFor,
  processStreamLine,
  requestFromControl,
} from "@harnesses/claude/streams.ts";
import { RequestKind } from "@protocol/asks.ts";
import { AdapterEventKind } from "@protocol/events.ts";

test("system/init carries the session id as a native_session_update", () => {
  const events = processStreamLine(
    {
      type: "system",
      subtype: "init",
      session_id: "abc-123",
      model: "claude-sonnet-5",
    },
    "turn-1",
  );
  expect(events).toEqual([
    {
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: "turn-1",
      native_session: "abc-123",
    },
  ]);
});

test("assistant text block becomes an assistant_message, tool_use becomes a tool_call", () => {
  const events = processStreamLine(
    {
      type: "assistant",
      session_id: "abc-123",
      message: {
        content: [
          { type: "text", text: "Let me check that file." },
          {
            type: "tool_use",
            id: "t1",
            name: "Read",
            input: { file_path: "a.ts" },
          },
        ],
        usage: { input_tokens: 10, output_tokens: 5 },
        model: "claude-sonnet-5",
      },
    },
    "turn-1",
  );
  expect(events).toEqual([
    {
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: "turn-1",
      native_session: "abc-123",
    },
    {
      event: AdapterEventKind.AssistantMessage,
      turn_id: "turn-1",
      role: "assistant",
      content: "Let me check that file.",
    },
    {
      event: AdapterEventKind.ToolCall,
      turn_id: "turn-1",
      role: "assistant",
      content: "Read",
      payload: {
        type: "tool_use",
        id: "t1",
        name: "Read",
        input: { file_path: "a.ts" },
      },
    },
    {
      event: AdapterEventKind.Usage,
      turn_id: "turn-1",
      usage: {
        input_tokens: 10,
        output_tokens: 5,
        cached_input_tokens: undefined,
        model: "claude-sonnet-5",
      },
    },
  ]);
});

test("user tool_result block becomes a tool_result event", () => {
  const events = processStreamLine(
    {
      type: "user",
      message: {
        content: [
          {
            type: "tool_result",
            tool_use_id: "t1",
            content: "file contents here",
          },
        ],
      },
    },
    "turn-1",
  );
  expect(events).toEqual([
    {
      event: AdapterEventKind.ToolResult,
      turn_id: "turn-1",
      role: "tool",
      content: "file contents here",
      payload: {
        type: "tool_result",
        tool_use_id: "t1",
        content: "file contents here",
      },
    },
  ]);
});

test("subagent messages (parent_tool_use_id set) are skipped", () => {
  const events = processStreamLine(
    {
      type: "assistant",
      parent_tool_use_id: "t1",
      message: { content: [{ type: "text", text: "subagent chatter" }] },
    },
    "turn-1",
  );
  expect(events).toEqual([]);
});

test("result line emits usage then turn_complete", () => {
  const events = processStreamLine(
    {
      type: "result",
      result: "done",
      usage: { input_tokens: 100, output_tokens: 40 },
    },
    "turn-1",
  );
  expect(events).toEqual([
    {
      event: AdapterEventKind.Usage,
      turn_id: "turn-1",
      usage: {
        input_tokens: 100,
        output_tokens: 40,
        cached_input_tokens: undefined,
        model: undefined,
      },
    },
    { event: AdapterEventKind.TurnComplete, turn_id: "turn-1" },
  ]);
});

test("result line carries total_cost_usd into usage", () => {
  const events = processStreamLine(
    {
      type: "result",
      total_cost_usd: 0.0421,
      usage: { input_tokens: 100, output_tokens: 40 },
    },
    "turn-1",
  );
  expect(events[0]?.usage).toMatchObject({
    input_tokens: 100,
    output_tokens: 40,
    cost_usd: 0.0421,
  });
});

test("result line with cost but no usage object still emits cost", () => {
  const events = processStreamLine(
    { type: "result", total_cost_usd: 0.5 },
    "turn-1",
  );
  expect(events.map((event) => event.event)).toEqual([
    AdapterEventKind.Usage,
    AdapterEventKind.TurnComplete,
  ]);
  expect(events[0]?.usage?.cost_usd).toBe(0.5);
});

test("result line without cost leaves cost_usd unset", () => {
  const events = processStreamLine(
    { type: "result", usage: { input_tokens: 1, output_tokens: 1 } },
    "turn-1",
  );
  expect(events[0]?.usage?.cost_usd).toBeUndefined();
});

test("a non-object or unrecognized line produces no events", () => {
  expect(processStreamLine("not an object", "turn-1")).toEqual([]);
  expect(processStreamLine({ type: "stream_event" }, "turn-1")).toEqual([]);
});

function defined<T>(value: T | null | undefined): T {
  if (value === null || value === undefined)
    throw new Error("expected a value");
  return value;
}

// Synthetic fixtures: shaped after the Agent SDK's `can_use_tool` control
// protocol, not captured from a live CLI.
const bashRequest = {
  type: "control_request",
  request_id: "req-1",
  request: {
    subtype: "can_use_tool",
    tool_name: "Bash",
    input: { command: "make clean" },
  },
};

test("a can_use_tool control_request becomes a permission request event", () => {
  const result = requestFromControl(bashRequest, "turn-1");
  expect(result?.event).toEqual({
    event: AdapterEventKind.Request,
    turn_id: "turn-1",
    payload: {
      id: "req-1",
      kind: RequestKind.Permission,
      title: "Bash make clean",
      tool_name: "Bash",
      input: { command: "make clean" },
    },
  });
});

test("ExitPlanMode is a plan approval and AskUserQuestion is a question", () => {
  const plan = defined(
    requestFromControl(
      {
        ...bashRequest,
        request: {
          subtype: "can_use_tool",
          tool_name: "ExitPlanMode",
          input: { plan: "1. do\n2. it" },
        },
      },
      undefined,
    ),
  );
  expect(plan.pending.kind).toBe(RequestKind.PlanApproval);
  expect((plan.event.payload as { title: string }).title).toBe("1. do 2. it");

  const question = defined(
    requestFromControl(
      {
        ...bashRequest,
        request: {
          subtype: "can_use_tool",
          tool_name: "AskUserQuestion",
          input: {
            questions: [
              {
                question: "Which db?",
                options: [{ label: "pg" }, { label: "sqlite" }],
              },
            ],
          },
        },
      },
      undefined,
    ),
  );
  expect(question.pending.kind).toBe(RequestKind.Question);
  expect((question.event.payload as { title: string }).title).toBe(
    "Which db? [pg / sqlite]",
  );
});

test("other control traffic is not a request", () => {
  expect(
    requestFromControl(
      {
        type: "control_request",
        request_id: "x",
        request: { subtype: "interrupt" },
      },
      undefined,
    ),
  ).toBeNull();
  expect(requestFromControl({ type: "assistant" }, undefined)).toBeNull();
});

test("permission answers map to allow/deny control responses", () => {
  const { pending } = defined(requestFromControl(bashRequest, undefined));
  expect(controlResponseFor(pending, { allow: true })).toEqual({
    type: "control_response",
    response: {
      subtype: "success",
      request_id: "req-1",
      response: { behavior: "allow", updatedInput: { command: "make clean" } },
    },
  });
  const denied = controlResponseFor(pending, {
    allow: false,
    message: "too risky",
  }) as {
    response: { response: unknown };
  };
  expect(denied.response.response).toEqual({
    behavior: "deny",
    message: "too risky",
  });
});

test("plan rejection carries feedback, with a default when none is given", () => {
  const pending = {
    cliRequestId: "req-2",
    kind: RequestKind.PlanApproval,
    toolName: "ExitPlanMode",
    input: { plan: "p" },
  };
  const rejected = controlResponseFor(pending, {
    approved: false,
    feedback: "split it up",
  }) as {
    response: { response: unknown };
  };
  expect(rejected.response.response).toEqual({
    behavior: "deny",
    message: "split it up",
  });
  const bare = controlResponseFor(pending, { approved: false }) as {
    response: { response: { message: string } };
  };
  expect(bare.response.response.message).toBe("The user rejected the plan.");
});

test("a question answer is attached under its question text", () => {
  const pending = {
    cliRequestId: "req-3",
    kind: RequestKind.Question,
    toolName: "AskUserQuestion",
    input: { questions: [{ question: "Which db?" }] },
  };
  const reply = controlResponseFor(pending, { answer: "pg" }) as {
    response: { response: unknown };
  };
  expect(reply.response.response).toEqual({
    behavior: "allow",
    updatedInput: {
      questions: [{ question: "Which db?" }],
      answers: { "Which db?": "pg" },
    },
  });
});
