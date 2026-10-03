import {
  agentFor,
  type PendingAsk,
  replyFor,
  SessionStream,
} from "@harnesses/opencode/streams.ts";
import { asRecord } from "@json/records.ts";
import { stringValue } from "@json/scalars.ts";
import type { ThreadMode } from "@protocol/actions.ts";
import type { AnswerMessage } from "@protocol/asks.ts";
import { emitEvent, readAnswers, readLines } from "@protocol/channels.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";

const SERVER_USER = "mitos";
const STARTUP_TIMEOUT_MS = 30_000;
const LISTENING = /listening on (https?:\/\/\S+)/;

export type TurnOptions = {
  workdir: string;
  text: string;
  session: string | null;
  mode: ThreadMode;
  turnId?: string;
};

export type ServerAccess = { base: string; password: string };

type Io = {
  answers: AsyncIterable<AnswerMessage>;
  emit: (event: AdapterEvent) => void;
};

/** Runs one turn over a private `opencode serve`: creates or resumes the
 * session, streams the turn as adapter events, and relays host answers to
 * permission and question asks. Emits exactly one terminal event. */
export async function runTurn(options: TurnOptions): Promise<void> {
  const password = crypto.randomUUID();
  const proc = Bun.spawn(
    ["opencode", "serve", "--port", "0", "--hostname", "127.0.0.1"],
    {
      cwd: options.workdir,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        OPENCODE_SERVER_USERNAME: SERVER_USER,
        OPENCODE_SERVER_PASSWORD: password,
      },
    },
  );
  const stderr = new Response(proc.stderr).text();
  try {
    const base = await listeningUrl(proc.stdout, proc.exited);
    await driveTurn({ base, password }, options, {
      answers: readAnswers(),
      emit: emitEvent,
    });
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    const diagnostic = (
      await Promise.race([stderr, Promise.resolve("")])
    ).trim();
    emitEvent({
      event: AdapterEventKind.Error,
      turn_id: options.turnId,
      content: diagnostic || message,
    });
  } finally {
    proc.kill();
    await proc.exited;
  }
}

/** Drives one turn against a running server. Transport-only: the caller owns
 * the server process. */
export async function driveTurn(
  access: ServerAccess,
  options: TurnOptions,
  io: Io,
): Promise<void> {
  const client = new ServerClient(access, options.workdir);
  const abort = new AbortController();
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

  try {
    const sessionId = options.session ?? (await client.createSession());
    emit({
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: options.turnId,
      native_session: sessionId,
    });

    // Subscribe before prompting so no early event is missed.
    const events = await client.subscribe(abort.signal);
    await client.post(`/session/${sessionId}/prompt_async`, {
      agent: agentFor(options.mode),
      parts: [{ type: "text", text: options.text }],
    });

    const stream = new SessionStream(sessionId, options.turnId);
    const pending = new Map<string, PendingAsk>();
    void relayAnswers(client, pending, io.answers);

    for await (const raw of events) {
      const step = stream.process(raw);
      for (const event of step.events) {
        if (event.event === AdapterEventKind.Request) {
          const ask = stream.pendingFor(event);
          if (ask) pending.set(ask.id, ask);
        }
        emit(event);
      }
      if (step.done) return;
    }
    emit({
      event: AdapterEventKind.Error,
      turn_id: options.turnId,
      content: "opencode event stream ended before the turn completed",
    });
  } catch (cause) {
    emit({
      event: AdapterEventKind.Error,
      turn_id: options.turnId,
      content: cause instanceof Error ? cause.message : String(cause),
    });
  } finally {
    abort.abort();
  }
}

async function relayAnswers(
  client: ServerClient,
  pending: Map<string, PendingAsk>,
  answers: AsyncIterable<AnswerMessage>,
): Promise<void> {
  for await (const answer of answers) {
    const ask = pending.get(answer.request_id);
    if (!ask) continue;
    pending.delete(answer.request_id);
    const reply = replyFor(ask, answer.response);
    await client.post(reply.path, reply.body).catch(() => undefined);
  }
}

class ServerClient {
  private readonly headers: Record<string, string>;

  constructor(
    private readonly access: ServerAccess,
    private readonly directory: string,
  ) {
    this.headers = {
      authorization: `Basic ${btoa(`${SERVER_USER}:${access.password}`)}`,
    };
  }

  async createSession(): Promise<string> {
    const created = asRecord(await this.post("/session", {}));
    const id = stringValue(created?.id);
    if (!id) throw new Error("opencode did not return a session id");
    return id;
  }

  async post(path: string, body: unknown): Promise<unknown> {
    const response = await fetch(this.url(path), {
      method: "POST",
      headers: { ...this.headers, "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!response.ok)
      throw new Error(
        `opencode ${path} failed: ${response.status} ${(await response.text()).trim()}`,
      );
    const text = await response.text();
    return text ? (JSON.parse(text) as unknown) : null;
  }

  /** Resolves once the stream is open, then yields each SSE `data:` payload. */
  async subscribe(signal: AbortSignal): Promise<AsyncGenerator<unknown>> {
    const response = await fetch(this.url("/event"), {
      headers: this.headers,
      signal,
    });
    if (!response.ok || !response.body)
      throw new Error(`opencode /event failed: ${response.status}`);
    return sseEvents(response.body);
  }

  private url(path: string): string {
    return `${this.access.base}${path}?directory=${encodeURIComponent(this.directory)}`;
  }
}

async function* sseEvents(
  body: ReadableStream<Uint8Array>,
): AsyncGenerator<unknown> {
  let data: string[] = [];
  try {
    for await (const line of readLines(body)) {
      if (line === "") {
        if (data.length > 0) {
          const payload = data.join("\n");
          data = [];
          try {
            yield JSON.parse(payload) as unknown;
          } catch {
            // A malformed frame must not end the turn.
          }
        }
      } else if (line.startsWith("data:")) {
        data.push(line.slice(5).trimStart());
      }
    }
  } catch {
    // The stream is aborted when the turn ends.
  }
}

async function listeningUrl(
  stdout: ReadableStream<Uint8Array>,
  exited: Promise<number>,
): Promise<string> {
  const lines = readLines(stdout);
  const found = (async () => {
    for (;;) {
      const next = await lines.next();
      if (next.done) throw new Error("opencode serve exited before listening");
      const match = LISTENING.exec(next.value);
      if (match?.[1]) {
        // Keep draining so the child never blocks on a full pipe.
        void (async () => {
          for await (const _ of lines) {
            // discard
          }
        })();
        return match[1];
      }
    }
  })();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error("opencode serve did not start in time")),
      STARTUP_TIMEOUT_MS,
    );
  });
  const early = exited.then((code) => {
    throw new Error(`opencode serve exited with code ${code}`);
  });
  try {
    return await Promise.race([found, timeout, early]);
  } finally {
    clearTimeout(timer);
  }
}
