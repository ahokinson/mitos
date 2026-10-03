import { findString, isObject } from "@json/records.ts";
import { stringValue } from "@json/scalars.ts";
import { textValue } from "@json/texts.ts";
import { MessageRole } from "@protocol/roles.ts";

// Must match the preamble in crates/mitos/src/handoff/renderers.rs.
const HANDOFF_PREAMBLE_PREFIX = "You are joining Mitos session ";

export type MinedMessage = { role: string; text: string };

export function messagesFromRecord(record: unknown): MinedMessage[] {
  if (!isObject(record)) return [];
  const payload = isObject(record.payload) ? record.payload : record;
  const message = isObject(payload.message) ? payload.message : {};
  const role = stringValue(payload.role) ?? stringValue(message.role);
  if (role !== MessageRole.User && role !== MessageRole.Assistant) return [];
  const content =
    payload.content ?? message.content ?? payload.text ?? message.text;
  const text = textValue(content);
  if (!text) return [];
  if (role === MessageRole.User && text.startsWith(HANDOFF_PREAMBLE_PREFIX))
    return [];
  return [{ role, text }];
}

export function matchesWorkspace(records: unknown[], workdir: string): boolean {
  return records.some(
    (record) => findString(record, ["cwd", "workdir", "workspace"]) === workdir,
  );
}
