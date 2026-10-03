import {
  type AdapterRequest,
  INITIAL_ACTIONS,
  PROTOCOL_VERSION,
  ReplyAction,
} from "@protocol/actions.ts";
import type { AnswerMessage } from "@protocol/asks.ts";
import type { AdapterEvent } from "@protocol/events.ts";
import type {
  HandoffResponse,
  LaunchResponse,
  NegotiateResponse,
} from "@protocol/responses.ts";

let stdinLinesSingleton: AsyncGenerator<string> | undefined;

function stdinLines(): AsyncGenerator<string> {
  stdinLinesSingleton ??= readLines(Bun.stdin.stream());
  return stdinLinesSingleton;
}

/** Reads the first stdin line only; stdin stays open for `readAnswers`. */
export async function readRequest(): Promise<AdapterRequest> {
  const first = await stdinLines().next();
  if (first.done) throw new Error("no Mitos adapter request on stdin");
  const request = JSON.parse(first.value) as AdapterRequest;
  if (request.protocol_version !== PROTOCOL_VERSION) {
    throw new Error("unsupported Mitos adapter request");
  }
  if (!INITIAL_ACTIONS.includes(request.action)) {
    throw new Error("unsupported Mitos adapter request");
  }
  return request;
}

/** Answer lines from core, until it closes stdin at the end of the turn. */
export async function* readAnswers(): AsyncGenerator<AnswerMessage> {
  for await (const line of stdinLines()) {
    if (!line.trim()) continue;
    const message = JSON.parse(line) as { action?: string };
    if (message.action === ReplyAction.Answer) yield message as AnswerMessage;
  }
}

export function respond(
  response: LaunchResponse | HandoffResponse | NegotiateResponse,
): void {
  process.stdout.write(JSON.stringify(response));
}

export function emitEvent(event: AdapterEvent): void {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

/** Splits a Bun/web `ReadableStream<Uint8Array>` into text lines as they
 * arrive, so a child's NDJSON output is parsed while it is still running. */
export async function* readLines(
  stream: ReadableStream<Uint8Array>,
): AsyncGenerator<string> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of stream) {
    buffer += decoder.decode(chunk, { stream: true });
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex !== -1) {
      yield buffer.slice(0, newlineIndex);
      buffer = buffer.slice(newlineIndex + 1);
      newlineIndex = buffer.indexOf("\n");
    }
  }
  if (buffer.length > 0) yield buffer;
}
