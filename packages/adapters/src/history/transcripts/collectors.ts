import { basename } from "node:path";

import { type Candidate, discoverFiles } from "@history/files.ts";
import { emptyHandoff } from "@history/handoffs.ts";
import {
  matchesWorkspace,
  messagesFromRecord,
} from "@history/transcripts/messages.ts";
import { usageFromRecords } from "@history/transcripts/usage.ts";
import { findString } from "@json/records.ts";
import { stringValue } from "@json/scalars.ts";
import {
  type CollectHandoffRequest,
  PROTOCOL_VERSION,
} from "@protocol/actions.ts";
import { type HandoffResponse, ResponseKind } from "@protocol/responses.ts";

const MAX_FILES = 24;

export async function collectJsonlHandoff(
  request: CollectHandoffRequest,
  source: string,
  root: string,
): Promise<HandoffResponse> {
  const nativeSession = stringValue(request.native_session);
  const candidates = await findCandidates(
    root,
    nativeSession,
    request.launched_at,
  );
  for (const candidate of candidates) {
    const records = parseJsonLines(await Bun.file(candidate.path).text());
    if (!matchesWorkspace(records, request.workdir) && !nativeSession) continue;
    const messages = records.flatMap(messagesFromRecord);
    if (messages.length === 0) continue;
    return {
      protocol_version: PROTOCOL_VERSION,
      kind: ResponseKind.Handoff,
      native_session:
        nativeSession ??
        sessionFromRecords(records) ??
        sessionFromPath(candidate.path),
      transcript: { source, messages, truncated: false },
      usage: usageFromRecords(records, messages),
    };
  }
  return emptyHandoff(nativeSession);
}

async function findCandidates(
  root: string,
  nativeSession: string | null,
  launchedAt: string,
): Promise<Candidate[]> {
  const threshold = Date.parse(launchedAt) - 60_000;
  const candidates = await discoverFiles(root, ".jsonl", MAX_FILES * 8);
  return candidates
    .filter((candidate) =>
      nativeSession
        ? candidate.path.includes(nativeSession)
        : candidate.modified >= threshold,
    )
    .sort((left, right) => right.modified - left.modified)
    .slice(0, MAX_FILES);
}

function parseJsonLines(text: string): unknown[] {
  return text.split("\n").flatMap((line) => {
    try {
      return [JSON.parse(line) as unknown];
    } catch {
      return [];
    }
  });
}

function sessionFromRecords(records: unknown[]): string | null {
  for (const record of records) {
    const session = findString(record, [
      "thread_id",
      "session_id",
      "sessionId",
      "id",
    ]);
    if (session) return session;
  }
  return null;
}

function sessionFromPath(path: string): string | null {
  const name = basename(path, ".jsonl");
  const match = name.match(/[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}/i);
  return match?.[0] ?? null;
}
