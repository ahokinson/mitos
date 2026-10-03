import { expect, test } from "bun:test";

import { driveTurn, type Link } from "@harnesses/hermes/acps.ts";
import type { AnswerMessage } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";

const SESSION = "sess-1";

type Sent = Record<string, unknown>;

/** A scripted ACP agent: `respond` sees each client message and returns the
 * lines the agent sends back. */
function fakeAgent(respond: (message: Sent, agent: Agent) => Sent[]) {
  const sent: Sent[] = [];
  const queue: string[] = [];
  let wake: (() => void) | undefined;
  let closed = false;
  const agent: Agent = {
    sent,
    push(message) {
      queue.push(JSON.stringify(message));
      wake?.();
    },
    close() {
      closed = true;
      wake?.();
    },
  };
  const link: Link = {
    write(line) {
      const message = JSON.parse(line) as Sent;
      sent.push(message);
      queueMicrotask(() => {
        for (const reply of respond(message, agent)) agent.push(reply);
      });
    },
    lines: (async function* () {
      for (;;) {
        const next = queue.shift();
        if (next !== undefined) yield next;
        else if (closed) return;
        else await new Promise<void>((resolve) => (wake = resolve));
      }
    })(),
  };
  return { link, agent };
}

type Agent = {
  sent: Sent[];
  push(message: Sent): void;
  close(): void;
};

function result(message: Sent, value: unknown): Sent {
  return { jsonrpc: "2.0", id: message.id, result: value };
}

function notification(update: Record<string, unknown>, sessionId = SESSION) {
  return {
    jsonrpc: "2.0",
    method: "session/update",
    params: { sessionId, update },
  };
}

const CAPABILITIES = {
  protocolVersion: 1,
  agentCapabilities: {
    loadSession: true,
    sessionCapabilities: { resume: {} },
  },
};

async function* noAnswers(): AsyncGenerator<AnswerMessage> {}

async function run(
  link: Link,
  session: string | null = null,
  answers: AsyncIterable<AnswerMessage> = noAnswers(),
  diagnostic?: () => Promise<string>,
): Promise<AdapterEvent[]> {
  const events: AdapterEvent[] = [];
  await driveTurn(
    link,
    { workdir: "/work", text: "hello", session, turnId: "turn-1" },
    { answers, emit: (event) => events.push(event), diagnostic },
  );
  return events;
}

function standard(message: Sent, agent: Agent): Sent[] {
  switch (message.method) {
    case "initialize":
      return [result(message, CAPABILITIES)];
    case "session/new":
      return [result(message, { sessionId: SESSION })];
    case "session/prompt":
      agent.push(
        notification({
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "Hi there" },
        }),
      );
      return [result(message, { stopReason: "end_turn" })];
    default:
      return [];
  }
}

test("a new session streams a turn and ends with one turn_complete", async () => {
  const { link, agent } = fakeAgent(standard);
  const events = await run(link);
  expect(events.map((e) => e.event)).toEqual([
    AdapterEventKind.NativeSessionUpdate,
    AdapterEventKind.AssistantDelta,
    AdapterEventKind.AssistantMessage,
    AdapterEventKind.TurnComplete,
  ]);
  expect(events[0]?.native_session).toBe(SESSION);
  const init = agent.sent[0];
  expect(init).toMatchObject({
    jsonrpc: "2.0",
    method: "initialize",
    params: { protocolVersion: 1 },
  });
  expect(agent.sent[1]).toMatchObject({
    method: "session/new",
    params: { cwd: "/work", mcpServers: [] },
  });
  expect(agent.sent[2]).toMatchObject({
    method: "session/prompt",
    params: { sessionId: SESSION, prompt: [{ type: "text", text: "hello" }] },
  });
});

test("an existing session is resumed without creating one", async () => {
  const { link, agent } = fakeAgent((message, a) =>
    message.method === "session/resume"
      ? [result(message, null)]
      : standard(message, a),
  );
  const events = await run(link, SESSION);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.TurnComplete);
  const methods = agent.sent.map((m) => m.method);
  expect(methods).toContain("session/resume");
  expect(methods).not.toContain("session/new");
  expect(methods).not.toContain("session/load");
});

