import {
  type PendingServerRequest,
  processNotification,
  processTurnCompleted,
  requestFromServer,
  resultFor,
  TurnTokens,
  threadSandbox,
  turnPolicy,
} from "@harnesses/codex/streams.ts";
import type { ThreadMode } from "@protocol/actions.ts";
import { emitEvent, readAnswers, readLines } from "@protocol/channels.ts";
import { AdapterEventKind } from "@protocol/events.ts";
import { RpcInboundKind, RpcPeer } from "@rpc/peers.ts";

const CLIENT_INFO = { name: "mitos", title: "Mitos", version: "0.1.0" };
const METHOD_NOT_FOUND = -32601;

/** Runs one turn over `codex app-server`: starts or resumes the thread,
 * streams the turn as adapter events, and relays host answers back to
 * server-initiated requests. Emits exactly one terminal event. */
export async function runTurn(options: {
  workdir: string;
  text: string;
  session: string | null;
  mode: ThreadMode;
  turnId?: string;
}): Promise<void> {
  const { turnId } = options;
  const proc = Bun.spawn(["codex", "app-server"], {
    cwd: options.workdir,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = new Response(proc.stderr).text();
  const rpc = new RpcPeer((line) => {
    proc.stdin.write(`${line}\n`);
    proc.stdin.flush();
  });
  const pending = new Map<string, PendingServerRequest>();
  const tokens = new TurnTokens();
  let finished = false;

  // Ends when core closes our stdin after the terminal event.
  void (async () => {
    for await (const answer of readAnswers()) {
      const request = pending.get(answer.request_id);
      if (!request) continue;
      pending.delete(answer.request_id);
      rpc.respond(request.rpcId, resultFor(request, answer.response));
    }
  })();

  const reading = (async () => {
    for await (const line of readLines(proc.stdout)) {
      const inbound = rpc.receive(line);
      if (!inbound) continue;
      if (inbound.kind === RpcInboundKind.Notification) {
        if (inbound.method === "turn/completed") {
          emitEvent(processTurnCompleted(inbound.params, turnId));
          finished = true;
          return;
        }
        for (const event of processNotification(
          inbound.method,
          inbound.params,
          turnId,
          tokens,
        ))
          emitEvent(event);
        continue;
      }
      const request = requestFromServer(
        inbound.id,
        inbound.method,
        inbound.params,
        turnId,
      );
      if (!request) {
        rpc.respondError(
          inbound.id,
          METHOD_NOT_FOUND,
          `Mitos cannot answer ${inbound.method}`,
        );
        continue;
      }
      pending.set(String(inbound.id), request.pending);
      emitEvent(request.event);
    }
  })().finally(() => rpc.close(new Error("codex app-server exited")));

  try {
    await rpc.request("initialize", { clientInfo: CLIENT_INFO });
    rpc.notify("initialized");

    const policy = turnPolicy(options.mode);
    const threadParams = {
      cwd: options.workdir,
      approvalPolicy: policy.approvalPolicy,
      sandbox: threadSandbox(options.mode),
    };
    const started = options.session
      ? await rpc.request("thread/resume", {
          threadId: options.session,
          excludeTurns: true,
          ...threadParams,
        })
      : await rpc.request("thread/start", threadParams);
    const threadId = (started as { thread?: { id?: unknown } } | undefined)
      ?.thread?.id;
    if (typeof threadId !== "string")
      throw new Error("codex did not return a thread id");
    emitEvent({
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: turnId,
      native_session: threadId,
    });

    tokens.begin();
    await rpc.request("turn/start", {
      threadId,
      input: [{ type: "text", text: options.text }],
      cwd: options.workdir,
      ...policy,
    });
    await reading;
    if (!finished) {
      const diagnostic = (await stderr).trim();
      emitEvent({
        event: AdapterEventKind.Error,
        turn_id: turnId,
        content: diagnostic || "codex ended without completing the turn",
      });
    }
  } catch (cause) {
    const diagnostic = (
      await Promise.race([stderr, Promise.resolve("")])
    ).trim();
    const message = cause instanceof Error ? cause.message : String(cause);
    emitEvent({
      event: AdapterEventKind.Error,
      turn_id: turnId,
      content: diagnostic || message,
    });
  } finally {
    proc.stdin.end();
    proc.kill();
    await proc.exited;
  }
}
