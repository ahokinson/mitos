import { PROTOCOL_VERSION } from "@protocol/actions.ts";
import {
  type HandoffResponse,
  type HandoffUsage,
  ResponseKind,
} from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

export type TranscriptMessage = { role: string; text: string };

export function emptyHandoff(nativeSession: string | null): HandoffResponse {
  return {
    protocol_version: PROTOCOL_VERSION,
    kind: ResponseKind.Handoff,
    native_session: nativeSession,
    transcript: null,
    usage: null,
  };
}

export function transcriptHandoff(
  source: string,
  sessionId: string,
  messages: TranscriptMessage[],
  usage: HandoffUsage | null,
): HandoffResponse {
  const turns = messages.filter(
    (message) => message.role === MessageRole.Assistant,
  ).length;
  return {
    protocol_version: PROTOCOL_VERSION,
    kind: ResponseKind.Handoff,
    native_session: sessionId,
    transcript: { source, messages, truncated: false },
    usage: usage ? { ...usage, turns } : turns > 0 ? { turns } : null,
  };
}

/** The last usage-bearing row wins, so a later turn's snapshot replaces an
 * earlier one. */
export function latestUsage<Row>(
  rows: readonly Row[],
  usageFromRow: (row: Row) => HandoffUsage | null,
): HandoffUsage | null {
  let usage: HandoffUsage | null = null;
  for (const row of rows) {
    const found = usageFromRow(row);
    if (found) usage = found;
  }
  return usage;
}
