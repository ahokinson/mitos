import { expect, test } from "bun:test";

import { RpcInboundKind, RpcPeer } from "@rpc/peers.ts";

function peer() {
  const written: Array<Record<string, unknown>> = [];
  const rpc = new RpcPeer((line) => written.push(JSON.parse(line)));
  return { rpc, written };
}

test("a response settles the matching request", async () => {
  const { rpc, written } = peer();
  const result = rpc.request<{ ok: boolean }>("initialize", { a: 1 });
  expect(written[0]).toEqual({ id: 1, method: "initialize", params: { a: 1 } });
  expect(
    rpc.receive(JSON.stringify({ id: 1, result: { ok: true } })),
  ).toBeNull();
  expect(await result).toEqual({ ok: true });
});

test("an error response rejects the request", async () => {
  const { rpc } = peer();
  const result = rpc.request("thread/start");
  rpc.receive(JSON.stringify({ id: 1, error: { code: -1, message: "bad" } }));
  await expect(result).rejects.toThrow("bad");
});

test("notifications and server requests are handed to the caller", () => {
  const { rpc } = peer();
  expect(
    rpc.receive(JSON.stringify({ method: "turn/started", params: { x: 1 } })),
  ).toEqual({
    kind: RpcInboundKind.Notification,
    method: "turn/started",
    params: { x: 1 },
  });
  expect(
    rpc.receive(
      JSON.stringify({
        id: "s1",
        method: "item/fileChange/requestApproval",
        params: {},
      }),
    ),
  ).toEqual({
    kind: RpcInboundKind.Request,
    id: "s1",
    method: "item/fileChange/requestApproval",
    params: {},
  });
});

test("respond and respondError reuse the server's id", () => {
  const { rpc, written } = peer();
  rpc.respond("s1", { decision: "accept" });
  rpc.respondError(2, -32601, "nope");
  expect(written).toEqual([
    { id: "s1", result: { decision: "accept" } },
    { id: 2, error: { code: -32601, message: "nope" } },
  ]);
});

test("garbage lines are ignored and close rejects in-flight requests", async () => {
  const { rpc } = peer();
  expect(rpc.receive("not json")).toBeNull();
  const result = rpc.request("x");
  rpc.close(new Error("exited"));
  await expect(result).rejects.toThrow("exited");
});

test("an error's data.details is appended to its message", async () => {
  const { rpc } = peer();
  const result = rpc.request("session/new");
  rpc.receive(
    JSON.stringify({
      id: 1,
      error: {
        code: -32603,
        message: "Internal error",
        data: { details: "No LLM provider configured" },
      },
    }),
  );
  await expect(result).rejects.toThrow(
    "Internal error: No LLM provider configured",
  );
});

test("a versioned peer stamps jsonrpc on every message", () => {
  const written: Array<Record<string, unknown>> = [];
  const rpc = new RpcPeer((line) => written.push(JSON.parse(line)), true);
  void rpc.request("initialize");
  rpc.notify("initialized");
  rpc.respond(7, { ok: true });
  expect(written.map((message) => message.jsonrpc)).toEqual([
    "2.0",
    "2.0",
    "2.0",
  ]);
});
