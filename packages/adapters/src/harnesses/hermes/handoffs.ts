import type { Database } from "bun:sqlite";
import { join } from "node:path";
import { openReadonlyDatabase } from "@history/databases.ts";
import {
  emptyHandoff,
  type TranscriptMessage,
  transcriptHandoff,
} from "@history/handoffs.ts";
import type { JsonRecord } from "@json/records.ts";
import { numberValue, stringValue } from "@json/scalars.ts";
import type { CollectHandoffRequest } from "@protocol/actions.ts";
import type { HandoffResponse, HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

const SOURCE = "Hermes session transcript";
const RECENT_SESSIONS = 24;
const LAUNCH_SLACK_MS = 60_000;

export function collectHermesHandoff(
  request: CollectHandoffRequest,
): HandoffResponse {
  const home =
    process.env.HERMES_HOME ?? join(process.env.HOME ?? "", ".hermes");
  const requestedSession = stringValue(request.native_session);
  const database = openReadonlyDatabase(join(home, "state.db"));
  if (!database) return emptyHandoff(requestedSession);
  try {
    const session = findSession(database, request, requestedSession);
    const sessionId = stringValue(session?.id);
    if (!session || !sessionId) return emptyHandoff(requestedSession);
    const messages = sessionMessages(database, sessionId);
    if (messages.length === 0) return emptyHandoff(sessionId);
    return transcriptHandoff(SOURCE, sessionId, messages, usageOf(session));
  } catch {
    return emptyHandoff(requestedSession);
  } finally {
    database.close();
  }
}

function findSession(
  database: Database,
  request: CollectHandoffRequest,
  requestedSession: string | null,
): JsonRecord | null {
  if (requestedSession) {
    return (
      (database
        .query("SELECT * FROM sessions WHERE id = ?")
        .get(requestedSession) as JsonRecord | null) ?? null
    );
  }
  const threshold = Date.parse(request.launched_at) - LAUNCH_SLACK_MS;
  const rows = database
    .query(
      "SELECT * FROM sessions WHERE cwd = ? AND archived = 0 AND hidden = 0 ORDER BY started_at DESC LIMIT ?",
    )
    .all(request.workdir, RECENT_SESSIONS) as JsonRecord[];
  return rows.find((row) => touchedAtMs(row) >= threshold) ?? null;
}

/** Timestamps are epoch seconds. */
function touchedAtMs(session: JsonRecord): number {
  const seconds =
    numberValue(session.last_activity_at) ??
    numberValue(session.ended_at) ??
    numberValue(session.started_at) ??
    0;
  return seconds * 1000;
}

function sessionMessages(
  database: Database,
  sessionId: string,
): TranscriptMessage[] {
  const rows = database
    .query(
      "SELECT role, content FROM messages WHERE session_id = ? ORDER BY id",
    )
    .all(sessionId) as JsonRecord[];
  return rows.flatMap((row) => {
    const role = stringValue(row.role);
    const text = stringValue(row.content)?.trim();
    if (role !== MessageRole.User && role !== MessageRole.Assistant) return [];
    return text ? [{ role, text }] : [];
  });
}

function usageOf(session: JsonRecord): HandoffUsage | null {
  const input = numberValue(session.input_tokens) ?? 0;
  const output = numberValue(session.output_tokens) ?? 0;
  const cached = numberValue(session.cache_read_tokens) ?? 0;
  const cost =
    numberValue(session.actual_cost_usd) ??
    numberValue(session.estimated_cost_usd);
  const model = stringValue(session.model);
  if (input === 0 && output === 0 && cached === 0 && cost === null && !model)
    return null;
  return {
    input_tokens: input,
    output_tokens: output,
    cached_input_tokens: cached,
    cost_usd: cost ?? undefined,
    model: model ?? undefined,
  };
}
