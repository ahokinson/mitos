import type { Database } from "bun:sqlite";
import { join } from "node:path";
import { openReadonlyDatabase } from "@history/databases.ts";
import {
  emptyHandoff,
  type TranscriptMessage,
  transcriptHandoff,
} from "@history/handoffs.ts";
import { type JsonRecord, parsedRecord } from "@json/records.ts";
import { numberValue, stringValue } from "@json/scalars.ts";
import type { CollectHandoffRequest } from "@protocol/actions.ts";
import type { HandoffResponse, HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

const SOURCE = "OpenCode session transcript";
const DATABASE_NAMES = ["opencode-stable.db", "opencode.db"];
const RECENT_SESSIONS = 48;
const LAUNCH_SLACK_MS = 60_000;

export function collectOpenCodeHandoff(
  request: CollectHandoffRequest,
): HandoffResponse {
  const nativeSession = stringValue(request.native_session);
  const dataHome =
    process.env.XDG_DATA_HOME ??
    join(process.env.HOME ?? "", ".local", "share");
  for (const name of DATABASE_NAMES) {
    const database = openReadonlyDatabase(join(dataHome, "opencode", name));
    if (!database) continue;
    try {
      const handoff = readHandoff(database, request, nativeSession);
      if (handoff) return handoff;
    } catch {
      // A database without the expected tables is not this harness's.
    } finally {
      database.close();
    }
  }
  return emptyHandoff(nativeSession);
}

function readHandoff(
  database: Database,
  request: CollectHandoffRequest,
  nativeSession: string | null,
): HandoffResponse | null {
  const session = findSession(database, request, nativeSession);
  if (!session) return null;
  const sessionId = stringValue(session.id);
  if (!sessionId) return null;
  const messages = sessionMessages(database, sessionId);
  if (messages.length === 0) return emptyHandoff(sessionId);
  return transcriptHandoff(SOURCE, sessionId, messages, usageOf(session));
}

function findSession(
  database: Database,
  request: CollectHandoffRequest,
  nativeSession: string | null,
): JsonRecord | null {
  if (nativeSession) {
    return (
      (database
        .query("SELECT * FROM session WHERE id = ?")
        .get(nativeSession) as JsonRecord | null) ?? null
    );
  }
  const threshold = Date.parse(request.launched_at) - LAUNCH_SLACK_MS;
  const rows = database
    .query(
      "SELECT * FROM session WHERE directory = ? AND parent_id IS NULL ORDER BY time_updated DESC LIMIT ?",
    )
    .all(request.workdir, RECENT_SESSIONS) as JsonRecord[];
  return (
    rows.find((row) => (numberValue(row.time_updated) ?? 0) >= threshold) ??
    null
  );
}

function sessionMessages(
  database: Database,
  sessionId: string,
): TranscriptMessage[] {
  const messages = database
    .query(
      "SELECT id, data FROM message WHERE session_id = ? ORDER BY time_created, id",
    )
    .all(sessionId) as JsonRecord[];
  const parts = database
    .query(
      "SELECT message_id, data FROM part WHERE session_id = ? ORDER BY time_created, id",
    )
    .all(sessionId) as JsonRecord[];

  const textByMessage = new Map<string, string[]>();
  for (const row of parts) {
    const messageId = stringValue(row.message_id);
    const part = parsedRecord(row.data);
    if (!messageId || !part || part.type !== "text") continue;
    if (part.synthetic === true || part.ignored === true) continue;
    const text = stringValue(part.text);
    if (!text) continue;
    const texts = textByMessage.get(messageId) ?? [];
    texts.push(text);
    textByMessage.set(messageId, texts);
  }

  return messages.flatMap((row) => {
    const id = stringValue(row.id);
    const role = stringValue(parsedRecord(row.data)?.role);
    if (!id || (role !== MessageRole.User && role !== MessageRole.Assistant))
      return [];
    const text = textByMessage.get(id)?.join("\n").trim();
    return text ? [{ role, text }] : [];
  });
}

function usageOf(session: JsonRecord): HandoffUsage | null {
  const input = numberValue(session.tokens_input) ?? 0;
  const output = numberValue(session.tokens_output) ?? 0;
  const cached = numberValue(session.tokens_cache_read) ?? 0;
  const cost = numberValue(session.cost) ?? 0;
  const model = stringValue(parsedRecord(session.model)?.id);
  if (input === 0 && output === 0 && cached === 0 && cost === 0 && !model)
    return null;
  return {
    input_tokens: input,
    output_tokens: output,
    cached_input_tokens: cached,
    cost_usd: cost,
    model: model ?? undefined,
  };
}
