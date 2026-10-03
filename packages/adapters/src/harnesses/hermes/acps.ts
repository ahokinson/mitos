import {
  AcpStream,
  outcomeFor,
  type PendingPermission,
  permissionRequest,
  REQUEST_PERMISSION,
  SESSION_UPDATE,
} from "@harnesses/hermes/streams.ts";
import { asRecord } from "@json/records.ts";
import { stringValue } from "@json/scalars.ts";
import type { AnswerMessage } from "@protocol/asks.ts";
import { emitEvent, readAnswers, readLines } from "@protocol/channels.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";
import { RpcInboundKind, RpcPeer } from "@rpc/peers.ts";

const PROTOCOL_VERSION = 1;
const CLIENT_INFO = { name: "mitos", title: "Mitos", version: "0.1.0" };
const METHOD_NOT_FOUND = -32601;
const EXITED = "hermes acp exited";
const DIAGNOSTIC_WAIT_MS = 200;
const ERROR_LINE = /\b(ERROR|CRITICAL)\b/;

export type TurnOptions = {
  workdir: string;
  text: string;
  session: string | null;
  turnId?: string;
};

/** Newline-delimited JSON-RPC over a child's stdio. */
export type Link = {
  write(line: string): void;
  lines: AsyncIterable<string>;
};

type Io = {
  answers: AsyncIterable<AnswerMessage>;
  emit: (event: AdapterEvent) => void;
  /** Why the child died, when it left no better message. */
  diagnostic?: () => Promise<string>;
};

type AgentCapabilities = {
  loadSession: boolean;
  resumeSession: boolean;
};

/** Runs one turn over `hermes acp`: starts or resumes the session, streams
 * the turn as adapter events, and relays host answers to permission
 * requests. Emits exactly one terminal event. */
export async function runTurn(options: TurnOptions): Promise<void> {
  const proc = Bun.spawn(["hermes", "acp"], {
    cwd: options.workdir,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = new Response(proc.stderr).text();
  const link: Link = {
    write: (line) => {
      proc.stdin.write(`${line}\n`);
      proc.stdin.flush();
    },
    lines: readLines(proc.stdout),
  };
  try {
    await driveTurn(link, options, {
      answers: readAnswers(),
      emit: emitEvent,
      diagnostic: () =>
        Promise.race([
          stderr.then(lastErrorLine),
          Bun.sleep(DIAGNOSTIC_WAIT_MS).then(() => ""),
        ]),
    });
  } finally {
    proc.stdin.end();
    proc.kill();
    await proc.exited;
  }
}

function lastErrorLine(log: string): string {
  const lines = log.split("\n").filter((line) => ERROR_LINE.test(line));
  return lines.at(-1)?.trim() ?? "";
}

/** Drives one turn over an open ACP link. Transport-only: the caller owns
 * the child process. */
export async function driveTurn(
  link: Link,
  options: TurnOptions,
  io: Io,
): Promise<void> {
  const rpc = new RpcPeer(link.write, true);
  let terminated = false;
  const emit = (event: AdapterEvent) => {
    if (terminated) return;
    if (
      event.event === AdapterEventKind.TurnComplete ||
      event.event === AdapterEventKind.Error
    )
      terminated = true;
    io.emit(event);
  };

  const pending = new Map<string, PendingPermission>();
  const session: { stream: AcpStream | null } = { stream: null };

  void relayAnswers(rpc, pending, io.answers);

  // Updates that arrive before the session is ready are history replayed by
  // `session/load`, not part of this turn.
  void (async () => {
    for await (const line of link.lines) {
      const inbound = rpc.receive(line);
      if (!inbound) continue;
      if (inbound.kind === RpcInboundKind.Notification) {
        if (inbound.method === SESSION_UPDATE && session.stream)
          for (const event of session.stream.update(inbound.params))
            emit(event);
        continue;
      }
      const request =
        inbound.method === REQUEST_PERMISSION && session.stream
          ? permissionRequest(inbound.id, inbound.params, options.turnId)
          : null;
      if (!request) {
        rpc.respondError(
          inbound.id,
          METHOD_NOT_FOUND,
          `Mitos cannot answer ${inbound.method}`,
        );
        continue;
      }
      pending.set(String(inbound.id), request.pending);
      emit(request.event);
    }
  })().finally(() => rpc.close(new Error(EXITED)));

  try {
    const capabilities = capabilitiesOf(
      await rpc.request("initialize", {
        protocolVersion: PROTOCOL_VERSION,
        clientCapabilities: {},
        clientInfo: CLIENT_INFO,
      }),
    );
    const sessionId = await openSession(rpc, options, capabilities);
    emit({
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: options.turnId,
      native_session: sessionId,
    });
    const stream = new AcpStream(sessionId, options.turnId);
    session.stream = stream;

    const result = await rpc.request("session/prompt", {
      sessionId,
      prompt: [{ type: "text", text: options.text }],
    });
    for (const event of stream.finish(result)) emit(event);
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    const detail = message === EXITED ? ((await io.diagnostic?.()) ?? "") : "";
    emit({
      event: AdapterEventKind.Error,
      turn_id: options.turnId,
      content: detail ? `${message}: ${detail}` : message,
    });
  } finally {
    rpc.close(new Error("turn ended"));
  }
}

async function openSession(
  rpc: RpcPeer,
  options: TurnOptions,
  capabilities: AgentCapabilities,
): Promise<string> {
  const base = { cwd: options.workdir, mcpServers: [] };
  if (!options.session) {
    const created = asRecord(await rpc.request("session/new", base));
    const id = stringValue(created?.sessionId);
    if (!id) throw new Error("hermes did not return a session id");
    return id;
  }
  const params = { ...base, sessionId: options.session };
  if (capabilities.resumeSession) await rpc.request("session/resume", params);
  else if (capabilities.loadSession) await rpc.request("session/load", params);
  else throw new Error("hermes cannot resume sessions");
  return options.session;
}

function capabilitiesOf(result: unknown): AgentCapabilities {
  const agent = asRecord(asRecord(result)?.agentCapabilities);
  return {
    loadSession: agent?.loadSession === true,
    resumeSession: asRecord(agent?.sessionCapabilities)?.resume !== undefined,
  };
}

async function relayAnswers(
  rpc: RpcPeer,
  pending: Map<string, PendingPermission>,
  answers: AsyncIterable<AnswerMessage>,
): Promise<void> {
  for await (const answer of answers) {
    const request = pending.get(answer.request_id);
    if (!request) continue;
    pending.delete(answer.request_id);
    rpc.respond(request.rpcId, outcomeFor(request, answer.response));
  }
}