test("load replay is not part of the turn when only session/load exists", async () => {
  const { link, agent } = fakeAgent((message, a) => {
    if (message.method === "initialize")
      return [
        result(message, {
          protocolVersion: 1,
          agentCapabilities: { loadSession: true },
        }),
      ];
    if (message.method === "session/load") {
      a.push(
        notification({
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "old history" },
        }),
      );
      return [result(message, null)];
    }
    return standard(message, a);
  });
  const events = await run(link, SESSION);
  expect(agent.sent.map((m) => m.method)).toContain("session/load");
  expect(events.some((e) => e.content === "old history")).toBe(false);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.TurnComplete);
});

test("an agent that cannot resume is reported as an error", async () => {
  const { link } = fakeAgent((message) =>
    message.method === "initialize"
      ? [result(message, { protocolVersion: 1, agentCapabilities: {} })]
      : [],
  );
  const events = await run(link, SESSION);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.Error);
  expect(events.at(-1)?.content).toBe("hermes cannot resume sessions");
});

test("a permission request is answered with the chosen option", async () => {
  async function* answers(): AsyncGenerator<AnswerMessage> {
    await Bun.sleep(30);
    yield {
      action: "answer",
      request_id: "9",
      response: { allow: true },
    } as AnswerMessage;
  }
  const { link, agent } = fakeAgent((message, a) => {
    if (message.method === "session/prompt") {
      a.push({
        jsonrpc: "2.0",
        id: 9,
        method: "session/request_permission",
        params: {
          sessionId: SESSION,
          toolCall: { toolCallId: "t1", title: "Run ls" },
          options: [
            { optionId: "ao", name: "Allow", kind: "allow_once" },
            { optionId: "ro", name: "Reject", kind: "reject_once" },
          ],
        },
      });
      setTimeout(
        () => a.push(result(message, { stopReason: "end_turn" })),
        150,
      );
      return [];
    }
    return standard(message, a);
  });
  const events = await run(link, null, answers());
  expect(events.map((e) => e.event)).toContain(AdapterEventKind.Request);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.TurnComplete);
  const reply = agent.sent.find((m) => m.id === 9 && "result" in m);
  expect(reply?.result).toEqual({
    outcome: { outcome: "selected", optionId: "ao" },
  });
});

test("an unknown agent request is rejected", async () => {
  const { link, agent } = fakeAgent((message, a) => {
    if (message.method === "session/prompt") {
      a.push({
        jsonrpc: "2.0",
        id: 5,
        method: "fs/read_text_file",
        params: { sessionId: SESSION, path: "/etc/passwd" },
      });
    }
    return standard(message, a);
  });
  await run(link);
  const rejection = agent.sent.find((m) => m.id === 5);
  expect(rejection?.error).toMatchObject({ code: -32601 });
});

test("an rpc error is the single terminal error with the server's details", async () => {
  const { link } = fakeAgent((message) => {
    if (message.method === "initialize") return [result(message, CAPABILITIES)];
    return [
      {
        jsonrpc: "2.0",
        id: message.id,
        error: {
          code: -32603,
          message: "Internal error",
          data: { details: "No LLM provider configured" },
        },
      },
    ];
  });
  const events = await run(link);
  const terminal = events.filter(
    (e) =>
      e.event === AdapterEventKind.Error ||
      e.event === AdapterEventKind.TurnComplete,
  );
  expect(terminal).toHaveLength(1);
  expect(terminal[0]?.content).toBe(
    "Internal error: No LLM provider configured",
  );
});

test("a process that exits mid-turn reports why from its diagnostic", async () => {
  const { link, agent } = fakeAgent((message, a) => {
    if (message.method === "session/prompt") {
      a.close();
      return [];
    }
    return standard(message, a);
  });
  const events = await run(link, null, noAnswers(), async () => "ERROR boom");
  expect(events.at(-1)?.event).toBe(AdapterEventKind.Error);
  expect(events.at(-1)?.content).toBe("hermes acp exited: ERROR boom");
  expect(agent.sent.length).toBeGreaterThan(0);
});

test("a stop reason other than end_turn is an error", async () => {
  const { link } = fakeAgent((message, a) =>
    message.method === "session/prompt"
      ? [result(message, { stopReason: "refusal" })]
      : standard(message, a),
  );
  const events = await run(link);
  expect(events.at(-1)).toMatchObject({
    event: AdapterEventKind.Error,
    content: "Hermes refused this turn",
  });
});
