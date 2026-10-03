import { afterEach, expect, test } from "bun:test";

import { driveTurn } from "@harnesses/opencode/servers.ts";
import { ThreadMode } from "@protocol/actions.ts";
import type { AnswerMessage } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";

const SESSION = "ses_fake";
const encoder = new TextEncoder();

type Fake = {
  base: string;
  requests: Array<{ method: string; path: string; body: unknown }>;
  push(type: string, properties: Record<string, unknown>): void;
  close(): void;
};

const servers: Array<ReturnType<typeof Bun.serve>> = [];

afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true);
});

function fakeServer(onPrompt: (fake: Fake) => void): Fake {
  let sse: ReadableStreamDefaultController<Uint8Array> | undefined;
  const requests: Fake["requests"] = [];
  const fake: Fake = {
    base: "",
    requests,
    push: (type, properties) =>
      sse?.enqueue(
        encoder.encode(
          `data: ${JSON.stringify({ type, properties: { sessionID: SESSION, ...properties } })}\n\n`,
        ),
      ),
    close: () => sse?.close(),
  };
  const server = Bun.serve({
    port: 0,
    async fetch(request) {
      const url = new URL(request.url);
      const body =
        request.method === "POST"
          ? await request.json().catch(() => null)
          : null;
      requests.push({
        method: request.method,
        path: url.pathname,
        body,
      });
      if (request.headers.get("authorization") === null)
        return new Response("unauthorized", { status: 401 });
      if (url.pathname === "/event") {
        return new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              sse = controller;
              controller.enqueue(
                encoder.encode(
                  `data: ${JSON.stringify({ type: "server.connected", properties: {} })}\n\n`,
                ),
              );
            },
          }),
          { headers: { "content-type": "text/event-stream" } },
        );
      }
      if (url.pathname === "/session") return Response.json({ id: SESSION });
      if (url.pathname.endsWith("/prompt_async")) {
        queueMicrotask(() => onPrompt(fake));
        return new Response(null, { status: 204 });
      }
      return Response.json(true);
    },
  });
  servers.push(server);
  fake.base = `http://127.0.0.1:${server.port}`;
  return fake;
}

async function* noAnswers(): AsyncGenerator<AnswerMessage> {}

function options(overrides: Partial<Parameters<typeof driveTurn>[1]> = {}) {
  return {
    workdir: "/work",
    text: "hello",
    session: null,
    mode: ThreadMode.Build,
    turnId: "turn-1",
    ...overrides,
  };
}

async function run(
  fake: Fake,
  turn = options(),
  answers: AsyncIterable<AnswerMessage> = noAnswers(),
): Promise<AdapterEvent[]> {
  const events: AdapterEvent[] = [];
  await driveTurn({ base: fake.base, password: "pw" }, turn, {
    answers,
    emit: (event) => events.push(event),
  });
  return events;
}

test("a new session streams a turn and ends with one turn_complete", async () => {
  const fake = fakeServer((f) => {
    f.push("message.updated", {
      info: { id: "msg_a", role: "assistant", modelID: "m" },
    });
    f.push("message.part.updated", {
      part: {
        id: "prt_1",
        type: "text",
        messageID: "msg_a",
        text: "done",
        time: { start: 1, end: 2 },
      },
    });
    f.push("session.idle", {});
    f.push("session.idle", {});
  });
  const events = await run(fake);
  expect(events.map((e) => e.event)).toEqual([
    AdapterEventKind.NativeSessionUpdate,
    AdapterEventKind.AssistantMessage,
    AdapterEventKind.TurnComplete,
  ]);
  expect(events[0]?.native_session).toBe(SESSION);
  const prompt = fake.requests.find((r) => r.path.endsWith("/prompt_async"));
  expect(prompt?.body).toEqual({
    agent: "build",
    parts: [{ type: "text", text: "hello" }],
  });
});

test("an existing session is resumed without creating one", async () => {
  const fake = fakeServer((f) => {
    f.push("message.updated", { info: { id: "msg_a", role: "assistant" } });
    f.push("session.idle", {});
  });
  const events = await run(
    fake,
    options({ session: SESSION, mode: ThreadMode.Plan }),
  );
  expect(events.at(-1)?.event).toBe(AdapterEventKind.TurnComplete);
  expect(fake.requests.some((r) => r.path === "/session")).toBe(false);
  expect(
    fake.requests.find((r) => r.path.endsWith("/prompt_async"))?.body,
  ).toMatchObject({ agent: "plan" });
});

test("a permission ask is answered over the permission endpoint", async () => {
  async function* answers(): AsyncGenerator<AnswerMessage> {
    await Bun.sleep(30);
    yield {
      action: "answer",
      request_id: "per_1",
      response: { allow: true },
    } as AnswerMessage;
  }
  const fake = fakeServer((f) => {
    f.push("permission.asked", {
      id: "per_1",
      permission: "bash",
      patterns: ["ls"],
    });
    setTimeout(() => f.push("session.idle", {}), 120);
  });
  const events = await run(fake, options(), answers());
  expect(events.map((e) => e.event)).toContain(AdapterEventKind.Request);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.TurnComplete);
  const reply = fake.requests.find((r) => r.path === "/permission/per_1/reply");
  expect(reply?.body).toEqual({ reply: "once" });
});

test("a session error ends the turn with exactly one error", async () => {
  const fake = fakeServer((f) => {
    f.push("session.error", {
      error: { name: "UnknownError", data: { message: "boom" } },
    });
    f.push("session.idle", {});
  });
  const events = await run(fake);
  const terminal = events.filter(
    (e) =>
      e.event === AdapterEventKind.Error ||
      e.event === AdapterEventKind.TurnComplete,
  );
  expect(terminal).toHaveLength(1);
  expect(terminal[0]?.content).toBe("boom");
});

test("a stream that closes early is reported as an error", async () => {
  const fake = fakeServer((f) => f.close());
  const events = await run(fake);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.Error);
  expect(events.at(-1)?.content).toContain("event stream ended");
});

test("an unreachable server is reported as an error", async () => {
  const fake = fakeServer(() => {});
  fake.base = fake.base.replace(/:\d+$/, ":1");
  const events = await run(fake);
  expect(events.at(-1)?.event).toBe(AdapterEventKind.Error);
});
